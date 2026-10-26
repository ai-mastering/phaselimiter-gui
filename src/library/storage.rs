//! Read legacy settings and remap numeric identities consistently to UUIDs.
use super::Library;
use crate::model::{AppSettings, Settings};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct SavedLibrary {
    app_settings: Option<AppSettings>,
    draft: Option<Value>,
    settings: Option<Value>,
    tracks: Vec<Value>,
    jobs: Vec<Value>,
    selected: Option<(Value, Option<Value>)>,
    output_side: Option<bool>,
    equalize: Option<bool>,
    comparison_job: Option<Value>,
}

impl TryFrom<SavedLibrary> for Library {
    type Error = String;
    fn try_from(mut saved: SavedLibrary) -> Result<Self, String> {
        let mut ids = std::collections::HashMap::new();
        let mut convert = |value: &mut Value| {
            if let Some(old) = value.as_u64() {
                let id = ids.entry(old).or_insert_with(Uuid::new_v4);
                *value = Value::String(id.to_string());
            }
        };
        for track in &mut saved.tracks {
            if !track.is_object() {
                return Err("Invalid track record".into());
            }
            convert(&mut track["id"]);
        }
        for job in &mut saved.jobs {
            if !job.is_object() {
                return Err("Invalid job record".into());
            }
            convert(&mut job["id"]);
            // Older jobs omitted the original track ID (which defaulted to zero).
            if job.get("track_id").is_none() {
                job["track_id"] = Value::from(0);
            }
            convert(&mut job["track_id"]);
        }
        if let Some((track, job)) = &mut saved.selected {
            convert(track);
            if let Some(job) = job {
                convert(job);
            }
        }
        if let Some(job) = &mut saved.comparison_job {
            convert(job);
        }
        let selected = saved
            .selected
            .map(|value| {
                serde_json::from_value(serde_json::to_value(value).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())
            })
            .transpose()?;
        let comparison_job = saved
            .comparison_job
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        let tracks = saved
            .tracks
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let jobs = saved
            .jobs
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let latest = saved.draft.or(saved.settings);
        let app_settings = match saved.app_settings {
            Some(settings) => settings,
            None => latest
                .as_ref()
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or_default(),
        };
        let draft: Settings = latest
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        Ok(Self {
            app_settings,
            draft,
            tracks,
            jobs,
            selected,
            output_side: saved.output_side,
            equalize: saved.equalize,
            comparison_job,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uuid_library_round_trip_preserves_ids_memos_timestamps_and_manifest_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mix.wav");
        std::fs::write(&source, b"source").unwrap();
        let track_id = Uuid::parse_str("c2d1dd5e-7afe-4027-8b53-15c5e21a0334").unwrap();
        let job_id = Uuid::new_v4();
        let mut library = Library::default();
        let track = crate::library::import(dir.path(), source, track_id).unwrap();
        let input = track.input.clone();
        library.tracks.push(track);
        let mut job = crate::model::test_job(
            input,
            dir.path().join("result.wav"),
            library.draft.parse().unwrap(),
            0,
        );
        job.id = job_id;
        job.track_id = track_id;
        job.created_at = Some("2026-10-26T03:00:00Z".parse().unwrap());
        job.memo = "検証済みのマスター".into();
        job.status = crate::model::Status::Succeeded;
        library.jobs.push(job);
        library.selected = Some((track_id, Some(job_id)));
        library.comparison_job = Some(job_id);
        library.save(dir.path()).unwrap();
        let path = dir.path().join("library.json");
        let original = std::fs::read(&path).unwrap();
        let loaded = Library::load(dir.path()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(loaded.tracks[0].id, track_id);
        assert_eq!(loaded.jobs[0].id, job_id);
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&library).unwrap()
        );
        loaded.save(dir.path()).unwrap();
        let saved = Library::load(dir.path()).unwrap();
        assert_eq!(saved.jobs[0].memo, "検証済みのマスター");
        assert_eq!(saved.jobs[0].created_at, library.jobs[0].created_at);
        assert_eq!(saved.selected, library.selected);
        assert_eq!(saved.comparison_job, library.comparison_job);
    }
    #[test]
    fn legacy_draft_wins_and_job_execution_records_are_preserved() {
        let config = crate::model::Settings::default().parse().unwrap();
        let mut legacy_config = serde_json::to_value(config).unwrap();
        for (key, value) in [
            ("backend", "/old/engine"),
            ("ffmpeg", "/old/ffmpeg"),
            ("cache", "/old/cache"),
            ("output_dir", "/old/export"),
        ] {
            legacy_config[key] = json!(value);
        }
        let old = json!({
            "settings":{"loudness":"-9","backend":"last engine","output_dir":"last export"},
            "draft":{"loudness":"-12","backend":"custom engine","ffmpeg":"ffmpeg","cache":"","output_dir":"current export"},
            "jobs":[{"id":7,"track_id":1,"input":"input.wav","output":"result.wav","status":"Succeeded",
                "progress":1.0,"log":["entire log"],"command":"recorded command","config":legacy_config}],
            "next_id":8,"selected":[1,7]
        });
        let library: Library = serde_json::from_value(old).unwrap();
        assert_eq!(library.draft.loudness, "-12");
        assert_eq!(library.app_settings.backend, "custom engine");
        assert_eq!(library.app_settings.export_dir, "current export");
        assert!(library.app_settings.cache.is_empty());
        assert_eq!(
            library.jobs[0].tools.backend,
            std::path::PathBuf::from("/old/engine")
        );
        assert_eq!(library.jobs[0].command.as_deref(), Some("recorded command"));
        assert_eq!(library.jobs[0].log, ["entire log"]);
        let saved = serde_json::to_value(&library).unwrap();
        assert!(saved.get("settings").is_none());
        for key in ["backend", "ffmpeg", "cache", "output_dir", "export_dir"] {
            assert!(saved["draft"].get(key).is_none());
            assert!(saved["jobs"][0]["config"].get(key).is_none());
        }
        let restored: Library = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.app_settings.export_dir, "current export");
        assert_eq!(restored.jobs[0].tools.ffmpeg, "/old/ffmpeg");
    }

    #[test]
    fn legacy_settings_without_a_draft_remain_readable_and_are_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let bytes =
            br#"{"settings":{"loudness":"-13","backend":"","output_dir":"missing export folder"}}"#;
        std::fs::write(dir.path().join("library.json"), bytes).unwrap();
        let library = Library::load(dir.path()).unwrap();
        assert_eq!(library.draft.loudness, "-13");
        assert_eq!(library.app_settings.export_dir, "missing export folder");
        assert!(library.draft.parse().is_ok());
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("library-before-idname-")
            })
            .unwrap();
        assert_eq!(std::fs::read(backup).unwrap(), bytes);
        library.save(dir.path()).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("library.json")).unwrap())
                .unwrap();
        assert!(saved.get("settings").is_none());
        assert_eq!(Library::load(dir.path()).unwrap().draft.loudness, "-13");
    }
    #[test]
    fn numeric_ids_migrate_with_references_paths_and_exact_manifest_backup() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mix.wav");
        std::fs::write(&source, b"original").unwrap();
        let track = crate::library::import(dir.path(), source, Uuid::new_v4()).unwrap();
        let input = track.input.clone();
        let mut library = Library::default();
        library.tracks.push(track);
        for index in 0..2 {
            library.jobs.push(crate::model::test_job(
                input.clone(),
                dir.path().join(format!("jobs/{index}/output.wav")),
                library.draft.parse().unwrap(),
                index,
            ));
        }
        let paths: Vec<_> = library.jobs.iter().map(|j| j.output.clone()).collect();
        let mut old = serde_json::to_value(library).unwrap();
        old["tracks"][0]["id"] = json!(3);
        for (index, job) in old["jobs"].as_array_mut().unwrap().iter_mut().enumerate() {
            job["id"] = json!(index + 4);
            job["track_id"] = json!(3);
            job.as_object_mut().unwrap().remove("created_at");
            job.as_object_mut().unwrap().remove("memo");
        }
        old["selected"] = json!([3, 5]);
        old["comparison_job"] = json!(4);
        old["next_id"] = json!(6);
        let bytes = serde_json::to_vec(&old).unwrap();
        std::fs::write(dir.path().join("library.json"), &bytes).unwrap();
        let migrated = Library::load(dir.path()).unwrap();
        let track_id = migrated.tracks[0].id;
        assert_eq!(track_id.get_version(), Some(uuid::Version::Random));
        assert_eq!(
            migrated.selected,
            Some((track_id, Some(migrated.jobs[1].id)))
        );
        assert_eq!(migrated.comparison_job, Some(migrated.jobs[0].id));
        for (job, path) in migrated.jobs.iter().zip(paths) {
            assert_eq!(job.id.get_version(), Some(uuid::Version::Random));
            assert_eq!(job.track_id, track_id);
            assert_eq!(job.input, input);
            assert_eq!(job.output, path);
            assert_eq!(job.created_at, None);
            assert!(job.memo.is_empty());
        }
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("library-before-idname-")
            })
            .unwrap();
        assert_eq!(std::fs::read(backup).unwrap(), bytes);
        migrated.save(dir.path()).unwrap();
        let restored = Library::load(dir.path()).unwrap();
        assert_eq!(restored.selected, migrated.selected);
        assert_eq!(restored.comparison_job, migrated.comparison_job);
        let saved = serde_json::to_value(restored).unwrap();
        assert!(saved.get("next_id").is_none());
        assert!(saved["tracks"][0]["id"].is_string());
    }

    #[test]
    fn malformed_records_return_errors() {
        for key in ["tracks", "jobs"] {
            let mut value = json!({});
            value[key] = json!([42]);
            assert!(serde_json::from_value::<Library>(value).is_err());
        }
    }
}
