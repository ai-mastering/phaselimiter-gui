//! Passive workspace status and queue progress.
use crate::{App, Element, Message, Status};
use iced::Fill;
use iced::widget::{column, container, progress_bar, row, rule, text};

const STATUS_HEIGHT: f32 = 24.;

impl App {
    fn queue_view(&self) -> Element<'_, Message> {
        let waiting = self
            .library
            .jobs
            .iter()
            .filter(|j| j.status == Status::Waiting)
            .count();
        let mut footer = row![].spacing(8).align_y(iced::Center);
        if let Some(active) = &self.active
            && let Some(job) = self.library.jobs.iter().find(|j| j.id == active.id)
        {
            footer = footer
                .push(text("Processing").size(12))
                .push(container(progress_bar(0.0..=1.0, job.progress).girth(5)).width(100));
        }
        footer = footer.push(text(format!("{waiting} queued")).size(12));
        footer.into()
    }

    pub(super) fn status_messages(&self) -> Vec<String> {
        self.status_content().0
    }

    fn status_content(&self) -> (Vec<String>, bool) {
        let mut messages: Vec<String> = self.diagnostic_messages();
        if let Some(error) = &self.error
            && !messages.contains(error)
        {
            messages.insert(0, error.clone());
        }
        let mut is_error = !messages.is_empty();
        if let Some((_, reason)) = self.settings_issue() {
            messages.push(format!("Cannot run mastering: {reason}"));
            is_error = true;
        } else if self.selected.is_none() {
            messages.push("Select an input first.".into());
        } else if self.root.is_none() {
            messages.push("Data directory unavailable.".into());
            is_error = true;
        }
        if self.hovering {
            messages.push("Release to import audio.".into());
        }
        if self.importing > 0 {
            messages.push(format!("Importing {}…", self.importing));
        }
        if self.loading {
            messages.push("Preparing selection · current audio continues playing".into());
        } else if self.loaded && self.listening_key() != self.active_key {
            let playing = self
                .active_key
                .and_then(|key| {
                    self.library
                        .tracks
                        .iter()
                        .find(|t| t.id == key.0)
                        .map(|track| {
                            let label = key
                                .1
                                .and_then(|id| self.library.jobs.iter().find(|j| j.id == id))
                                .map_or_else(
                                    || "Original".into(),
                                    |job| self.library.job_label(job),
                                );
                            format!("{} / {label}", track.name)
                        })
                })
                .unwrap_or_else(|| "previous selection".into());
            messages.push(format!("Selected audio unavailable · playing {playing}"));
        }
        if self.equalize
            && self.job().is_some_and(|j| j.status == Status::Succeeded)
            && self.matching_loudness().is_none()
        {
            messages.push("Loudness matching awaits analysis for both A and B.".into());
        }
        if self.analyzing.is_some() {
            messages.push("Analyzing…".into());
        }
        if self.draft_reference.active || self.draft_reference.due.is_some() {
            messages.push("Analyzing reference…".into());
        }
        if self.reference_pending.is_some() {
            messages.push("Loading reference analysis…".into());
        }
        if let Some(notice) = &self.notice {
            messages.push(notice.clone());
        }
        (messages, is_error)
    }

    pub(super) fn status_bar(&self) -> Element<'_, Message> {
        let (messages, is_error) = self.status_content();
        let summary = messages
            .first()
            .map_or("Ready", |s| s.lines().next().unwrap_or("Error"));
        let label = text(summary.to_owned())
            .size(12)
            .color(if is_error {
                crate::theme::ERROR
            } else {
                crate::theme::LABEL
            })
            .wrapping(iced::widget::text::Wrapping::None);
        let content = container(
            row![
                container(label).width(Fill).clip(true),
                rule::vertical(1),
                self.queue_view(),
            ]
            .spacing(8)
            .align_y(iced::Center),
        )
        .padding([0, 10])
        .height(STATUS_HEIGHT - 1.)
        .center_y(STATUS_HEIGHT - 1.);
        container(column![rule::horizontal(1), content])
            .width(Fill)
            .height(STATUS_HEIGHT)
            .style(crate::theme::status_bar)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_distinguished_from_normal_guidance_and_recover() {
        let mut app = App::default();
        assert!(!app.status_content().1);
        app.error = Some("Import failed".into());
        let (messages, is_error) = app.status_content();
        assert_eq!(messages[0], "Import failed");
        assert!(is_error);
        app.error = None;
        assert!(!app.status_content().1);
        app.library.draft.low_cut = "-1".into();
        assert!(app.status_content().1);
    }

    #[test]
    fn status_is_passive_and_long_errors_leave_room_for_queue_state() {
        let app = App {
            error: Some("long error /".repeat(200)),
            ..App::default()
        };
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(640., STATUS_HEIGHT),
            app.status_bar(),
        );
        let queue = ui.find("0 queued").unwrap().visible_bounds().unwrap();
        assert!(queue.x >= 0. && queue.x + queue.width <= 640.);
        assert!(queue.y >= 1. && queue.y + queue.height <= STATUS_HEIGHT);
        ui.point_at(iced::Point::new(100., STATUS_HEIGHT / 2.));
        ui.simulate(iced_test::simulator::click());
        ui.click("0 queued").unwrap();
        assert_eq!(ui.into_messages().count(), 0);
    }
}
