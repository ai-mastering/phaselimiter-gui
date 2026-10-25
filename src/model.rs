mod storage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Wav,
    Mp3,
    Aac,
}
impl Format {
    pub const ALL: [Self; 3] = [Self::Wav, Self::Mp3, Self::Aac];
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Aac => "aac",
        }
    }
}
impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.extension())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Solver {
    DePrmm,
    Cmaes,
}
impl Solver {
    pub const ALL: [Self; 2] = [Self::DePrmm, Self::Cmaes];
    pub fn argument(self) -> &'static str {
        match self {
            Self::DePrmm => "de_prmm",
            Self::Cmaes => "cmaes",
        }
    }
}
impl std::fmt::Display for Solver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DePrmm => "DE-PRMM",
            Self::Cmaes => "CMA-ES",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReferenceMode {
    #[default]
    Loudness,
    YoutubeLoudness,
    Rms,
    Peak,
    Zero,
}
impl ReferenceMode {
    pub const ALL: [Self; 5] = [
        Self::Loudness,
        Self::YoutubeLoudness,
        Self::Rms,
        Self::Peak,
        Self::Zero,
    ];
    pub fn argument(self) -> &'static str {
        match self {
            Self::Loudness => "loudness",
            Self::YoutubeLoudness => "youtube_loudness",
            Self::Rms => "rms",
            Self::Peak => "peak",
            Self::Zero => "zero",
        }
    }
}
impl std::fmt::Display for ReferenceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.argument())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CeilingMode {
    Peak,
    #[default]
    TruePeak,
    LowpassTruePeak,
}
impl CeilingMode {
    pub const ALL: [Self; 3] = [Self::Peak, Self::TruePeak, Self::LowpassTruePeak];
    pub fn argument(self) -> &'static str {
        match self {
            Self::Peak => "peak",
            Self::TruePeak => "true_peak",
            Self::LowpassTruePeak => "lowpass_true_peak",
        }
    }
}
impl std::fmt::Display for CeilingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.argument())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NoiseMode {
    #[default]
    Linear,
    Adaptive,
}
impl NoiseMode {
    pub const ALL: [Self; 2] = [Self::Linear, Self::Adaptive];
    pub fn argument(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Adaptive => "adaptive",
        }
    }
}
impl std::fmt::Display for NoiseMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.argument())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub noise_mode: NoiseMode,
    pub max_iter2: String,
    pub max_iter1: String,
    pub oversample: String,
    pub lowpass_cut: String,
    pub ceiling_mode: CeilingMode,
    pub reference_mode: ReferenceMode,
    pub high_cut: String,
    pub low_cut: String,
    pub mastering: bool,
    pub reference: String,
    pub loudness: String,
    pub intensity: String,
    pub ceiling: String,
    pub sample_rate: String,
    pub bit_depth: String,
    pub workers: String,
    pub evaluations: String,
    pub seed: String,
    pub preserve_bass: bool,
    pub pre_compression: bool,
    pub format: Format,
    pub solver: Solver,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub backend: String,
    pub ffmpeg: String,
    pub cache: String,
    #[serde(alias = "output_dir")]
    pub export_dir: String,
}

fn default_export_directory() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    [home.join("Downloads"), home.join("Desktop"), home]
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            backend: String::new(),
            ffmpeg: String::new(),
            cache: String::new(),
            export_dir: default_export_directory().to_string_lossy().into_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTools {
    pub backend: PathBuf,
    pub ffmpeg: String,
    pub cache: PathBuf,
}

impl AppSettings {
    pub fn backend_path(&self) -> PathBuf {
        if self.backend.trim().is_empty() {
            bundled_root()
                .join("phaselimiter/bin")
                .join(if cfg!(windows) {
                    "phase_limiter.exe"
                } else {
                    "phase_limiter"
                })
        } else {
            PathBuf::from(&self.backend)
        }
    }

    pub fn ffmpeg_command(&self) -> String {
        if self.ffmpeg.trim().is_empty() {
            bundled_root()
                .join(if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                })
                .to_string_lossy()
                .into_owned()
        } else {
            self.ffmpeg.clone()
        }
    }

    pub fn cache_path(&self) -> PathBuf {
        if self.cache.trim().is_empty() {
            bundled_root().join("phaselimiter/resource/sound_quality2_cache")
        } else {
            PathBuf::from(&self.cache)
        }
    }

    pub fn export_directory(&self) -> PathBuf {
        if self.export_dir.trim().is_empty() {
            default_export_directory()
        } else {
            PathBuf::from(&self.export_dir)
        }
    }

    pub fn resolved_tools(&self) -> ExecutionTools {
        ExecutionTools {
            backend: self.backend_path(),
            ffmpeg: self.ffmpeg_command(),
            cache: self.cache_path(),
        }
    }
}

fn bundled_root() -> PathBuf {
    let executable = std::env::current_exe().unwrap_or_default();
    bundled_root_for(&executable)
}

fn bundled_root_for(executable: &Path) -> PathBuf {
    let dir = executable.parent().unwrap_or(Path::new("."));
    if dir.file_name().is_some_and(|n| n == "MacOS") {
        dir.parent().unwrap_or(dir).join("Resources")
    } else {
        dir.to_path_buf()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mastering: true,
            low_cut: "20".into(),
            high_cut: "20000".into(),
            reference_mode: ReferenceMode::Loudness,
            ceiling_mode: CeilingMode::TruePeak,
            lowpass_cut: "15000".into(),
            oversample: "1".into(),
            max_iter1: "100".into(),
            max_iter2: "400".into(),
            noise_mode: NoiseMode::Linear,
            reference: String::new(),
            loudness: "-9".into(),
            intensity: "1".into(),
            ceiling: "0".into(),
            sample_rate: "44100".into(),
            bit_depth: "16".into(),
            workers: "0".into(),
            evaluations: "40000".into(),
            seed: "1".into(),
            preserve_bass: true,
            pre_compression: true,
            format: Format::Wav,
            solver: Solver::DePrmm,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_noise_mode")]
    pub noise_mode: NoiseMode,
    #[serde(default = "default_max_iter2")]
    pub max_iter2: u32,
    #[serde(default = "default_max_iter1")]
    pub max_iter1: u32,
    #[serde(default = "default_oversample")]
    pub oversample: u32,
    #[serde(default = "default_lowpass_cut")]
    pub lowpass_cut: f64,
    #[serde(default = "default_ceiling_mode")]
    pub ceiling_mode: CeilingMode,
    #[serde(default = "default_reference_mode")]
    pub reference_mode: ReferenceMode,
    #[serde(default = "default_high_cut")]
    pub high_cut: f64,
    #[serde(default = "default_low_cut")]
    pub low_cut: f64,
    #[serde(default = "default_mastering")]
    pub mastering: bool,
    #[serde(default)]
    pub reference: Option<PathBuf>,
    pub loudness: f64,
    pub intensity: f64,
    pub ceiling: f64,
    pub sample_rate: u32,
    pub bit_depth: u32,
    pub workers: u32,
    pub evaluations: u32,
    pub seed: u64,
    pub preserve_bass: bool,
    pub pre_compression: bool,
    pub format: Format,
    pub solver: Solver,
}

impl Settings {
    pub fn parse(&self) -> Result<Config, String> {
        fn number(s: &str, label: &str, min: f64, max: f64) -> Result<f64, String> {
            let value: f64 = s
                .trim()
                .parse()
                .map_err(|_| format!("{label}: enter a number."))?;
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("{label}: use {min} to {max}."));
            }
            Ok(value)
        }
        fn integer(s: &str, label: &str) -> Result<u32, String> {
            s.trim()
                .parse()
                .map_err(|_| format!("{label}: enter a non-negative integer."))
        }
        let sample_rate = integer(&self.sample_rate, "Sample rate")?;
        if !(8000..=192000).contains(&sample_rate) {
            return Err("Sample rate: use 8000 to 192000 Hz.".into());
        }
        let bit_depth = integer(&self.bit_depth, "Bit depth")?;
        if ![16, 24, 32].contains(&bit_depth) {
            return Err("WAV bit depth: use 16, 24 or 32 (float).".into());
        }
        let evaluations = integer(&self.evaluations, "Evaluation budget")?;
        if evaluations == 0 {
            return Err("Evaluation budget must be positive.".into());
        }
        let config = Config {
            noise_mode: self.noise_mode,
            max_iter2: integer(&self.max_iter2, "max_iter2")?,
            max_iter1: integer(&self.max_iter1, "max_iter1")?,
            oversample: integer(&self.oversample, "oversample")?,
            lowpass_cut: number(&self.lowpass_cut, "lowpass_cut", 0.0, 22050.0)?,
            ceiling_mode: self.ceiling_mode,
            reference_mode: self.reference_mode,
            high_cut: number(&self.high_cut, "high_cut", 0.0, 22050.0)?,
            low_cut: number(&self.low_cut, "low_cut", 0.0, 22050.0)?,
            mastering: self.mastering,
            reference: (!self.reference.is_empty()).then(|| PathBuf::from(&self.reference)),
            loudness: number(&self.loudness, "Target loudness", -60.0, 0.0)?,
            intensity: number(&self.intensity, "Mastering intensity", 0.0, 1.0)?,
            ceiling: number(&self.ceiling, "True-peak ceiling", -20.0, 0.0)?,
            sample_rate,
            bit_depth,
            workers: integer(&self.workers, "Workers")?,
            evaluations,
            seed: self
                .seed
                .trim()
                .parse()
                .map_err(|_| "Seed: enter a non-negative 64-bit integer.".to_owned())?,
            preserve_bass: self.preserve_bass,
            pre_compression: self.pre_compression,
            format: self.format,
            solver: self.solver,
        };
        if config.low_cut >= 22050.0
            || config.high_cut >= 22050.0
            || config.high_cut == 0.0 && config.low_cut != 0.0
            || config.high_cut != 0.0 && config.low_cut >= config.high_cut
        {
            return Err("Cut frequencies: use low < high, or set both to 0 to disable.".into());
        }
        if config.lowpass_cut <= 0.0
            || config.oversample == 0
            || config.max_iter1 == 0
            || config.max_iter2 == 0
        {
            return Err(
                "Lowpass peak frequency, oversampling and iteration limits must be positive."
                    .into(),
            );
        }
        if !config.oversample.is_power_of_two() || config.oversample > (i32::MAX as u32 / 44100) {
            return Err(
                "Oversampling: use a power of 2 (1, 2, 4, 8, …) within the DSP sample-rate limit."
                    .into(),
            );
        }
        if config.max_iter1 > i32::MAX as u32 || config.max_iter2 > i32::MAX as u32 {
            return Err("Iteration limits exceed the backend integer range.".into());
        }
        if config.ceiling_mode == CeilingMode::LowpassTruePeak
            && config.lowpass_cut > config.sample_rate.min(44100) as f64 / 2.0
        {
            return Err("Lowpass peak frequency must not exceed half the output sample rate (up to 22050 Hz).".into());
        }
        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Waiting,
    Processing,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}
impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Waiting => "Queued",
            Self::Processing => "Processing",
            Self::Succeeded => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::Interrupted => "Interrupted",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "storage::SavedJob")]
pub struct Job {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub track_id: usize,
    #[serde(default)]
    pub output_hash: Option<String>,
    #[serde(default)]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub reference_hash: Option<String>,
    #[serde(default)]
    pub analysis: Option<crate::analysis::Analysis>,
    #[serde(default)]
    pub analysis_error: Option<String>,
    #[serde(default)]
    pub limiting_error: Option<f64>,
    pub id: usize,
    pub input: PathBuf,
    pub output: PathBuf,
    pub config: Config,
    pub tools: ExecutionTools,
    pub status: Status,
    pub progress: f32,
    pub log: Vec<String>,
}

#[cfg(test)]
pub fn test_job(input: PathBuf, output: PathBuf, config: Config, id: usize) -> Job {
    Job {
        id,
        track_id: 0,
        input,
        output,
        config,
        tools: AppSettings::default().resolved_tools(),
        status: Status::Waiting,
        progress: 0.,
        log: Vec::new(),
        output_hash: None,
        input_hash: None,
        reference_hash: None,
        analysis: None,
        analysis_error: None,
        limiting_error: None,
        command: None,
    }
}

fn default_mastering() -> bool {
    true
}

fn default_low_cut() -> f64 {
    20.0
}

fn default_high_cut() -> f64 {
    20000.0
}

fn default_reference_mode() -> ReferenceMode {
    ReferenceMode::Loudness
}

fn default_ceiling_mode() -> CeilingMode {
    CeilingMode::TruePeak
}

fn default_lowpass_cut() -> f64 {
    15000.0
}

fn default_oversample() -> u32 {
    1
}

fn default_max_iter1() -> u32 {
    100
}

fn default_max_iter2() -> u32 {
    400
}

fn default_noise_mode() -> NoiseMode {
    NoiseMode::Linear
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_filter_and_limiter_contracts() {
        let mut settings = Settings {
            ..Settings::default()
        };
        assert!(settings.parse().unwrap().preserve_bass);
        settings.high_cut = "0".into();
        assert!(settings.parse().is_err());
        settings.low_cut = "0".into();
        assert!(settings.parse().is_ok());
        settings.high_cut = "22050".into();
        assert!(settings.parse().is_err());
        settings.high_cut = "20000".into();
        settings.oversample = "3".into();
        assert!(settings.parse().is_err());
        settings.oversample = "4".into();
        assert!(settings.parse().is_ok());
        settings.max_iter1 = "0".into();
        assert!(settings.parse().is_err());
    }

    #[test]
    fn blank_tool_overrides_resolve_at_use_time_without_becoming_saved_paths() {
        let settings = AppSettings {
            ..AppSettings::default()
        };
        let config = settings.resolved_tools();
        assert_eq!(config.backend, settings.backend_path());
        assert_eq!(config.ffmpeg, settings.ffmpeg_command());
        assert_eq!(config.cache, settings.cache_path());
        assert!(!config.backend.as_os_str().is_empty());
        assert!(config.backend.starts_with(bundled_root()));
        assert!(Path::new(&config.ffmpeg).starts_with(bundled_root()));
        assert!(config.cache.starts_with(bundled_root()));
        let saved = serde_json::to_value(&settings).unwrap();
        for field in ["backend", "ffmpeg", "cache"] {
            assert_eq!(saved[field], "");
        }
        let restored: AppSettings = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.backend_path(), config.backend);
    }

    #[test]
    fn explicit_tool_overrides_are_used_even_when_the_files_do_not_exist() {
        let mut settings = AppSettings {
            backend: "missing custom engine".into(),
            ffmpeg: "ffmpeg".into(),
            cache: "custom cache".into(),
            ..AppSettings::default()
        };
        assert_eq!(
            settings.backend_path(),
            PathBuf::from("missing custom engine")
        );
        assert_eq!(settings.ffmpeg_command(), "ffmpeg");
        assert_eq!(settings.cache_path(), PathBuf::from("custom cache"));
        settings.ffmpeg.clear();
        assert_eq!(
            settings.backend_path(),
            PathBuf::from("missing custom engine")
        );
        assert!(Path::new(&settings.ffmpeg_command()).starts_with(bundled_root()));
    }

    #[test]
    fn bundled_resources_follow_the_current_executable_directory() {
        assert_eq!(
            bundled_root_for(Path::new("version-a/phaselimiter-gui")),
            PathBuf::from("version-a")
        );
        assert_eq!(
            bundled_root_for(Path::new("version-b/phaselimiter-gui.exe")),
            PathBuf::from("version-b")
        );
        assert_eq!(
            bundled_root_for(Path::new(
                "version-c/Phaselimiter.app/Contents/MacOS/phaselimiter-gui"
            )),
            PathBuf::from("version-c/Phaselimiter.app/Contents/Resources")
        );
    }

    #[test]
    fn old_saved_configs_receive_backend_defaults() {
        let settings = Settings {
            ..Settings::default()
        };
        let mut value = serde_json::to_value(settings.parse().unwrap()).unwrap();
        for name in [
            "mastering",
            "low_cut",
            "high_cut",
            "reference_mode",
            "ceiling_mode",
            "lowpass_cut",
            "oversample",
            "max_iter1",
            "max_iter2",
            "noise_mode",
        ] {
            value.as_object_mut().unwrap().remove(name);
        }
        let config: Config = serde_json::from_value(value).unwrap();
        assert!(config.mastering);
        assert_eq!(config.high_cut, 20000.0);
        assert_eq!(config.oversample, 1);
        assert_eq!(config.noise_mode, NoiseMode::Linear);
    }

    #[test]
    fn rejects_non_finite_values_and_invalid_depth() {
        let mut settings = Settings {
            intensity: "NaN".into(),
            ..Settings::default()
        };
        assert!(settings.parse().is_err());
        settings.intensity = "0.5".into();
        settings.bit_depth = "20".into();
        assert!(settings.parse().is_err());
    }
}
