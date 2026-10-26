//! Comparison selection and historical reference snapshots; neither changes the draft.
use crate::{App, Job, Message, Settings, Status, analysis, model};
use iced::Task;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ComparisonChoice {
    pub job: Option<Uuid>,
    pub label: String,
}
impl std::fmt::Display for ComparisonChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
impl App {
    pub(super) fn comparison_job(&self) -> Option<&Job> {
        let (track, selected) = self.selected?;
        self.library.jobs.iter().find(|j| {
            Some(j.id) == self.comparison_job
                && j.track_id == track
                && j.id != selected
                && j.status == Status::Succeeded
        })
    }
    pub(super) fn comparison_choices(&self) -> Vec<ComparisonChoice> {
        let mut choices = vec![ComparisonChoice {
            job: None,
            label: "Original".into(),
        }];
        if let Some((track, selected)) = self.selected {
            choices.extend(
                self.library
                    .jobs
                    .iter()
                    .filter(|j| {
                        j.track_id == track && j.status == Status::Succeeded && j.id != selected
                    })
                    .map(|j| ComparisonChoice {
                        job: Some(j.id),
                        label: self.library.job_label(j),
                    }),
            );
        }
        choices
    }
    pub(super) fn comparison_label(&self) -> String {
        self.comparison_job()
            .map_or_else(|| "Original".into(), |j| self.library.job_label(j))
    }
    pub(super) fn comparison_analysis(&self) -> Option<&analysis::Analysis> {
        if let Some(job) = self.comparison_job() {
            job.analysis.as_ref()
        } else {
            self.track()?.analysis.as_ref()
        }
    }
    fn reference_path(&self) -> Option<&PathBuf> {
        self.job()?.config.reference.as_ref()
    }
    pub(super) fn reference_analysis(&self) -> Option<&analysis::Analysis> {
        let path = self.reference_path()?;
        self.reference_report
            .as_ref()
            .filter(|(p, _)| p == path)?
            .1
            .as_ref()
            .ok()
    }
    pub(super) fn reference_name(&self) -> String {
        let Some(path) = self.reference_path() else {
            return "Auto (no reference)".into();
        };
        if let Some(source) = self.job().and_then(|j| j.config.reference_audio.as_ref()) {
            return source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        self.library
            .tracks
            .iter()
            .find(|t| t.analysis.as_ref().and_then(|a| a.path()) == Some(path.as_path()))
            .map_or_else(
                || {
                    format!(
                        "saved report {}",
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    )
                },
                |t| t.name.clone(),
            )
    }
    pub(super) fn load_reference(&mut self) -> Task<Message> {
        let Some(path) = self.reference_path().cloned() else {
            self.reference_report = None;
            self.reference_pending = None;
            return Task::none();
        };
        if self
            .reference_report
            .as_ref()
            .is_some_and(|(p, _)| *p == path)
            || self.reference_pending.as_ref() == Some(&path)
        {
            return Task::none();
        }
        self.reference_report = None;
        self.reference_pending = Some(path.clone());
        Task::perform(
            async move {
                let report_path = path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    analysis::Analysis::read_report(report_path)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
                (path, result)
            },
            |(path, result)| Message::ReferenceLoaded(path, result),
        )
    }
}

pub(super) fn settings_differences(
    config: &model::Config,
    draft: &Settings,
) -> Vec<(&'static str, String, String)> {
    let Ok(old) = serde_json::to_value(crate::settings_from_config(config)) else {
        return Vec::new();
    };
    let Ok(new) = serde_json::to_value(draft) else {
        return Vec::new();
    };
    const FIELDS: &[(&str, &str)] = &[
        ("reference_mode", "Volume reference"),
        ("loudness", "Target"),
        ("ceiling_mode", "Peak control"),
        ("ceiling", "Ceiling"),
        ("mastering", "Mastering"),
        ("intensity", "Intensity"),
        ("reference", "Reference report"),
        ("use_reference", "Use reference audio"),
        ("reference_audio", "Reference audio"),
        ("preserve_bass", "Preserve bass"),
        ("pre_compression", "Pre-compression"),
        ("lowpass_cut", "Lowpass peak Hz"),
        ("low_cut", "Low cut"),
        ("high_cut", "High cut"),
        ("format", "Format"),
        ("sample_rate", "Sample rate"),
        ("bit_depth", "WAV bits"),
        ("oversample", "Oversampling"),
        ("max_iter1", "Outer iterations"),
        ("max_iter2", "Inner iterations"),
        ("noise_mode", "Noise update"),
        ("solver", "Solver"),
        ("evaluations", "Evaluations"),
        ("seed", "Seed"),
        ("workers", "Workers"),
    ];
    FIELDS
        .iter()
        .filter_map(|&(key, label)| {
            let a = old.get(key)?;
            let b = new.get(key)?;
            if a == b {
                return None;
            }
            if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
                // Integer comparison first preserves all 64 seed bits.
                let equal = match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(a), Ok(b)) => a == b,
                    _ if key != "seed" => match (a.parse::<f64>(), b.parse::<f64>()) {
                        (Ok(a), Ok(b)) => a == b,
                        _ => false,
                    },
                    _ => false,
                };
                if equal {
                    return None;
                }
            }
            let show = |v: &serde_json::Value| {
                v.as_str().map_or_else(
                    || v.to_string(),
                    |s| {
                        if s.is_empty() {
                            "default".into()
                        } else {
                            s.into()
                        }
                    },
                )
            };
            Some((label, show(a), show(b)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn differences_ignore_numeric_spelling_but_preserve_large_seed_changes() {
        let mut draft = Settings {
            seed: (u64::MAX - 1).to_string(),
            ..Settings::default()
        };
        let config = draft.parse().unwrap();
        draft.loudness = "-9.0".into();
        assert!(settings_differences(&config, &draft).is_empty());
        draft.seed = u64::MAX.to_string();
        draft.intensity = "0.75".into();
        let changes = settings_differences(&config, &draft);
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|(label, _, _)| *label == "Intensity"));
        assert!(changes.iter().any(|(label, _, _)| *label == "Seed"));
    }
}
