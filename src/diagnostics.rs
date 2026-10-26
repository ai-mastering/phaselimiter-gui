//! Selectable, read-only error text. Selection survives ordinary playback ticks.
use crate::{App, Element, Message};
use iced::widget::{button, row, text, text_editor};

pub(super) struct Diagnostic {
    pub text: String,
    pub content: text_editor::Content,
}

impl App {
    pub(super) fn refresh_diagnostics(&mut self) {
        let mut errors = Vec::new();
        if let Some(error) = &self.error {
            errors.push(error.clone());
        }
        if let Some(track) = self.track() {
            if let Some(error) = &track.analysis_error {
                errors.push(format!("Original analysis: {error}"));
            }
            if let Some(error) = self.job().and_then(|job| job.analysis_error.as_ref()) {
                errors.push(format!("Selected analysis: {error}"));
            }
            for (label, report) in [
                ("Original", track.analysis.as_ref()),
                (
                    "Compare A",
                    self.comparison_job().and_then(|j| j.analysis.as_ref()),
                ),
                ("Selected", self.job().and_then(|j| j.analysis.as_ref())),
                ("Reference", self.reference_analysis()),
            ] {
                if let Some(error) = report.and_then(|a| a.sound_quality_error()) {
                    errors.push(format!("{label} Professionality: {error}"));
                }
            }
            if self.job().is_some_and(|j| j.config.reference.is_some())
                && let Some((_, Err(error))) = &self.reference_report
            {
                errors.push(format!("Reference analysis unavailable: {error}"));
            }
        }
        self.diagnostics.retain(|d| errors.contains(&d.text));
        for text in errors {
            if !self.diagnostics.iter().any(|d| d.text == text) {
                let content = text_editor::Content::with_text(&text);
                self.diagnostics.push(Diagnostic { text, content });
            }
        }
    }

    pub(super) fn error_view(&self, error: impl Into<String>) -> Element<'_, Message> {
        let error = error.into();
        let content: Element<'_, Message> =
            if let Some(diagnostic) = self.diagnostics.iter().find(|d| d.text == error) {
                let key = error.clone();
                text_editor(&diagnostic.content)
                    .size(12)
                    .padding(3)
                    .height(iced::Shrink)
                    .max_height(140)
                    .on_action(move |action| Message::SelectDiagnostic(key.clone(), action))
                    .style(crate::theme::diagnostic)
                    .into()
            } else {
                // New errors remain copyable even before the next state refresh.
                text(error.clone())
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .size(12)
                    .color(crate::theme::ERROR)
                    .width(iced::Fill)
                    .into()
            };
        row![
            content,
            button("Copy error")
                .on_press(Message::CopyText(error))
                .padding(4)
                .style(crate::theme::normal)
        ]
        .spacing(6)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_can_be_selected_without_editing_and_selection_survives_ticks() {
        let message =
            "解析に失敗しました\n/Users/example/日本語 audio/audio_detector: missing file";
        let mut app = App {
            error: Some(message.into()),
            ..App::default()
        };
        app.refresh_diagnostics();
        let _ = app.update(Message::SelectDiagnostic(
            message.into(),
            text_editor::Action::SelectAll,
        ));
        assert_eq!(
            app.diagnostics[0].content.selection().as_deref(),
            Some(message)
        );
        let _ = app.update(Message::Tick);
        assert_eq!(
            app.diagnostics[0].content.selection().as_deref(),
            Some(message)
        );
        let _ = app.update(Message::SelectDiagnostic(
            message.into(),
            text_editor::Action::Edit(text_editor::Edit::Insert('x')),
        ));
        assert_eq!(app.diagnostics[0].content.text(), message);
    }

    #[test]
    fn copy_error_includes_the_entire_multiline_message() {
        let message = format!(
            "解析に失敗しました\n{}\nlast line",
            "long path /".repeat(150)
        );
        let mut app = App {
            error: Some(message.clone()),
            ..App::default()
        };
        app.refresh_diagnostics();
        let mut ui = iced_test::simulator(app.error_view(message.clone()));
        ui.click("Copy error").unwrap();
        assert!(
            ui.into_messages()
                .any(|event| matches!(event, Message::CopyText(value) if value == message))
        );
    }
}
