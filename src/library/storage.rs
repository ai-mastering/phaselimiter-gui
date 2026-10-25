//! One-way migration of the old mixed settings/draft format.
use super::{Library, Track};
use crate::model::{AppSettings, Job, Settings};
use serde::Deserialize;
use serde_json::Value;

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct SavedLibrary {
    app_settings: Option<AppSettings>,
    draft: Option<Value>,
    settings: Option<Value>,
    tracks: Vec<Track>,
    jobs: Vec<Job>,
    next_id: usize,
    selected: Option<(usize, Option<usize>)>,
    output_side: Option<bool>,
    equalize: Option<bool>,
    comparison_job: Option<usize>,
}

impl TryFrom<SavedLibrary> for Library {
    type Error = String;
    fn try_from(saved: SavedLibrary) -> Result<Self, String> {
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
            tracks: saved.tracks,
            jobs: saved.jobs,
            next_id: saved.next_id,
            selected: saved.selected,
            output_side: saved.output_side,
            equalize: saved.equalize,
            comparison_job: saved.comparison_job,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
                    .starts_with("library-before-settings-separation-")
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
}
