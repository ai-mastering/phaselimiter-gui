//! Durable snapshots. External source/export paths are never used for playback.
use uuid::Uuid;
mod saver;
mod storage;
use crate::{
    analysis::Analysis,
    model::{AppSettings, Job, Settings, Status},
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
    pub id: Uuid,
    pub name: String,
    pub source: PathBuf,
    pub input: PathBuf,
    pub hash: String,
    pub added: u64,
    pub analysis: Option<Analysis>,
    pub analysis_error: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(try_from = "storage::SavedLibrary")]
pub struct Library {
    pub app_settings: AppSettings,
    pub draft: Settings,
    pub tracks: Vec<Track>,
    pub jobs: Vec<Job>,
    pub selected: Option<(Uuid, Option<Uuid>)>,
    pub output_side: Option<bool>,
    pub equalize: Option<bool>,
    pub comparison_job: Option<Uuid>,
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

pub fn import(root: &Path, source: PathBuf, id: Uuid) -> Result<Track, String> {
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
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let document: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read saved library: {e}"))?;
        let legacy = document.get("app_settings").is_none()
            || document.get("next_id").is_some()
            || ["tracks", "jobs"].iter().any(|key| {
                document
                    .get(key)
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|items| {
                        items
                            .iter()
                            .any(|item| item.get("id").is_some_and(serde_json::Value::is_number))
                    })
            });
        let mut library: Self = serde_json::from_value(document)
            .map_err(|e| format!("Cannot read saved library: {e}"))?;
        if legacy {
            // Old builds cannot read UUID identities. Preserve the exact
            // manifest before this build can save a migrated snapshot.
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let backup = root.join(format!("library-before-idname-{digest}.json"));
            if backup.exists() {
                verify(&backup, &digest)?;
            } else {
                let mut temporary =
                    tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
                temporary
                    .write_all(&bytes)
                    .and_then(|_| temporary.as_file().sync_all())
                    .map_err(|e| format!("Backing up the old library: {e}"))?;
                match temporary.persist_noclobber(&backup) {
                    Ok(_) => {}
                    Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
                        verify(&backup, &digest)?
                    }
                    Err(e) => return Err(format!("Backing up the old library: {e}")),
                }
            }
        }
        for (analysis, error) in library
            .tracks
            .iter_mut()
            .map(|t| (&mut t.analysis, &mut t.analysis_error))
            .chain(
                library
                    .jobs
                    .iter_mut()
                    .map(|j| (&mut j.analysis, &mut j.analysis_error)),
            )
        {
            if let Some(analysis) = analysis {
                if let Err(reason) = analysis.load_report() {
                    *error = Some(format!(
                        "Analysis unavailable: {reason}. Analyze this audio again."
                    ));
                } else {
                    *error = None;
                }
            }
        }
        for job in &mut library.jobs {
            if job.status == Status::Processing {
                job.status = Status::Interrupted;
            }
        }
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

pub fn export(source: &Path, expected: &str, destination: &Path) -> Result<PathBuf, String> {
    verify(source, expected)?;
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Export {}: {e}", parent.display()))?;
    }
    let mut path = destination.to_path_buf();
    let mut number = 0_u64;
    let mut output = loop {
        match File::options().write(true).create_new(true).open(&path) {
            Ok(file) => break file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                number += 1;
                let mut name = destination.file_stem().unwrap_or_default().to_os_string();
                name.push(format!(" ({number})"));
                if let Some(extension) = destination.extension() {
                    name.push(".");
                    name.push(extension);
                }
                path = destination.with_file_name(name);
            }
            Err(e) => return Err(format!("Export {}: {e}", path.display())),
        }
    };
    if let Err(error) = io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
        drop(output);
        let _ = fs::remove_file(&path);
        return Err(error.to_string());
    }
    Ok(path)
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
        let first = import(dir.path(), source.clone(), Uuid::from_u128(1)).unwrap();
        fs::write(&source, b"second").unwrap();
        let second = import(dir.path(), source.clone(), Uuid::from_u128(2)).unwrap();
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
        let track = import(dir.path(), source, Uuid::from_u128(1)).unwrap();
        state.app_settings.export_dir = dir.path().display().to_string();
        state.jobs.push(crate::model::test_job(
            track.input.clone(),
            dir.path().join("result.wav"),
            state.draft.parse().unwrap(),
            2,
        ));
        state.jobs[0].status = Status::Processing;
        state.tracks.push(track);
        state.save(dir.path()).unwrap();
        let loaded = Library::load(dir.path()).unwrap();
        assert_eq!(loaded.jobs[0].status, Status::Interrupted);
        assert_eq!(loaded.jobs[0].id, state.jobs[0].id);
        assert_eq!(loaded.tracks.len(), 1);
    }
    #[test]
    fn preserves_analysis_artifact_across_saves() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mix.wav");
        fs::write(&source, b"audio").unwrap();
        let mut track = import(dir.path(), source, Uuid::from_u128(0)).unwrap();
        let bytes = br#"{"loudness":-9.200000000000003,"bands":[]}"#;
        track.analysis = Some(Analysis::store(dir.path(), bytes).unwrap());
        let mut state = Library::default();
        state.tracks.push(track);
        state.save(dir.path()).unwrap();
        let mut loaded = Library::load(dir.path()).unwrap();
        let artifact = loaded.tracks[0]
            .analysis
            .as_ref()
            .unwrap()
            .path()
            .unwrap()
            .to_path_buf();
        assert_eq!(fs::read(&artifact).unwrap(), bytes);
        loaded.selected = Some((Uuid::from_u128(0), None));
        loaded.save(dir.path()).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("library.json")).unwrap()).unwrap();
        assert!(manifest["tracks"][0]["analysis"].get("loudness").is_none());
        assert!(
            manifest["tracks"][0]["analysis"]
                .get("analysis_file")
                .is_some()
        );
        assert_eq!(fs::read(&artifact).unwrap(), bytes);
        let restored = Library::load(dir.path()).unwrap();
        assert_eq!(
            restored.tracks[0].analysis.as_ref().unwrap().path(),
            Some(artifact.as_path())
        );
        fs::write(&artifact, b"{}").unwrap();
        let damaged = Library::load(dir.path()).unwrap();
        assert!(
            damaged.tracks[0]
                .analysis_error
                .as_ref()
                .unwrap()
                .contains("Analysis unavailable")
        );
        damaged.save(dir.path()).unwrap();
        assert_eq!(
            Library::load(dir.path()).unwrap().tracks[0]
                .analysis
                .as_ref()
                .unwrap()
                .path(),
            Some(artifact.as_path())
        );
    }

    #[test]
    fn missing_report_is_local_to_its_audio_and_keeps_historical_reference() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mix.wav");
        fs::write(&source, b"audio").unwrap();
        let mut state = Library::default();
        for id in 0..2 {
            let mut track = import(dir.path(), source.clone(), Uuid::from_u128(id)).unwrap();
            track.analysis = Some(
                Analysis::store(
                    dir.path(),
                    &serde_json::to_vec(&serde_json::json!({"loudness": -10.0-id as f64})).unwrap(),
                )
                .unwrap(),
            );
            state.tracks.push(track);
        }
        let path = state.tracks[0]
            .analysis
            .as_ref()
            .unwrap()
            .path()
            .unwrap()
            .to_path_buf();
        let mut config = state.draft.parse().unwrap();
        config.reference = Some(path.clone());
        let mut job = crate::model::test_job(
            state.tracks[1].input.clone(),
            dir.path().join("output.wav"),
            config,
            2,
        );
        job.reference_hash = Some(hash(&path).unwrap());
        state.jobs.push(job);
        state.save(dir.path()).unwrap();
        fs::remove_file(&path).unwrap();
        let loaded = Library::load(dir.path()).unwrap();
        assert!(loaded.tracks[0].analysis_error.is_some());
        assert!(!loaded.tracks[0].analysis.as_ref().unwrap().available());
        assert!(loaded.tracks[1].analysis_error.is_none());
        assert_eq!(
            loaded.tracks[1]
                .analysis
                .as_ref()
                .unwrap()
                .number("loudness"),
            Some(-11.)
        );
        verify(&loaded.tracks[0].input, &loaded.tracks[0].hash).unwrap();
        loaded.save(dir.path()).unwrap();
        let restored = Library::load(dir.path()).unwrap();
        assert_eq!(restored.jobs[0].config.reference.as_ref(), Some(&path));
        assert!(restored.tracks[0].analysis_error.is_some());
    }

    #[test]
    fn export_creates_destination_directory_and_preserves_extensionless_names() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        fs::write(&source, b"audio").unwrap();
        let hash = hash(&source).unwrap();
        let destination = dir.path().join("new/nested/audio");
        assert_eq!(export(&source, &hash, &destination).unwrap(), destination);
        let numbered = export(&source, &hash, &destination).unwrap();
        assert_eq!(numbered, dir.path().join("new/nested/audio (1)"));
        assert_eq!(fs::read(numbered).unwrap(), b"audio");
        assert!(export(&source, "wrong hash", &destination).is_err());
        assert!(!dir.path().join("new/nested/audio (2)").exists());
    }

    #[test]
    fn simultaneous_exports_reserve_distinct_names() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        fs::write(&source, b"audio").unwrap();
        let hash = hash(&source).unwrap();
        let destination = dir.path().join("export.wav");
        let barrier = std::sync::Barrier::new(4);
        let paths = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        export(&source, &hash, &destination).unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(unique.len(), 4);
        for path in paths {
            assert_eq!(fs::read(path).unwrap(), b"audio");
        }
    }

    #[test]
    fn export_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("result.wav");
        let dest = dir.path().join("export.wav");
        fs::write(&source, b"result").unwrap();
        let hash = hash(&source).unwrap();
        export(&source, &hash, &dest).unwrap();
        fs::write(&dest, b"existing").unwrap();
        fs::write(dir.path().join("export (1).wav"), b"occupied").unwrap();
        let copy = export(&source, &hash, &dest).unwrap();
        assert_eq!(copy, dir.path().join("export (2).wav"));
        assert_eq!(fs::read(copy).unwrap(), b"result");
        assert_eq!(fs::read(dest).unwrap(), b"existing");
    }
}
