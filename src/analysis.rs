use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis(pub Value);
impl Analysis {
    pub fn number(&self, name: &str) -> Option<f64> {
        self.0.get(name)?.as_f64().filter(|x| x.is_finite())
    }
    pub fn series(&self, name: &str, x: &str, y: &str) -> Vec<(f32, f32)> {
        self.0
            .get(name)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let x = p.get(x)?.as_f64()? as f32;
                let y = p.get(y)?.as_f64()? as f32;
                (x.is_finite() && y.is_finite()).then_some((x, y))
            })
            .collect()
    }
    pub fn waveform(&self) -> Vec<(f32, f32)> {
        let values = self.0.get("waveform").and_then(Value::as_array);
        let count = values.map_or(0, Vec::len).max(1) as f32;
        values
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(i, v)| {
                let y = v.as_f64()? as f32;
                y.is_finite().then_some((i as f32 / count, y))
            })
            .collect()
    }
}

pub fn executable(backend: &Path) -> PathBuf {
    backend.with_file_name(if cfg!(windows) {
        "audio_analyzer.exe"
    } else {
        "audio_analyzer"
    })
}

pub async fn run(
    input: PathBuf,
    expected: String,
    backend: PathBuf,
    ffmpeg: String,
    cache: PathBuf,
) -> Result<Analysis, String> {
    let checked = input.clone();
    tokio::task::spawn_blocking(move || crate::library::verify(&checked, &expected))
        .await
        .map_err(|e| e.to_string())??;
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    // The analyzer accepts WAV/WAVEX, not arbitrary FFmpeg-supported inputs.
    let normalized = scratch.path().join("input.wav");
    let mut normalize = tokio::process::Command::new(&ffmpeg);
    normalize
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&input)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "2",
            "-c:a",
            "pcm_f32le",
            "-f",
            "wav",
        ])
        .arg(&normalized)
        .kill_on_drop(true);
    #[cfg(windows)]
    normalize.creation_flags(0x08000000);
    let decoded = tokio::time::timeout(std::time::Duration::from_secs(300), normalize.output())
        .await
        .map_err(|_| "Preparing analysis timed out.".to_string())?
        .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    if !decoded.status.success() {
        return Err(format!(
            "Preparing analysis: {}",
            String::from_utf8_lossy(&decoded.stderr)
        ));
    }
    let mut command = tokio::process::Command::new(executable(&backend));
    command
        .args(["--input"])
        .arg(&normalized)
        .arg("--ffmpeg")
        .arg(ffmpeg)
        .arg("--sound_quality2_cache")
        .arg(cache)
        .arg("--tmp")
        .arg(scratch.path())
        .arg("--drr=true")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(1800), command.output())
        .await
        .map_err(|_| "Analysis timed out.".to_string())?
        .map_err(|e| format!("Could not start audio_analyzer: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Analysis failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .rev()
                .take(1200)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid analyzer JSON: {e}"))?;
    if !value.is_object() {
        return Err("Analyzer did not return an object.".into());
    }
    let report = input.with_extension("analysis.json");
    let mut temporary =
        tempfile::NamedTempFile::new_in(input.parent().ok_or("Missing audio directory")?)
            .map_err(|e| e.to_string())?;
    use std::io::Write;
    temporary
        .write_all(&output.stdout)
        .map_err(|e| e.to_string())?;
    temporary.persist(report).map_err(|e| e.to_string())?;
    Ok(Analysis(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_comparison_series_and_ignores_missing_metrics() {
        let analysis = Analysis(
            serde_json::json!({"loudness":-9.2,"spectrum":[{"hz":100,"db":-20},{"hz":200}],"waveform":[0.1,-0.2]}),
        );
        assert_eq!(analysis.number("loudness"), Some(-9.2));
        assert_eq!(analysis.number("true_peak"), None);
        assert_eq!(analysis.series("spectrum", "hz", "db"), vec![(100., -20.)]);
        assert_eq!(analysis.waveform().len(), 2);
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn normalizes_before_analyzing_and_persists_the_reference_report() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("日本語 input.mp3");
        std::fs::write(&input, b"fake compressed audio").unwrap();
        let ffmpeg = dir.path().join("fake ffmpeg");
        std::fs::write(
            &ffmpeg,
            "#!/bin/sh\nfor arg do dest=\"$arg\"; done\nprintf normalized > \"$dest\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755)).unwrap();
        let analyzer = dir.path().join("audio_analyzer");
        std::fs::write(&analyzer,"#!/bin/sh\n[ \"$1\" = '--input' ] || exit 4\n[ \"$(cat \"$2\")\" = normalized ] || exit 5\nprintf '{\"loudness\":-10,\"bands\":[],\"covariance\":[]}'\n").unwrap();
        std::fs::set_permissions(&analyzer, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = run(
            input.clone(),
            crate::library::hash(&input).unwrap(),
            dir.path().join("phase_limiter"),
            ffmpeg.display().to_string(),
            dir.path().join("cache"),
        )
        .await
        .unwrap();
        assert_eq!(result.number("loudness"), Some(-10.));
        assert!(input.with_extension("analysis.json").is_file());
    }
}
