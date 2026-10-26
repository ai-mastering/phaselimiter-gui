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

    pub fn read_report(path: PathBuf) -> Result<Self, String> {
        let mut analysis = Self::new(Value::Null);
        analysis.artifact = Some(path);
        analysis.load_report()?;
        Ok(analysis)
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
        let exists = path.exists();
        if !exists || crate::library::verify(&path, &digest).is_err() {
            let mut temporary =
                tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
            temporary.write_all(bytes).map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            if exists {
                // Repair only with bytes matching the original content ID.
                // Valid immutable reports are never rewritten.
                temporary.persist(&path).map_err(|e| e.to_string())?;
            } else {
                temporary
                    .persist_noclobber(&path)
                    .map_err(|e| e.to_string())?;
            }
        }
        let mut result = Self::new(value);
        result.artifact = Some(path);
        Ok(result)
    }

    #[cfg(test)]
    pub fn available(&self) -> bool {
        self.raw.is_object()
    }

    // Resolve file contents explicitly at the library boundary, not in serde.
    // A failed report keeps its reference so saving cannot erase recovery data.
    pub fn load_report(&mut self) -> Result<(), String> {
        let Some(path) = &self.artifact else {
            return Ok(());
        };
        let expected = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid analysis path")?;
        crate::library::verify(path, expected)?;
        let raw: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if !raw.is_object() {
            return Err("Analysis report is not an object.".into());
        }
        self.raw = Arc::new(raw);
        self.plots = Arc::default();
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
    pub fn sound_quality_error(&self) -> Option<&str> {
        self.raw.get("sound_quality_error")?.as_str()
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

// The classic detect2 score uses short-window analysis and the prepared
// detector population; audio_analyzer's optional v1 score is a different model.
async fn detect_sound_quality(
    backend: &Path,
    analyzer_json: &[u8],
    scratch: &Path,
) -> Result<Value, String> {
    let source = scratch.join("detector-input.json");
    std::fs::write(&source, analyzer_json).map_err(|e| format!("Saving detector input: {e}"))?;
    let preparation = backend
        .parent()
        .and_then(Path::parent)
        .ok_or("Cannot locate resource/detection_preparation.json beside the engine's bin folder.")?
        .join("resource/detection_preparation.json");
    let executable = backend.with_file_name(if cfg!(windows) {
        "audio_detector.exe"
    } else {
        "audio_detector"
    });
    let mut command = tokio::process::Command::new(&executable);
    command
        .arg("detect2")
        .arg("--source")
        .arg(source)
        .arg("--detection_preparation")
        .arg(preparation)
        .arg("--neighbor_ratio=0.01")
        .arg("--outlier_ratio=0.1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(1800), command.output())
        .await
        .map_err(|_| "audio_detector timed out.".to_string())?
        .map_err(|e| format!("Could not start {}: {e}", executable.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "audio_detector failed ({}): {}",
            output.status,
            stderr.chars().take(1200).collect::<String>()
        ));
    }
    let result: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid audio_detector JSON: {e}"))?;
    result
        .get("sound_quality")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or("audio_detector did not return a finite sound_quality value.")?;
    Ok(result)
}

impl Serialize for Analysis {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Reference<'a> {
            analysis_file: &'a Path,
        }
        let path = self.artifact.as_deref().ok_or_else(|| {
            serde::ser::Error::custom("Analysis must be stored before saving the library")
        })?;
        Reference {
            analysis_file: path,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Analysis {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Reference {
            analysis_file: PathBuf,
        }
        let reference = Reference::deserialize(deserializer)?;
        let mut result = Self::new(Value::Null);
        result.artifact = Some(reference.analysis_file);
        Ok(result)
    }
}

#[derive(Clone)]
pub struct Options {
    pub sound_quality2: bool,
    pub detect2: bool,
    pub force: bool,
    pub cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl Options {
    pub const FULL: Self = Self {
        sound_quality2: true,
        detect2: true,
        force: false,
        cancel: None,
    };
    pub const REFERENCE: Self = Self {
        sound_quality2: false,
        detect2: false,
        force: false,
        cancel: None,
    };
}

#[cfg(test)]
pub async fn run(
    root: PathBuf,
    input: PathBuf,
    expected: String,
    backend: PathBuf,
    ffmpeg: String,
    cache: PathBuf,
) -> Result<Analysis, String> {
    run_with_options(root, input, expected, backend, ffmpeg, cache, Options::FULL).await
}

pub async fn run_with_options(
    root: PathBuf,
    input: PathBuf,
    expected: String,
    backend: PathBuf,
    ffmpeg: String,
    cache: PathBuf,
    options: Options,
) -> Result<Analysis, String> {
    let checked = input.clone();
    let checked_hash = expected.clone();
    tokio::task::spawn_blocking(move || crate::library::verify(&checked, &checked_hash))
        .await
        .map_err(|e| e.to_string())??;
    // Unavailable dependencies disable cache lookup, never the partial analysis itself.
    let key_backend = backend.clone();
    let key_ffmpeg = ffmpeg.clone();
    let key_cache = cache.clone();
    let key_expected = expected.clone();
    let key_options = options.clone();
    let cache_entry = tokio::task::spawn_blocking(move || {
        crate::analysis_cache::entry(
            &key_expected,
            &key_backend,
            &key_ffmpeg,
            &key_cache,
            &key_options,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .ok();
    if options
        .cancel
        .as_ref()
        .is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed))
    {
        return Err("Reference analysis cancelled.".into());
    }
    if !options.force
        && let Some(entry) = &cache_entry
        && let Ok(bytes) = std::fs::read(entry)
        && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
        && cacheable(&value, &options)
        && let Ok(report) = Analysis::store(&root, &bytes)
    {
        let image = entry.with_extension("frequency.png");
        if image.is_file()
            && let Some(path) = report.path()
        {
            let _ = std::fs::copy(image, path.with_extension("frequency.png"));
        }
        return Ok(report);
    }
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
    let decoded = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        process_output(&mut normalize, options.cancel.as_deref()),
    )
    .await
    .map_err(|_| "Preparing analysis timed out.".to_string())?
    .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    if !decoded.status.success() {
        return Err(format!(
            "Preparing analysis: {}",
            String::from_utf8_lossy(&decoded.stderr)
        ));
    }
    let ffmpeg_for_key = ffmpeg.clone();
    let cache_for_key = cache.clone();
    let mut command = tokio::process::Command::new(executable(&backend));
    command
        .args(["--input"])
        .arg(&normalized)
        .arg("--ffmpeg")
        .arg(ffmpeg)
        .arg(format!("--sound_quality2={}", options.sound_quality2))
        .arg("--sound_quality2_cache_archiver=binary")
        .arg("--sound_quality2_cache")
        .arg(cache)
        .arg("--tmp")
        .arg(scratch.path())
        .arg("--spectrum_distribution_output")
        .arg(scratch.path().join("frequency.png"))
        .arg("--analysis_for_visualization=true")
        .arg("--drr=true")
        .arg("--sound_quality=false")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(1800),
        process_output(&mut command, options.cancel.as_deref()),
    )
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
    let mut value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid analyzer JSON: {e}"))?;
    let report = value
        .as_object_mut()
        .ok_or("Analyzer did not return an object.")?;
    // Never fall back to an analyzer score, even if an external analyzer emits it.
    for key in [
        "sound_quality",
        "sound_quality_error",
        "sound_quality_detection",
    ] {
        report.remove(key);
    }
    if !options.sound_quality2 {
        report.remove("sound_quality2");
    }
    if options.detect2 {
        report.insert("sound_quality_source".into(), Value::from("audio_detector"));
        report.insert("sound_quality_neighbor_ratio".into(), Value::from(0.01));
        report.insert("sound_quality_outlier_ratio".into(), Value::from(0.1));
        match detect_sound_quality(&backend, &output.stdout, scratch.path()).await {
            Ok(detection) => {
                report.insert("sound_quality".into(), detection["sound_quality"].clone());
                report.insert("sound_quality_detection".into(), detection);
            }
            Err(error) => {
                report.insert("sound_quality_error".into(), Value::from(error));
            }
        }
    }
    let checked = input.clone();
    let checked_hash = expected.clone();
    tokio::task::spawn_blocking(move || crate::library::verify(&checked, &checked_hash))
        .await
        .map_err(|e| e.to_string())??;
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    let analysis = Analysis::store(&root, &bytes)?;
    if let Some(report) = analysis.path() {
        let source = scratch.path().join("frequency.png");
        if source.is_file() {
            std::fs::copy(&source, report.with_extension("frequency.png"))
                .map_err(|e| format!("Saving analysis image: {e}"))?;
        }
    }
    if cacheable(&value, &options)
        && let Some(entry) = cache_entry
    {
        // Check dependencies again: a tool or model may have changed during analysis.
        let current = tokio::task::spawn_blocking(move || {
            crate::analysis_cache::entry(
                &expected,
                &backend,
                &ffmpeg_for_key,
                &cache_for_key,
                &options,
            )
        })
        .await
        .map_err(|e| e.to_string())?
        .ok();
        if current.as_ref() != Some(&entry) {
            return Err("Analysis dependencies changed during analysis. Please retry.".into());
        }
        if current.as_ref() == Some(&entry) {
            let _ =
                crate::analysis_cache::store(&entry, &bytes, &scratch.path().join("frequency.png"));
        }
    }
    analysis.plot_data();
    Ok(analysis)
}

fn cacheable(value: &Value, options: &Options) -> bool {
    value.is_object()
        && value.get("sound_quality_error").is_none()
        && (!options.sound_quality2
            || value
                .get("sound_quality2")
                .and_then(Value::as_f64)
                .is_some_and(f64::is_finite))
        && (!options.detect2
            || value
                .get("sound_quality")
                .and_then(Value::as_f64)
                .is_some_and(f64::is_finite))
}

// Cancellation kills and reaps the child before a replacement reference starts.
async fn process_output(
    command: &mut tokio::process::Command,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::process::Output> {
    use std::sync::atomic::Ordering;
    use tokio::io::AsyncReadExt;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("Missing stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("Missing stderr"))?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let (status, out_result, err_result) = tokio::join!(
        async {
            loop {
                if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                    child.kill().await?;
                    return Err(std::io::Error::other("Reference analysis cancelled."));
                }
                if let Some(status) = child.try_wait()? {
                    return Ok(status);
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        },
        stdout.read_to_end(&mut out),
        stderr.read_to_end(&mut err)
    );
    out_result?;
    err_result?;
    Ok(std::process::Output {
        status: status?,
        stdout: out,
        stderr: err,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clones_share_plot_data_and_saved_references_restore_reports() {
        let raw = serde_json::json!({"waveform":[0.1,0.2],"spectrum":[{"hz":100,"db":-10}]});
        let dir = tempfile::tempdir().unwrap();
        let analysis = Analysis::store(dir.path(), &serde_json::to_vec(&raw).unwrap()).unwrap();
        let other = analysis.clone();
        assert!(Arc::ptr_eq(&analysis.waveform(), &other.waveform()));
        assert!(Arc::ptr_eq(
            &analysis.series("spectrum", "hz", "db"),
            &other.series("spectrum", "hz", "db")
        ));
        let saved = serde_json::to_value(&analysis).unwrap();
        assert_eq!(
            saved,
            serde_json::json!({"analysis_file": analysis.path().unwrap()})
        );
        let mut restored: Analysis = serde_json::from_value(saved).unwrap();
        restored.load_report().unwrap();
        assert_eq!(restored.waveform().len(), 2);
    }
    #[test]
    fn inline_reports_are_not_a_saved_library_format() {
        let raw = serde_json::json!({"loudness": -10});
        assert!(serde_json::from_value::<Analysis>(raw.clone()).is_err());
        assert!(serde_json::to_value(Analysis::new(raw)).is_err());
    }

    #[test]
    fn regeneration_repairs_only_the_exact_content_addressed_report() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = br#"{"loudness":-10}"#;
        let original = Analysis::store(dir.path(), bytes).unwrap();
        let path = original.path().unwrap();
        std::fs::write(path, b"damaged").unwrap();
        let replacement = Analysis::store(dir.path(), br#"{"loudness":-11}"#).unwrap();
        assert_ne!(replacement.path(), Some(path));
        assert_eq!(std::fs::read(path).unwrap(), b"damaged");
        let repaired = Analysis::store(dir.path(), bytes).unwrap();
        assert_eq!(repaired.path(), Some(path));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
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
    fn write_executable(path: &Path, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    fn analysis_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("日本語 input.mp3"),
            b"fake compressed audio",
        )
        .unwrap();
        write_executable(
            &dir.path().join("fake ffmpeg"),
            "#!/bin/sh\nfor arg do dest=\"$arg\"; done\nprintf normalized > \"$dest\"\n",
        );
        let engine = dir.path().join("engine 日本語");
        std::fs::create_dir_all(engine.join("bin")).unwrap();
        std::fs::create_dir_all(engine.join("resource")).unwrap();
        std::fs::write(engine.join("resource/detection_preparation.json"), b"{}").unwrap();
        write_executable(
            &engine.join("bin/audio_analyzer"),
            r#"#!/bin/sh
[ "$1" = '--input' ] || exit 4
[ "$(cat "$2")" = normalized ] || exit 5
printf x >> "$0.calls"
quality=0
while [ "$#" -gt 0 ]; do
 case "$1" in
  --sound_quality=false) quality=1;;
  --sound_quality=true|--analysis_data_dir) exit 6;;
  --spectrum_distribution_output) printf image > "$2";;
 esac
 shift
done
[ "$quality" = 1 ] || exit 7
printf '{"loudness":-10,"sound_quality":0.11,"sound_quality2":0.88,"bands_short":[],"covariance_short":[]}'
"#,
        );
        let analyzer = engine.join("bin/audio_analyzer");
        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new()
                .append(true)
                .open(&analyzer)
                .unwrap(),
            "\n# {}",
            dir.path().display()
        )
        .unwrap();
        std::fs::write(dir.path().join("cache"), b"fake quality2 model").unwrap();
        std::fs::write(
            engine.join("resource/detection_preparation.json"),
            br#"{"paths":[]}"#,
        )
        .unwrap();
        dir
    }

    #[cfg(unix)]
    async fn run_fixture(dir: &Path) -> Analysis {
        let input = dir.join("日本語 input.mp3");
        run(
            dir.to_path_buf(),
            input.clone(),
            crate::library::hash(&input).unwrap(),
            dir.join("engine 日本語/bin/phase_limiter"),
            dir.join("fake ffmpeg").display().to_string(),
            dir.join("cache"),
        )
        .await
        .unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn normalizes_then_detects_classic_quality_and_persists_both_scores() {
        let dir = analysis_fixture();
        write_executable(
            &dir.path().join("engine 日本語/bin/audio_detector"),
            r#"#!/bin/sh
[ "$1" = detect2 ] || exit 4
source_file=; preparation=; neighbor=; outlier=
while [ "$#" -gt 0 ]; do
 case "$1" in
  --source) source_file="$2"; shift;;
  --detection_preparation) preparation="$2"; shift;;
  --neighbor_ratio=0.01) neighbor=yes;;
  --outlier_ratio=0.1) outlier=yes;;
 esac
 shift
done
[ -f "$preparation" ] && [ "$neighbor$outlier" = yesyes ] || exit 5
case "$(cat "$source_file")" in *'"bands_short"'*'"covariance_short"'*) ;; *) exit 6;; esac
printf '{"sound_quality":0.72,"lof":1.2,"border_lof":1.5,"ranking":[]}'
"#,
        );
        let result = run_fixture(dir.path()).await;
        assert_eq!(result.number("loudness"), Some(-10.));
        assert_eq!(result.number("sound_quality"), Some(0.72));
        assert_eq!(result.number("sound_quality2"), Some(0.88));
        assert_eq!(result.raw["sound_quality_source"], "audio_detector");
        assert_eq!(result.number("sound_quality_neighbor_ratio"), Some(0.01));
        assert_eq!(result.number("sound_quality_outlier_ratio"), Some(0.1));
        assert_eq!(result.raw["sound_quality_detection"]["lof"], 1.2);
        let restored = Analysis::read_report(result.path().unwrap().to_path_buf()).unwrap();
        assert_eq!(restored.number("sound_quality"), Some(0.72));
        assert_eq!(
            std::fs::read(result.path().unwrap().with_extension("frequency.png")).unwrap(),
            b"image"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn detector_errors_keep_analysis_without_falling_back_to_the_analyzer_score() {
        let dir = analysis_fixture();
        let detector = dir.path().join("engine 日本語/bin/audio_detector");
        // Missing executable, failed process, malformed output and missing/invalid score.
        for script in [
            None,
            Some("echo broken-preparation >&2; exit 7"),
            Some("printf not-json"),
            Some("printf '{}'"),
            Some("printf '{\"sound_quality\":null}'"),
            Some("printf '{\"sound_quality\":1e400}'"),
        ] {
            if let Some(script) = script {
                write_executable(&detector, &format!("#!/bin/sh\n{script}\n"));
            }
            let result = run_fixture(dir.path()).await;
            assert_eq!(result.number("loudness"), Some(-10.));
            assert_eq!(result.number("sound_quality2"), Some(0.88));
            assert_eq!(result.number("sound_quality"), None);
            assert!(
                result
                    .sound_quality_error()
                    .unwrap()
                    .contains("audio_detector")
            );
            let restored = Analysis::read_report(result.path().unwrap().to_path_buf()).unwrap();
            assert_eq!(restored.sound_quality_error(), result.sound_quality_error());
        }
        write_executable(&detector, "#!/bin/sh\nprintf '{\"sound_quality\":0}'\n");
        let result = run_fixture(dir.path()).await;
        assert_eq!(result.number("sound_quality"), Some(0.));
        assert!(result.sound_quality_error().is_none());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn cache_reuses_merged_scores_and_force_bypasses_it() {
        let dir = analysis_fixture();
        let detector = dir.path().join("engine 日本語/bin/audio_detector");
        write_executable(
            &detector,
            "#!/bin/sh\nprintf x >> \"$0.calls\"\nprintf '{\"sound_quality\":0.72}'\n",
        );
        let first = run_fixture(dir.path()).await;
        let report = first.path().unwrap();
        std::fs::remove_file(report).unwrap();
        std::fs::remove_file(report.with_extension("frequency.png")).unwrap();
        let second = run_fixture(dir.path()).await;
        assert_eq!(first.path(), second.path());
        assert_eq!(second.number("sound_quality"), Some(0.72));
        assert_eq!(second.number("sound_quality2"), Some(0.88));
        assert!(report.with_extension("frequency.png").is_file());
        let calls = dir.path().join("engine 日本語/bin/audio_analyzer.calls");
        assert_eq!(std::fs::read(&calls).unwrap(), b"x");
        assert_eq!(
            std::fs::read(detector.with_extension("calls")).unwrap(),
            b"x"
        );
        let input = dir.path().join("日本語 input.mp3");
        run_with_options(
            dir.path().to_path_buf(),
            input.clone(),
            library_hash(&input),
            dir.path().join("engine 日本語/bin/phase_limiter"),
            dir.path().join("fake ffmpeg").display().to_string(),
            dir.path().join("cache"),
            Options {
                force: true,
                ..Options::FULL
            },
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(calls).unwrap(), b"xx");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reference_profile_skips_scores_and_failed_full_results_are_retried() {
        let dir = analysis_fixture();
        let detector = dir.path().join("engine 日本語/bin/audio_detector");
        write_executable(&detector, "#!/bin/sh\nprintf x >> \"$0.calls\"\nexit 7\n");
        let input = dir.path().join("日本語 input.mp3");
        let reference = run_with_options(
            dir.path().to_path_buf(),
            input.clone(),
            library_hash(&input),
            dir.path().join("engine 日本語/bin/phase_limiter"),
            dir.path().join("fake ffmpeg").display().to_string(),
            dir.path().join("cache"),
            Options::REFERENCE,
        )
        .await
        .unwrap();
        assert_eq!(reference.number("sound_quality"), None);
        assert_eq!(reference.number("sound_quality2"), None);
        assert!(!detector.with_extension("calls").exists());
        assert!(
            run_fixture(dir.path())
                .await
                .sound_quality_error()
                .is_some()
        );
        assert!(
            run_fixture(dir.path())
                .await
                .sound_quality_error()
                .is_some()
        );
        assert_eq!(
            std::fs::read(detector.with_extension("calls")).unwrap(),
            b"xx"
        );
    }

    #[cfg(unix)]
    fn library_hash(path: &Path) -> String {
        crate::library::hash(path).unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_reaps_the_analysis_process_promptly() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancel = AtomicBool::new(false);
        let mut command = tokio::process::Command::new("sleep");
        command.arg("30").kill_on_drop(true);
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            tokio::join!(process_output(&mut command, Some(&cancel)), async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                cancel.store(true, Ordering::Relaxed);
            })
            .0
        })
        .await
        .unwrap();
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}
