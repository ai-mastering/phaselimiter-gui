//! Durable snapshots. External source/export paths are never used for playback.
mod saver;
use crate::{
    analysis::Analysis,
    model::{Job, Settings, Status},
};
pub use saver::SaveWorker;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub source: PathBuf,
    pub input: PathBuf,
    pub hash: String,
    pub added: u64,
    pub analysis: Option<Analysis>,
    pub analysis_error: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub settings: Settings,
    pub draft: Option<Settings>,
    pub tracks: Vec<Track>,
    pub jobs: Vec<Job>,
    pub next_id: usize,
    pub selected: Option<(usize, Option<usize>)>,
    pub output_side: Option<bool>,
    pub equalize: Option<bool>,
}

pub fn default_root() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    let base = {
        use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask};
        NSFileManager::defaultManager()
            .URLsForDirectory_inDomains(
                NSSearchPathDirectory::ApplicationSupportDirectory,
                NSSearchPathDomainMask::UserDomainMask,
            )
            .firstObject()
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
    };
    #[cfg(not(target_os = "macos"))]
    let base = dirs::data_local_dir();
    base.map(|p| p.join("phaselimiter-gui"))
        .ok_or_else(|| "Could not locate the application data directory.".into())
}

pub fn lock(root: &Path) -> Result<File, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("library.lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "The library is already open in another application instance.".to_string())?;
    Ok(file)
}

pub fn hash(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn verify(path: &Path, expected: &str) -> Result<(), String> {
    if hash(path)? != expected {
        return Err(format!(
            "Stored file has changed: {}. Add the audio again as a new item.",
            path.display()
        ));
    }
    Ok(())
}

pub fn import(root: &Path, source: PathBuf, id: usize) -> Result<Track, String> {
    if !source.is_file() {
        return Err("Choose an existing audio file.".into());
    }
    let dir = root.join("audio").join(id.to_string());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let extension = source.extension().unwrap_or_default();
    let input = dir.join("input").with_extension(extension);
    // Copy through a temporary file so failed imports cannot become registered inputs.
    let mut temporary = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    let before = hash(&source)?;
    io::copy(
        &mut File::open(&source).map_err(|e| e.to_string())?,
        &mut temporary,
    )
    .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    let copied = hash(temporary.path())?;
    if copied != before || hash(&source)? != before {
        return Err("The source changed while importing. Wait for the DAW export to finish and add it again.".into());
    }
    temporary
        .persist_noclobber(&input)
        .map_err(|e| e.to_string())?;
    Ok(Track {
        id,
        name: source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        source,
        input,
        hash: copied,
        added: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        analysis: None,
        analysis_error: None,
    })
}

impl Library {
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("library.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut library: Self =
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Cannot read saved library: {e}"))?;
        for job in &mut library.jobs {
            if job.status == Status::Processing {
                job.status = Status::Interrupted;
            }
        }
        // Reserve above every persisted ID, including libraries saved during an import.
        library.next_id = library.next_id.max(
            library
                .tracks
                .iter()
                .map(|t| t.id)
                .chain(library.jobs.iter().map(|j| j.id))
                .max()
                .map_or(0, |id| id + 1),
        );
        Ok(library)
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
        {
            // A JSON serializer writes many small tokens. Buffer them before
            // reaching the filesystem (especially shared/remote home folders).
            let mut buffered = std::io::BufWriter::with_capacity(65536, &mut temporary);
            serde_json::to_writer(&mut buffered, self).map_err(|e| e.to_string())?;
            buffered.flush().map_err(|e| e.to_string())?;
        }
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        temporary
            .persist(root.join("library.json"))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn export(source: &Path, expected: &str, destination: &Path) -> Result<(), String> {
    verify(source, expected)?;
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut output = File::options()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| {
            format!(
                "Export {}: {e}. Choose a new filename; existing files are preserved.",
                destination.display()
            )
        })?;
    if let Err(error) = io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
        drop(output);
        let _ = fs::remove_file(destination);
        return Err(error.to_string());
    }
    Ok(())
}

pub fn usage(root: &Path) -> u64 {
    let mut total = 0;
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                if let Ok(kind) = entry.file_type() {
                    if kind.is_dir() {
                        directories.push(entry.path());
                    } else if kind.is_file() {
                        total += entry.metadata().map_or(0, |m| m.len());
                    }
                }
            }
        }
    }
    total
}

pub async fn open_folder(path: PathBuf) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    let mut command = tokio::process::Command::new(program);
    command.arg(path);
    let status = command
        .status()
        .await
        .map_err(|e| format!("Opening library folder: {e}"))?;
    if !status.success() {
        return Err("Could not open the library folder.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_instance_can_write_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let first = lock(dir.path()).unwrap();
        assert!(lock(dir.path()).is_err());
        drop(first);
        assert!(lock(dir.path()).is_ok());
    }
    #[test]
    fn snapshots_survive_source_overwrite_and_reimport() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mix.wav");
        fs::write(&source, b"first").unwrap();
        let first = import(dir.path(), source.clone(), 1).unwrap();
        fs::write(&source, b"second").unwrap();
        let second = import(dir.path(), source.clone(), 2).unwrap();
        fs::remove_file(source).unwrap();
        verify(&first.input, &first.hash).unwrap();
        assert_ne!(first.hash, second.hash);
        fs::write(&first.input, b"tampered").unwrap();
        assert!(verify(&first.input, &first.hash).is_err());
    }
    #[test]
    fn restores_history_and_marks_running_work_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = Library::default();
        let source = dir.path().join("mix.wav");
        fs::write(&source, b"audio").unwrap();
        let track = import(dir.path(), source, 1).unwrap();
        state.settings.output_dir = dir.path().display().to_string();
        state.jobs.push(crate::model::test_job(
            track.input.clone(),
            dir.path().join("result.wav"),
            state.settings.parse().unwrap(),
            2,
        ));
        state.jobs[0].status = Status::Processing;
        state.tracks.push(track);
        state.save(dir.path()).unwrap();
        let loaded = Library::load(dir.path()).unwrap();
        assert_eq!(loaded.jobs[0].status, Status::Interrupted);
        assert_eq!(loaded.next_id, 3);
        assert_eq!(loaded.tracks.len(), 1);
    }
    #[test]
    fn export_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("result.wav");
        let dest = dir.path().join("export.wav");
        fs::write(&source, b"result").unwrap();
        let hash = hash(&source).unwrap();
        export(&source, &hash, &dest).unwrap();
        assert!(export(&source, &hash, &dest).is_err());
        assert_eq!(fs::read(dest).unwrap(), b"result");
    }
}
