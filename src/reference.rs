//! The draft stores a source path; prepared reports are transient execution state.
use crate::{App, Element, Field, Message, analysis, library, theme};
use iced::{
    Task,
    widget::{button, column, row, text, text_input},
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub(super) struct Prepared {
    pub analysis: analysis::Analysis,
    pub source_hash: String,
}

#[derive(Default)]
pub(super) struct State {
    pub generation: u64,
    pub due: Option<Instant>,
    pub active: bool,
    pub cancel: Option<Arc<AtomicBool>>,
    pub report: Option<Result<Prepared, String>>,
    pub force: bool,
    pub focused: bool,
    identity: Option<(String, String, String, String)>,
    source_stamp: Option<(u64, Option<std::time::SystemTime>)>,
}

impl App {
    pub(super) fn invalidate_draft_reference(&mut self, immediate: bool) {
        self.draft_reference.generation = self.draft_reference.generation.wrapping_add(1);
        if let Some(cancel) = &self.draft_reference.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.draft_reference.report = None;
        self.draft_reference.due =
            (self.library.draft.mastering && self.library.draft.use_reference).then(|| {
                Instant::now()
                    + if immediate {
                        Duration::ZERO
                    } else {
                        Duration::from_millis(500)
                    }
            });
    }

    pub(super) fn ready_reference(&self) -> Result<&Path, String> {
        self.draft_reference
            .report
            .as_ref()
            .ok_or("Reference analysis is not ready.")?
            .as_ref()
            .map_err(Clone::clone)?
            .analysis
            .path()
            .ok_or_else(|| "Reference analysis is not stored.".into())
    }

    pub(super) fn reference_ready(&self) -> bool {
        !self.library.draft.mastering
            || !self.library.draft.use_reference
            || self.ready_reference().is_ok()
    }

    pub(super) fn start_draft_reference(&mut self) -> Task<Message> {
        let settings = &self.library.app_settings;
        let identity = (
            self.library.draft.reference_audio.clone(),
            settings.backend_path().display().to_string(),
            settings.ffmpeg_command(),
            settings.cache_path().display().to_string(),
        );
        if self.draft_reference.identity.as_ref() != Some(&identity) {
            self.draft_reference.identity = Some(identity);
            // Edits already set their debounce deadline; tool changes invalidate too.
            if self.draft_reference.due.is_none() {
                self.invalidate_draft_reference(true);
            }
        }
        let stamp = std::fs::metadata(&self.library.draft.reference_audio)
            .ok()
            .filter(|m| m.is_file())
            .map(|m| (m.len(), m.modified().ok()));
        if self.draft_reference.source_stamp != stamp {
            self.draft_reference.source_stamp = stamp;
            if self.draft_reference.report.is_some() || self.draft_reference.active {
                self.invalidate_draft_reference(true);
            }
        }
        if self.closing
            || self.analyzing.is_some()
            || !self.library.draft.mastering
            || !self.library.draft.use_reference
            || self.draft_reference.active
            || self
                .draft_reference
                .due
                .is_none_or(|due| due > Instant::now())
        {
            return Task::none();
        }
        self.draft_reference.due = None;
        let Some(root) = self.root.clone() else {
            return Task::none();
        };
        let path = self.library.draft.reference_audio.clone();
        if path.is_empty() {
            self.draft_reference.report = Some(Err("Choose a reference audio file.".into()));
            return Task::none();
        }
        let generation = self.draft_reference.generation;
        let settings = self.library.app_settings.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.draft_reference.cancel = Some(cancel.clone());
        self.draft_reference.active = true;
        let options = analysis::Options {
            force: std::mem::take(&mut self.draft_reference.force),
            cancel: Some(cancel.clone()),
            ..analysis::Options::REFERENCE
        };
        Task::perform(
            async move {
                let input = std::path::PathBuf::from(path);
                let source = input.clone();
                let hash = tokio::task::spawn_blocking(move || library::hash(&source))
                    .await
                    .map_err(|e| e.to_string())??;
                if cancel.load(Ordering::Relaxed) {
                    return Err("Reference analysis cancelled.".into());
                }
                let report = analysis::run_with_options(
                    root,
                    input,
                    hash.clone(),
                    settings.backend_path(),
                    settings.ffmpeg_command(),
                    settings.cache_path(),
                    options,
                )
                .await?;
                Ok(Prepared {
                    analysis: report,
                    source_hash: hash,
                })
            },
            move |result| Message::DraftReferenceAnalyzed(generation, result),
        )
    }

    pub(super) fn reference_controls(&self) -> Element<'_, Message> {
        if !self.library.draft.use_reference {
            return text("Auto (no reference)").size(theme::SMALL_TEXT).into();
        }
        let label = if self.library.draft.reference_audio.is_empty() {
            "Browse…"
        } else {
            "Change…"
        };
        let enabled = self.library.draft.mastering;
        let mut input = text_input("Reference audio file", &self.library.draft.reference_audio)
            .id("reference-path");
        if enabled
            && (self.numeric_errors.contains_key(&Field::Reference)
                || matches!(self.draft_reference.report, Some(Err(_))))
        {
            input = input.style(theme::invalid_input);
        }
        if enabled {
            input = input
                .on_input(Message::ReferencePath)
                .on_submit(Message::ReferenceSubmit);
        }
        let mut controls = column![
            row![
                input,
                button(label)
                    .on_press_maybe(enabled.then_some(Message::BrowseReference))
                    .style(theme::normal)
            ]
            .spacing(theme::SECTION_GAP)
        ]
        .spacing(theme::ITEM_GAP);
        if !enabled {
            return controls.into();
        }
        if !self.draft_reference.active
            && self.draft_reference.due.is_none()
            && matches!(self.draft_reference.report, Some(Err(_)))
            && self.draft_reference.source_stamp.is_some()
        {
            controls = controls.push(
                button("Retry")
                    .on_press(Message::ReferenceSubmit)
                    .style(theme::normal),
            );
        }
        controls.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared(root: &Path) -> Prepared {
        Prepared {
            analysis: analysis::Analysis::store(root, br#"{"bands":[],"covariance":[]}"#).unwrap(),
            source_hash: "test-hash".into(),
        }
    }

    #[test]
    fn edits_debounce_cancel_work_and_ignore_obsolete_results() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App {
            root: Some(dir.path().to_path_buf()),
            ..App::default()
        };
        app.library.draft.use_reference = true;
        let _ = app.update(Message::ReferencePath("first.wav".into()));
        let first = app.draft_reference.generation;
        assert!(app.draft_reference.due.unwrap() > Instant::now());
        assert!(!app.reference_ready());
        let cancel = Arc::new(AtomicBool::new(false));
        app.draft_reference.active = true;
        app.draft_reference.cancel = Some(cancel.clone());
        let _ = app.update(Message::ReferencePath("second.wav".into()));
        assert!(cancel.load(Ordering::Relaxed));
        assert!(
            app.draft_reference.active,
            "wait for cancellation before replacing work"
        );
        let _ = app.update(Message::DraftReferenceAnalyzed(
            first,
            Ok(prepared(dir.path())),
        ));
        assert!(app.draft_reference.report.is_none());
        assert!(!app.reference_ready());
        let generation = app.draft_reference.generation;
        app.draft_reference.due = None;
        let _ = app.update(Message::DraftReferenceAnalyzed(
            generation,
            Ok(prepared(dir.path())),
        ));
        assert!(app.reference_ready());
        let _ = app.update(Message::ReferenceEnabled(false));
        assert!(app.reference_ready());
        assert_eq!(app.library.draft.reference_audio, "second.wav");
        assert!(app.draft_reference.report.is_none());
    }

    #[test]
    fn empty_reference_edits_keep_the_selected_file_and_prepared_report() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::default();
        app.library.draft.use_reference = true;
        app.library.draft.reference_audio = "target.wav".into();
        app.draft_reference.report = Some(Ok(prepared(dir.path())));
        let generation = app.draft_reference.generation;
        let _ = app.update(Message::ReferencePath(String::new()));
        assert_eq!(app.library.draft.reference_audio, "target.wav");
        assert_eq!(app.draft_reference.generation, generation);
        assert!(app.reference_ready());
    }

    #[test]
    fn enter_and_blur_flush_pending_edits_without_restarting_ready_work() {
        let mut app = App::default();
        app.library.draft.use_reference = true;
        app.library.draft.reference_audio = "target.wav".into();
        // No data root means a flushed deadline cannot launch a process in this test.
        app.invalidate_draft_reference(false);
        app.draft_reference.focused = true;
        let _ = app.update(Message::ReferenceFocused(false));
        assert!(!app.draft_reference.focused);
        assert!(app.draft_reference.due.is_none());
        let dir = tempfile::tempdir().unwrap();
        app.draft_reference.report = Some(Ok(prepared(dir.path())));
        let generation = app.draft_reference.generation;
        let _ = app.update(Message::ReferenceSubmit);
        assert_eq!(generation, app.draft_reference.generation);
        assert!(app.reference_ready());
    }
}
