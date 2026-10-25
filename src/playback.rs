//! FFmpeg decodes into bounded-memory, disposable PCM files; miniaudio plays them.
use miniaudio::{Device, DeviceConfig, DeviceType, Format};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
};

const RATE: u64 = 44100;
#[derive(Debug)]
pub struct Decoded {
    pub path: PathBuf,
    pub frames: u64,
    _directory: tempfile::TempDir,
}

pub async fn decode(
    input: PathBuf,
    expected: String,
    ffmpeg: String,
) -> Result<Arc<Decoded>, String> {
    let checked = input.clone();
    tokio::task::spawn_blocking(move || crate::library::verify(&checked, &expected))
        .await
        .map_err(|e| e.to_string())??;
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
    }))
}

#[derive(Default)]
struct State {
    file: Option<BufReader<File>>,
    audio: Option<Arc<Decoded>>,
    position: u64,
    playing: bool,
    gain: f32,
    region: Option<(u64, u64)>,
    error: Option<String>,
}
impl State {
    fn seek(&mut self, frame: u64) {
        self.position = frame.min(self.audio.as_ref().map_or(0, |a| a.frames));
        if let Some(file) = &mut self.file
            && let Err(e) = file.seek(SeekFrom::Start(self.position * 8))
        {
            self.error = Some(format!("Seeking audio: {e}"));
            self.playing = false;
        }
    }
    fn render(&mut self, output: &mut [f32]) {
        output.fill(0.0);
        if !self.playing {
            return;
        }
        for frame in output.as_chunks_mut::<2>().0 {
            if let Some((start, end)) = self.region
                && self.position >= end
            {
                self.seek(start);
            }
            if self.position >= self.audio.as_ref().map_or(0, |a| a.frames) {
                self.playing = false;
                break;
            }
            let mut bytes = [0u8; 8];
            if let Some(file) = &mut self.file {
                if let Err(e) = file.read_exact(&mut bytes) {
                    self.error = Some(format!("Reading stored playback audio: {e}"));
                    self.playing = false;
                    break;
                }
                for (sample, bytes) in frame.iter_mut().zip(bytes.as_chunks::<4>().0) {
                    *sample = f32::from_le_bytes(*bytes) * self.gain;
                }
                self.position += 1;
            }
        }
    }
}

#[derive(Default)]
pub struct Player {
    state: Arc<Mutex<State>>,
    device: Option<Device>,
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

fn open_device(
    config: &DeviceConfig,
    backend: Option<miniaudio::Backend>,
) -> Result<Device, String> {
    // With a supplied context, ma_device_init always clears the device before
    // returning an error. ma_device_init_ex (the implicit-context path) can fail
    // before touching it, but the Rust wrapper reads pUserData even on failure.
    let context = if let Some(backend) = backend {
        log("Initializing audio context");
        Some(
            miniaudio::Context::new(&[backend], None)
                .map_err(|e| format!("Audio backend initialization: {e}"))?,
        )
    } else {
        None
    };
    log("Initializing playback device");
    Device::new(context, config).map_err(|e| format!("Audio device: {e}"))
}

impl Player {
    pub fn load(&mut self, audio: Arc<Decoded>, playing: bool) -> Result<(), String> {
        let file = File::open(&audio.path).map_err(|e| e.to_string())?;
        let mut state = self.state.lock().unwrap();
        let position = state.position;
        state.file = Some(BufReader::new(file));
        state.audio = Some(audio);
        state.seek(position);
        state.playing = false;
        drop(state);
        if playing {
            self.play()?;
        }
        Ok(())
    }
    pub fn play(&mut self) -> Result<(), String> {
        if self.device.is_none() {
            log("Play requested: configuring F32 stereo at 44100 Hz");
            let mut config = DeviceConfig::new(DeviceType::Playback);
            config.playback_mut().set_format(Format::F32);
            config.playback_mut().set_channels(2);
            config.set_sample_rate(RATE as u32);
            let state = self.state.clone();
            #[cfg(target_os = "macos")]
            let backend = Some(miniaudio::Backend::CoreAudio);
            #[cfg(not(target_os = "macos"))]
            let backend = None;
            let mut device = open_device(&config, backend).inspect_err(|error| log(error))?;
            log("Playback device initialized; installing callback");
            device.set_data_callback(move |_, output, _| {
                if let Ok(mut state) = state.lock() {
                    state.render(output.as_samples_mut::<f32>());
                }
            });
            log("Starting playback device");
            device
                .start()
                .map_err(|e| format!("Starting audio: {e}"))
                .inspect_err(|error| log(error))?;
            log("Playback device started");
            self.device = Some(device);
        }
        let mut state = self.state.lock().unwrap();
        if state.audio.is_none() {
            return Err("Select an audio file first.".into());
        }
        if state.position >= state.audio.as_ref().map_or(0, |a| a.frames) {
            state.seek(0);
        }
        if let Some((start, end)) = state.region
            && (state.position < start || state.position >= end)
        {
            state.seek(start);
        }
        state.playing = true;
        log("Playback enabled");
        Ok(())
    }
    pub fn pause(&self) {
        self.state.lock().unwrap().playing = false;
    }
    pub fn seek(&self, seconds: f64) {
        self.state
            .lock()
            .unwrap()
            .seek((seconds.max(0.0) * RATE as f64) as u64);
    }
    pub fn gain(&self, gain: f32) {
        self.state.lock().unwrap().gain = gain;
    }
    pub fn region(&self, region: Option<(f64, f64)>) {
        let mut state = self.state.lock().unwrap();
        let frames = state.audio.as_ref().map_or(u64::MAX, |a| a.frames);
        state.region = region.and_then(|(a, b)| {
            let start = (a.max(0.0) * RATE as f64) as u64;
            let end = ((b.max(0.0) * RATE as f64) as u64).min(frames);
            (end > start).then_some((start, end))
        });
    }
    pub fn status(&self) -> (f64, f64, bool) {
        let state = self.state.lock().unwrap();
        (
            state.position as f64 / RATE as f64,
            state
                .audio
                .as_ref()
                .map_or(0., |a| a.frames as f64 / RATE as f64),
            state.playing,
        )
    }
    pub fn take_error(&self) -> Option<String> {
        self.state.lock().unwrap().error.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn unavailable_backend_returns_an_error_before_device_allocation() {
        let config = DeviceConfig::new(DeviceType::Playback);
        let result = open_device(&config, Some(miniaudio::Backend::CoreAudio));
        assert!(matches!(result,Err(error) if error.starts_with("Audio backend initialization:")));
    }
    #[test]
    fn playback_switch_keeps_time_and_loop_wraps_without_an_audio_device() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio.f32");
        let bytes: Vec<u8> = [0.2f32, 0.4, 0.6, 0.8]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        std::fs::write(&path, bytes).unwrap();
        let audio = Arc::new(Decoded {
            path,
            frames: 2,
            _directory: directory,
        });
        let mut player = Player::default();
        player.load(audio.clone(), false).unwrap();
        player.seek(1.0 / RATE as f64);
        player.load(audio, false).unwrap();
        assert_eq!(player.state.lock().unwrap().position, 1);
        let mut state = player.state.lock().unwrap();
        state.gain = 0.5;
        state.playing = true;
        state.region = Some((0, 2));
        let mut output = [0.0; 6];
        state.render(&mut output);
        assert_eq!(output, [0.3, 0.4, 0.1, 0.2, 0.3, 0.4]);
    }
}
