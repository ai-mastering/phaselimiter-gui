use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct Settings {
    pub backend: String,
    pub ffmpeg: String,
    pub cache: String,
    pub output_dir: String,
    pub loudness: String,
    pub intensity: String,
    pub stereo: String,
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

impl Default for Settings {
    fn default() -> Self {
        let executable = std::env::current_exe().unwrap_or_default();
        let dir = executable.parent().unwrap_or(Path::new("."));
        // In a .app, bundled tools/resources live beside the MacOS directory.
        let root = if dir.file_name().is_some_and(|n| n == "MacOS") {
            dir.parent().unwrap_or(dir).join("Resources")
        } else {
            dir.to_path_buf()
        };
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let output = [home.join("Downloads"), home.join("Desktop"), home]
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let tool = if cfg!(windows) {
            "phase_limiter.exe"
        } else {
            "phase_limiter"
        };
        let ffmpeg = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let bundled_ffmpeg = root.join(ffmpeg);
        Self {
            backend: root
                .join("phaselimiter/bin")
                .join(tool)
                .to_string_lossy()
                .into_owned(),
            ffmpeg: if bundled_ffmpeg.is_file() {
                bundled_ffmpeg.to_string_lossy().into_owned()
            } else {
                ffmpeg.into()
            },
            cache: root
                .join("phaselimiter/resource/sound_quality2_cache")
                .to_string_lossy()
                .into_owned(),
            output_dir: output.to_string_lossy().into_owned(),
            loudness: "-9".into(),
            intensity: "1".into(),
            stereo: "1".into(),
            ceiling: "0".into(),
            sample_rate: "44100".into(),
            bit_depth: "16".into(),
            workers: "0".into(),
            evaluations: "40000".into(),
            seed: "1".into(),
            preserve_bass: false,
            pre_compression: true,
            format: Format::Wav,
            solver: Solver::DePrmm,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub backend: PathBuf,
    pub ffmpeg: String,
    pub cache: PathBuf,
    pub output_dir: PathBuf,
    pub loudness: f64,
    pub intensity: f64,
    pub stereo: f64,
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
        if self.backend.trim().is_empty()
            || self.ffmpeg.trim().is_empty()
            || self.cache.trim().is_empty()
        {
            return Err("Set the backend, FFmpeg and sound-quality cache paths.".into());
        }
        let output_dir = PathBuf::from(&self.output_dir);
        if !output_dir.is_dir() {
            return Err("Output directory must already exist.".into());
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
        Ok(Config {
            backend: PathBuf::from(&self.backend),
            ffmpeg: self.ffmpeg.clone(),
            cache: PathBuf::from(&self.cache),
            output_dir,
            loudness: number(&self.loudness, "Target loudness", -60.0, 0.0)?,
            intensity: number(&self.intensity, "Mastering intensity", 0.0, 1.0)?,
            stereo: number(&self.stereo, "Stereo matching", 0.0, 1.0)?,
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
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Processing,
    Succeeded,
    Failed,
    Cancelled,
}
impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Waiting => "Queued",
            Self::Processing => "Processing",
            Self::Succeeded => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        })
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub input: PathBuf,
    pub output: PathBuf,
    pub config: Config,
    pub status: Status,
    pub progress: f32,
    pub log: Vec<String>,
}

pub fn add_job(
    jobs: &mut Vec<Job>,
    input: PathBuf,
    config: Config,
    id: usize,
) -> Result<(), String> {
    if !input.is_file() {
        return Err(format!("Not a file: {}", input.display()));
    }
    let canonical = input.canonicalize().map_err(|e| e.to_string())?;
    if jobs
        .iter()
        .any(|j| j.input == canonical && matches!(j.status, Status::Waiting | Status::Processing))
    {
        return Err(format!("Already queued: {}", input.display()));
    }
    let stem = input
        .file_stem()
        .ok_or("Input has no filename.")?
        .to_string_lossy();
    let extension = config.format.extension();
    let mut suffix = 1;
    let output = loop {
        let name = if suffix == 1 {
            format!("{stem}_output.{extension}")
        } else {
            format!("{stem}_output_{suffix}.{extension}")
        };
        let candidate = config.output_dir.join(name);
        if !candidate.exists() && !jobs.iter().any(|j| j.output == candidate) {
            break candidate;
        }
        suffix += 1;
    };
    jobs.push(Job {
        id,
        input: canonical,
        output,
        config,
        status: Status::Waiting,
        progress: 0.0,
        log: Vec::new(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[test]
    fn preserves_existing_outputs_and_distinguishes_same_named_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            output_dir: tmp.path().to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let config = settings.parse().unwrap();
        std::fs::write(tmp.path().join("track_output.wav"), b"original").unwrap();
        let mut jobs = Vec::new();
        for (id, dir) in ["one", "two"].into_iter().enumerate() {
            let path = tmp.path().join(dir);
            std::fs::create_dir(&path).unwrap();
            let input = path.join("track.wav");
            std::fs::write(&input, b"audio").unwrap();
            add_job(&mut jobs, input, config.clone(), id).unwrap();
        }
        assert_eq!(jobs[0].output.file_name().unwrap(), "track_output_2.wav");
        assert_eq!(jobs[1].output.file_name().unwrap(), "track_output_3.wav");
        assert!(add_job(&mut jobs.clone(), jobs[0].input.clone(), config, 2).is_err());
        assert_eq!(
            std::fs::read(tmp.path().join("track_output.wav")).unwrap(),
            b"original"
        );
    }
}
