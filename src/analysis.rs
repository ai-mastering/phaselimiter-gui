use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};

pub type Series = Arc<[(f32, f32)]>;
pub fn empty_series() -> Series {
    static EMPTY: OnceLock<Series> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::from([])).clone()
}
#[derive(Debug, Default)]
struct PlotData {
    waveform: Series,
    spectrum: Series,
    loudness: Series,
    histogram: Series,
}
#[derive(Debug, Clone)]
pub struct Analysis {
    raw: Arc<Value>,
    artifact: Option<PathBuf>,
    plots: Arc<OnceLock<PlotData>>,
}
impl Analysis {
    pub fn new(value: Value) -> Self {
        Self {
            raw: Arc::new(value),
            artifact: None,
            plots: Arc::default(),
        }
    }
    pub fn path(&self) -> Option<&Path> {
        self.artifact.as_deref()
    }

    // Reports live outside audio/job directories so removing an input cannot
    // invalidate a reference retained by another mastering.
    pub fn store(root: &Path, bytes: &[u8]) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if !value.is_object() {
            return Err("Analyzer did not return an object.".into());
        }
        let directory = root.join("analyses");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let path = directory.join(format!("{digest}.json"));
        if path.exists() {
            crate::library::verify(&path, &digest)?;
        } else {
            let mut temporary =
                tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
            temporary.write_all(bytes).map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            temporary
                .persist_noclobber(&path)
                .map_err(|e| e.to_string())?;
        }
        let mut result = Self::new(value);
        result.artifact = Some(path);
        Ok(result)
    }

    pub fn persist(&mut self, root: &Path) -> Result<(), String> {
        if self.artifact.is_none() {
            *self = Self::store(
                root,
                &serde_json::to_vec(&*self.raw).map_err(|e| e.to_string())?,
            )?;
        }
        Ok(())
    }
    fn points(&self, name: &str, x: &str, y: &str) -> Series {
        self.raw
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
    fn plot_data(&self) -> &PlotData {
        self.plots.get_or_init(|| {
            let values = self.raw.get("waveform").and_then(Value::as_array);
            let count = values.map_or(0, Vec::len).max(1) as f32;
            let waveform = values
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(i, v)| {
                    let y = v.as_f64()? as f32;
                    y.is_finite().then_some((i as f32 / count, y))
                })
                .collect();
            PlotData {
                waveform,
                spectrum: self.points("spectrum", "hz", "db"),
                loudness: self.points("loudness_time_series", "sec", "db"),
                histogram: self.points("histogram", "db", "frequency"),
            }
        })
    }
    pub fn number(&self, name: &str) -> Option<f64> {
        self.raw.get(name)?.as_f64().filter(|x| x.is_finite())
    }
    pub fn series(&self, name: &str, x: &str, y: &str) -> Series {
        let data = self.plot_data();
        match (name, x, y) {
            ("spectrum", "hz", "db") => data.spectrum.clone(),
            ("loudness_time_series", "sec", "db") => data.loudness.clone(),
            ("histogram", "db", "frequency") => data.histogram.clone(),
            _ => self.points(name, x, y),
        }
    }
    pub fn waveform(&self) -> Series {
        self.plot_data().waveform.clone()
    }
}

pub fn executable(backend: &Path) -> PathBuf {
    backend.with_file_name(if cfg!(windows) {
        "audio_analyzer.exe"
    } else {
        "audio_analyzer"
    })
}

impl Serialize for Analysis {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(path) = &self.artifact {
            #[derive(Serialize)]
            struct Reference<'a> {
                analysis_file: &'a Path,
            }
            Reference {
                analysis_file: path,
            }
            .serialize(serializer)
        } else {
            self.raw.serialize(serializer)
        }
    }
}
impl<'de> Deserialize<'de> for Analysis {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if let Some(path) = value.get("analysis_file").and_then(Value::as_str) {
            let path = PathBuf::from(path);
            let expected = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| serde::de::Error::custom("Invalid analysis path"))?;
            crate::library::verify(&path, expected).map_err(serde::de::Error::custom)?;
            let bytes = std::fs::read(&path).map_err(serde::de::Error::custom)?;
            let raw = serde_json::from_slice(&bytes).map_err(serde::de::Error::custom)?;
            let mut result = Self::new(raw);
            result.artifact = Some(path);
            Ok(result)
        } else {
            Ok(Self::new(value))
        }
    }
}

pub async fn run(
    root: PathBuf,
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
    let analysis = Analysis::store(&root, &output.stdout)?;
    analysis.plot_data();
    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clones_share_plot_data_and_saved_json_remains_compatible() {
        let raw = serde_json::json!({"waveform":[0.1,0.2],"spectrum":[{"hz":100,"db":-10}]});
        let analysis = Analysis::new(raw.clone());
        let other = analysis.clone();
        assert!(Arc::ptr_eq(&analysis.waveform(), &other.waveform()));
        assert!(Arc::ptr_eq(
            &analysis.series("spectrum", "hz", "db"),
            &other.series("spectrum", "hz", "db")
        ));
        assert_eq!(serde_json::to_value(&analysis).unwrap(), raw);
        let restored: Analysis = serde_json::from_value(raw).unwrap();
        assert_eq!(restored.waveform().len(), 2);
    }
    #[test]
    fn reads_comparison_series_and_ignores_missing_metrics() {
        let analysis = Analysis::new(
            serde_json::json!({"loudness":-9.2,"spectrum":[{"hz":100,"db":-20},{"hz":200}],"waveform":[0.1,-0.2]}),
        );
        assert_eq!(analysis.number("loudness"), Some(-9.2));
        assert_eq!(analysis.number("true_peak"), None);
        assert_eq!(&*analysis.series("spectrum", "hz", "db"), &[(100., -20.)]);
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
            dir.path().to_path_buf(),
            input.clone(),
            crate::library::hash(&input).unwrap(),
            dir.path().join("phase_limiter"),
            ffmpeg.display().to_string(),
            dir.path().join("cache"),
        )
        .await
        .unwrap();
        assert_eq!(result.number("loudness"), Some(-10.));
        assert!(result.path().unwrap().is_file());
    }
}
