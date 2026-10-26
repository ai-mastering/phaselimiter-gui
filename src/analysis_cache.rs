//! Rebuildable analysis cache. Immutable reports used by history live separately.
use crate::{analysis, library};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

fn detector_reference(recorded: &str, preparation: &Path) -> Result<PathBuf, String> {
    let portable = recorded.replace('\\', "/");
    let path = PathBuf::from(&portable);
    if path.is_absolute() {
        return Ok(path);
    }
    let parent = preparation.parent().ok_or("Missing preparation parent")?;
    if let Some(offset) = portable.rfind("analysis_data/") {
        let relocated = parent.join(&portable[offset..]);
        if relocated.is_file() {
            return Ok(relocated);
        }
    }
    let relative = parent.join(&path);
    if relative.is_file() {
        return Ok(relative);
    }
    if path.is_file() {
        return Ok(path);
    }
    Err(format!("Cannot locate detector reference: {recorded}"))
}

pub fn entry(
    input_hash: &str,
    backend: &Path,
    ffmpeg: &Path,
    quality_cache: &Path,
    options: &analysis::Options,
) -> Result<PathBuf, String> {
    let mut recipe = json!({
        "revision": 2,
        "input": input_hash,
        "analyzer": library::hash(&analysis::executable(backend))?,
        "ffmpeg": library::hash(ffmpeg)?,
        "decode": [ "-vn", "-ac", "2", "-c:a", "pcm_f32le", "-f", "wav"],
        "analyze": {"drr":true,"sound_quality":false,"sound_quality2":options.sound_quality2,
                    "analysis_for_visualization":true},
        "detect2": options.detect2,
    });
    if options.sound_quality2 {
        recipe["quality2"] = json!({"hash":library::hash(quality_cache)?, "archiver":"binary"});
    }
    if options.detect2 {
        let detector = backend.with_file_name(if cfg!(windows) {
            "audio_detector.exe"
        } else {
            "audio_detector"
        });
        let preparation = backend
            .parent()
            .and_then(Path::parent)
            .ok_or("Missing engine root")?
            .join("resource/detection_preparation.json");
        let bytes = std::fs::read(&preparation).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let paths = value["paths"].as_array().ok_or("Missing detector paths")?;
        let mut corpus = Vec::new();
        for path in paths {
            let recorded = path.as_str().ok_or("Invalid detector path")?;
            corpus.push(library::hash(&detector_reference(recorded, &preparation)?)?);
        }
        recipe["detector"] = json!({"executable":library::hash(&detector)?,
            "preparation":library::hash(&preparation)?, "corpus":corpus,
            "task":"detect2", "neighbor_ratio":0.01, "outlier_ratio":0.1});
    }
    let bytes = serde_json::to_vec(&recipe).map_err(|e| e.to_string())?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(dirs::cache_dir()
        .ok_or("Cache directory unavailable")?
        .join("phaselimiter-gui/analysis")
        .join(format!("{digest}.json")))
}

pub fn store(entry: &Path, bytes: &[u8], image: &Path) -> Result<(), String> {
    let parent = entry.parent().ok_or("Cache parent unavailable")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    if image.is_file() {
        std::fs::copy(image, entry.with_extension("frequency.png")).map_err(|e| e.to_string())?;
    }
    file.persist(entry).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: tempfile::TempDir,
        backend: PathBuf,
        ffmpeg: PathBuf,
        cache: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join("bin")).unwrap();
            std::fs::create_dir_all(dir.path().join("resource/analysis_data")).unwrap();
            let backend = dir.path().join("bin/phase_limiter");
            let ffmpeg = dir.path().join("ffmpeg");
            let cache = dir.path().join("quality2");
            for path in [
                &analysis::executable(&backend),
                &backend.with_file_name(if cfg!(windows) {
                    "audio_detector.exe"
                } else {
                    "audio_detector"
                }),
                &ffmpeg,
                &cache,
                &dir.path().join("resource/analysis_data/a.json"),
            ] {
                std::fs::write(path, path.to_string_lossy().as_bytes()).unwrap();
            }
            std::fs::write(
                dir.path().join("resource/detection_preparation.json"),
                br#"{"paths":["./resource/analysis_data/a.json"]}"#,
            )
            .unwrap();
            Self {
                dir,
                backend,
                ffmpeg,
                cache,
            }
        }
        fn key(&self, options: &analysis::Options) -> PathBuf {
            entry(
                "source-hash",
                &self.backend,
                &self.ffmpeg,
                &self.cache,
                options,
            )
            .unwrap()
        }
    }

    #[test]
    fn keys_follow_active_dependencies_including_detector_corpus() {
        let fixture = Fixture::new();
        let full = analysis::Options::FULL;
        let reference = analysis::Options::REFERENCE;
        let mut previous = fixture.key(&full);
        let reference_key = fixture.key(&reference);
        let detector = fixture.backend.with_file_name(if cfg!(windows) {
            "audio_detector.exe"
        } else {
            "audio_detector"
        });
        for path in [
            &fixture.cache,
            &detector,
            &fixture.dir.path().join("resource/analysis_data/a.json"),
        ] {
            std::fs::write(path, b"changed dependency").unwrap();
            let next = fixture.key(&full);
            assert_ne!(
                previous,
                next,
                "{} must invalidate full analysis",
                path.display()
            );
            assert_eq!(reference_key, fixture.key(&reference));
            previous = next;
        }
        for path in [&analysis::executable(&fixture.backend), &fixture.ffmpeg] {
            let previous_reference = fixture.key(&reference);
            std::fs::write(path, b"changed executable").unwrap();
            assert_ne!(previous_reference, fixture.key(&reference));
        }
        let force = analysis::Options {
            force: true,
            ..analysis::Options::FULL
        };
        assert_eq!(fixture.key(&full), fixture.key(&force));
        assert_ne!(fixture.key(&reference), fixture.key(&full));
        assert_ne!(
            fixture.key(&reference),
            entry(
                "different-source",
                &fixture.backend,
                &fixture.ffmpeg,
                &fixture.cache,
                &reference
            )
            .unwrap()
        );
    }

    #[test]
    fn disabled_features_need_no_models_or_detector() {
        let fixture = Fixture::new();
        std::fs::remove_dir_all(fixture.dir.path().join("resource")).unwrap();
        std::fs::remove_file(&fixture.cache).unwrap();
        assert!(
            entry(
                "source",
                &fixture.backend,
                &fixture.ffmpeg,
                &fixture.cache,
                &analysis::Options::REFERENCE
            )
            .is_ok()
        );
        assert!(
            entry(
                "source",
                &fixture.backend,
                &fixture.ffmpeg,
                &fixture.cache,
                &analysis::Options::FULL
            )
            .is_err()
        );
    }
}
