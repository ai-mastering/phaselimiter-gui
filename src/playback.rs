//! FFmpeg decodes into bounded-memory, disposable PCM files; miniaudio plays them.
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

const RATE: u64 = 44100;
#[derive(Debug)]
pub struct Decoded {
    pub path: PathBuf,
    pub frames: u64,
    _directory: tempfile::TempDir,
    source_len: u64,
    source_modified: Option<std::time::SystemTime>,
}

pub async fn decode(
    input: PathBuf,
    expected: String,
    ffmpeg: String,
) -> Result<Arc<Decoded>, String> {
    crate::model::validate_tool_path(std::path::Path::new(&ffmpeg), "FFmpeg")?;
    let checked = input.clone();
    tokio::task::spawn_blocking(move || crate::library::verify(&checked, &expected))
        .await
        .map_err(|e| e.to_string())??;
    let metadata = std::fs::metadata(&input).map_err(|e| e.to_string())?;
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = directory.path().join("preview.f32");
    let mut command = tokio::process::Command::new(ffmpeg);
    command
        .arg("-v")
        .arg("error")
        .arg("-nostdin")
        .arg("-i")
        .arg(input)
        .args(["-vn", "-ac", "2", "-ar", "44100", "-f", "f32le"])
        .arg(&path)
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(300), command.output())
        .await
        .map_err(|_| "Preparing playback timed out.".to_string())?
        .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Preparing playback: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let frames = std::fs::metadata(&path).map_err(|e| e.to_string())?.len() / 8;
    if frames == 0 {
        return Err("Audio contains no playable samples.".into());
    }
    Ok(Arc::new(Decoded {
        path,
        frames,
        _directory: directory,
        source_len: metadata.len(),
        source_modified: metadata.modified().ok(),
    }))
}

impl Decoded {
    pub fn unchanged(&self, path: &std::path::Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| {
            m.len() == self.source_len
                && self.source_modified.is_some()
                && m.modified().ok() == self.source_modified
        })
    }
}

mod mixer;
mod reader;

#[derive(Default)]
pub struct Player {
    engine: mixer::Engine,
    device: Option<native::PlaybackDevice>,
}
// Keep diagnostics off the audio callback. These entries also survive a native crash.
pub fn log(message: &str) {
    #[cfg(not(test))]
    if let Ok(root) = crate::library::default_root() {
        use std::io::Write;
        let path = root.join("audio-diagnostics.log");
        let truncate = std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(!truncate)
            .truncate(truncate)
            .open(path)
        {
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = writeln!(file, "{time}: {message}");
        }
    }
    #[cfg(test)]
    let _ = message;
}

mod native;

impl Player {
    pub fn load(&mut self, audio: Arc<Decoded>, gain: f32) -> Result<(), String> {
        self.engine.send(mixer::Command::Load(audio, gain));
        Ok(())
    }
    pub fn warm(&self, audio: Arc<Decoded>) {
        self.engine.send(mixer::Command::Warm(audio));
    }
    pub fn play(&mut self) -> Result<(), String> {
        self.engine.ready()?;
        if self.device.is_none() {
            let device =
                native::PlaybackDevice::open(&mut self.engine.output, self.engine.shared.clone())
                    .inspect_err(|error| log(error))?;
            // Retain the device/queue on start failure so Play can retry.
            self.device = Some(device);
        }
        if let Some(device) = &self.device {
            device.start().inspect_err(|error| log(error))?;
        }

        let (position, duration, _) = self.status();
        if duration > 0. && position >= duration {
            self.seek(0.);
        }
        self.engine.shared.playing.store(true, Ordering::Release);
        log("Playback enabled");
        Ok(())
    }
    pub fn pause(&self) {
        self.engine.shared.playing.store(false, Ordering::Release);
    }
    pub fn seek(&self, seconds: f64) {
        self.engine
            .send(mixer::Command::Seek((seconds.max(0.) * RATE as f64) as u64));
    }
    pub fn gain(&self, gain: f32) {
        self.engine.send(mixer::Command::Gain(gain));
    }
    pub fn region(&self, region: Option<(f64, f64)>) {
        let region = region.and_then(|(a, b)| {
            let start = (a.max(0.) * RATE as f64) as u64;
            let end = (b.max(0.) * RATE as f64) as u64;
            (end > start).then_some((start, end))
        });
        self.engine.send(mixer::Command::Region(region));
    }
    pub fn status(&self) -> (f64, f64, bool) {
        let s = &self.engine.shared;
        (
            s.position.load(Ordering::Acquire) as f64 / RATE as f64,
            s.duration.load(Ordering::Acquire) as f64 / RATE as f64,
            s.playing.load(Ordering::Acquire),
        )
    }
    pub fn take_error(&self) -> Option<String> {
        self.engine.error()
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.device.take();
    }
}

#[cfg(test)]
pub(crate) fn test_audio(samples: &[[f32; 2]], source: Option<&std::path::Path>) -> Arc<Decoded> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audio.f32");
    let bytes: Vec<_> = samples
        .iter()
        .flatten()
        .flat_map(|s| s.to_le_bytes())
        .collect();
    std::fs::write(&path, bytes).unwrap();
    let metadata = std::fs::metadata(source.unwrap_or(&path)).unwrap();
    Arc::new(Decoded {
        path,
        frames: samples.len() as u64,
        _directory: directory,
        source_len: metadata.len(),
        source_modified: metadata.modified().ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejects_command_names_and_relative_ffmpeg_paths_before_decoding() {
        for ffmpeg in ["ffmpeg", "./ffmpeg"] {
            let result = decode("unused.wav".into(), "unused hash".into(), ffmpeg.into()).await;
            assert!(result.unwrap_err().contains("FFmpeg: use an absolute path"));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "Run explicitly with an ALSA null-device configuration"]
    fn device_callback_keeps_running_while_sources_switch() {
        let audio = test_audio(&vec![[0.01, -0.01]; 44100 * 30], None);
        let other = test_audio(&vec![[0.02, -0.02]; 44100 * 30], None);
        let mut player = Player::default();
        player.load(audio, 1.).unwrap();
        player.warm(other.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while player.status().1 == 0. {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        player.play().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let before = player.status().0;
        assert!(before > 0.);
        assert!(player.status().2);
        player.load(other, 0.5).unwrap();
        assert!(player.status().2);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(player.status().0 > before);
        assert!(player.status().2);
        assert!(player.take_error().is_none());
        player.pause();
        assert!(!player.status().2);
    }
}
