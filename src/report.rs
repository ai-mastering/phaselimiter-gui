//! On-demand diagnostic snapshots in an independent, read-only window.
use crate::{App, Element, Message, theme};
use iced::widget::{button, column, container, row, space, text_editor};
use iced::{Fill, Task, window};
use serde_json::{Value, json};
use std::io::{Read, Seek, SeekFrom};

pub(super) struct Report {
    pub id: window::Id,
    content: text_editor::Content,
}

#[derive(Debug, Clone)]
pub(super) enum Event {
    Open,
    Refresh,
    Select(text_editor::Action),
    Close,
}

impl App {
    pub(super) fn update_report(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Open => {
                if let Some(report) = &self.report {
                    return window::gain_focus(report.id);
                }
                let content = text_editor::Content::with_text(&self.report_text());
                let (id, open) = window::open(window::Settings {
                    size: iced::Size::new(720., 520.),
                    min_size: Some(iced::Size::new(440., 280.)),
                    position: window::Position::Centered,
                    exit_on_close_request: false,
                    ..Default::default()
                });
                self.report = Some(Report { id, content });
                return open.map(Message::WindowOpened);
            }
            Event::Refresh => {
                if self.report.is_some() {
                    let content = text_editor::Content::with_text(&self.report_text());
                    if let Some(report) = &mut self.report {
                        report.content = content;
                    }
                }
            }
            Event::Select(action) => {
                if !action.is_edit()
                    && let Some(report) = &mut self.report
                {
                    report.content.perform(action);
                }
            }
            Event::Close => {
                if let Some(report) = self.report.take() {
                    return window::close(report.id);
                }
            }
        }
        Task::none()
    }

    pub(super) fn report_view(&self) -> Element<'_, Message> {
        let Some(report) = &self.report else {
            return space().into();
        };
        container(
            column![
                text_editor(&report.content)
                    .font(iced::Font::MONOSPACE)
                    .size(theme::SMALL_TEXT)
                    .height(Fill)
                    .on_action(|action| Message::Report(Event::Select(action))),
                row![
                    button("Copy report").on_press(Message::CopyText(report.content.text())),
                    button("Refresh").on_press(Message::Report(Event::Refresh)),
                    space().width(Fill),
                    button("Close").on_press(Message::Report(Event::Close)),
                ]
                .spacing(theme::SECTION_GAP),
            ]
            .spacing(theme::SECTION_GAP),
        )
        .padding(12)
        .into()
    }

    fn report_text(&self) -> String {
        let jobs: Vec<Value> = self.library.jobs.iter().filter(|job| {
            self.selected.is_some_and(|key| key.1 == job.id)
                || self.active.as_ref().is_some_and(|active| active.id == job.id)
        }).map(|job| {
            let start = job.log.len().saturating_sub(200);
            json!({
                "id": job.id, "status": job.status, "progress": job.progress,
                "input": job.input, "output": job.output, "config": job.config,
                "tools": job.tools, "command": job.command, "analysis_error": job.analysis_error,
                "log_total_lines": job.log.len(), "log_tail": &job.log[start..],
            })
        }).collect();
        let audio_log = self
            .root
            .as_ref()
            .map(|root| log_tail(&root.join("audio-diagnostics.log")));
        let report = json!({
            "report_version": 1,
            "captured_at": chrono::Utc::now().to_rfc3339(),
            "application": { "version": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
            "data_directory": self.root,
            "settings": self.library.app_settings,
            "mastering_draft": self.library.draft,
            "selection": self.selected,
            "input": self.track().map(|track| json!({"name": track.name, "source": track.source, "input": track.input, "analysis_error": track.analysis_error})),
            "status_messages": self.status_messages(),
            "queue": { "running": self.running, "active": self.active.as_ref().map(|active| active.id),
                "waiting": self.library.jobs.iter().filter(|job| job.status == crate::Status::Waiting).count() },
            "playback": { "loaded": self.loaded, "loading": self.loading, "active_key": self.active_key,
                "player": self.player.status(), "equalize": self.equalize },
            "analysis": { "active": self.analyzing, "queued": self.analysis_queue, "reference_pending": self.reference_pending },
            "importing": self.importing,
            "selected_and_active_jobs": jobs,
            "audio_log": audio_log,
        });
        // Value serialization cannot fail for the fields assembled above.
        report.to_string()
    }
}

fn log_tail(path: &std::path::Path) -> Value {
    const LIMIT: u64 = 64 * 1024;
    let read = || -> std::io::Result<(u64, String)> {
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        file.seek(SeekFrom::Start(len.saturating_sub(LIMIT)))?;
        let mut bytes = Vec::new();
        file.take(LIMIT).read_to_end(&mut bytes)?;
        Ok((len, String::from_utf8_lossy(&bytes).into_owned()))
    };
    match read() {
        Ok((len, tail)) => {
            json!({ "path": path, "total_bytes": len, "truncated": len > LIMIT, "tail": tail })
        }
        Err(error) => json!({ "path": path, "read_error": error.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_analysis_errors_are_visible_copyable_and_selectable() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input.wav");
        std::fs::write(&source, b"audio").unwrap();
        let track = crate::library::import(dir.path(), source, uuid::Uuid::nil()).unwrap();
        let mut app = App::default();
        for id in [1, 2] {
            let mut job = crate::model::test_job(
                track.input.clone(),
                dir.path().join(format!("output-{id}.wav")),
                app.library.draft.parse().unwrap(),
                id,
            );
            job.status = crate::Status::Succeeded;
            app.library.jobs.push(job);
        }
        app.library.tracks.push(track);
        app.selected = Some((uuid::Uuid::nil(), uuid::Uuid::from_u128(1)));
        app.comparison_job = Some(uuid::Uuid::from_u128(2));
        app.library.jobs[1].analysis_error = Some("Saved comparison report is missing".into());
        let error = "Compare A analysis: Saved comparison report is missing";
        assert!(app.status_messages().iter().any(|m| m == error));
        let _ = app.update_report(Event::Open);
        let snapshot = app.report.as_ref().unwrap().content.text();
        let value: Value = serde_json::from_str(&snapshot).unwrap();
        assert!(
            value["status_messages"]
                .as_array()
                .unwrap()
                .contains(&json!(error))
        );
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(720., 520.),
            app.report_view(),
        );
        ui.click("Copy report").unwrap();
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::CopyText(text) if text == snapshot))
        );
        let _ = app.update_report(Event::Select(text_editor::Action::SelectAll));
        let _ = app.update(Message::Tick);
        assert_eq!(
            app.report.as_ref().unwrap().content.selection().as_deref(),
            Some(snapshot.as_str())
        );
    }

    #[test]
    fn snapshot_is_read_only_copyable_and_refreshes_only_on_request() {
        let error = format!("Import failed\n{}\nlast line", "日本語 path/".repeat(200));
        let mut app = App {
            error: Some(error.clone()),
            ..App::default()
        };
        let _ = app.update_report(Event::Open);
        let id = app.report.as_ref().unwrap().id;
        let original = app.report.as_ref().unwrap().content.text();
        let parsed: Value = serde_json::from_str(&original).unwrap();
        assert!(
            parsed["status_messages"]
                .as_array()
                .unwrap()
                .contains(&json!(error))
        );
        let _ = app.update_report(Event::Select(text_editor::Action::SelectAll));
        let _ = app.update_report(Event::Select(text_editor::Action::Edit(
            text_editor::Edit::Insert('x'),
        )));
        app.error = Some("Changed".into());
        let _ = app.update(Message::Tick);
        assert_eq!(app.report.as_ref().unwrap().content.text(), original);
        assert_eq!(
            app.report.as_ref().unwrap().content.selection().as_deref(),
            Some(original.as_str())
        );
        let _ = app.update_report(Event::Open);
        assert_eq!(app.report.as_ref().unwrap().id, id);
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(440., 280.),
            app.window_view(id),
        );
        ui.click("Copy report").unwrap();
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::CopyText(value) if value == original))
        );
        let _ = app.update_report(Event::Refresh);
        assert!(
            app.report
                .as_ref()
                .unwrap()
                .content
                .text()
                .contains("Changed")
        );
        // An independent report never shields the workspace.
        let mut ui = iced_test::simulator(app.view());
        ui.click("Master audio…").unwrap();
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::BrowseAudio))
        );
        let _ = app.open_preferences();
        let _ = app.window_input(
            id,
            iced::Event::Window(window::Event::CloseRequested),
            iced::event::Status::Ignored,
        );
        assert!(app.report.is_none());
        assert!(app.preferences.is_some());
        assert!(!app.closing);
    }

    #[test]
    fn audio_log_reads_a_bounded_tail_and_reports_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.log");
        assert!(log_tail(&path)["read_error"].is_string());
        std::fs::write(&path, format!("{}END", "x".repeat(100_000))).unwrap();
        let value = log_tail(&path);
        assert_eq!(value["truncated"], true);
        let tail = value["tail"].as_str().unwrap();
        assert_eq!(tail.len(), 64 * 1024);
        assert!(tail.ends_with("END"));
    }
}
