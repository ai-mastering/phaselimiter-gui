//! Passive workspace status and queue progress.
use crate::{App, Element, Message, Status};
use iced::Fill;
use iced::widget::{container, progress_bar, row, text};

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
        let mut messages: Vec<String> = self.diagnostic_messages();
        if let Some(error) = &self.error
            && !messages.contains(error)
        {
            messages.insert(0, error.clone());
        }
        if let Some((_, reason)) = self.settings_issue() {
            messages.push(format!("Cannot run mastering: {reason}"));
        } else if self.selected.is_none() {
            messages.push("Select an input first.".into());
        } else if self.root.is_none() {
            messages.push("Data directory unavailable.".into());
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
        if self.equalize && self.listening_analysis().is_none() {
            messages.push("Loudness matching awaits analysis.".into());
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
        messages
    }

    pub(super) fn status_bar(&self) -> Element<'_, Message> {
        let messages = self.status_messages();
        let summary = messages
            .first()
            .map_or("Ready", |s| s.lines().next().unwrap_or("Error"));
        let label = text(summary.to_owned())
            .size(12)
            .wrapping(iced::widget::text::Wrapping::None);
        container(
            row![
                container(label).width(Fill).clip(true).padding(4),
                self.queue_view(),
            ]
            .spacing(8)
            .align_y(iced::Center),
        )
        .height(36)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_passive_and_long_errors_leave_room_for_queue_state() {
        let app = App {
            error: Some("long error /".repeat(200)),
            ..App::default()
        };
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(640., 36.),
            app.status_bar(),
        );
        let queue = ui.find("0 queued").unwrap().visible_bounds().unwrap();
        assert!(queue.x >= 0. && queue.x + queue.width <= 640.);
        ui.point_at(iced::Point::new(100., 18.));
        ui.simulate(iced_test::simulator::click());
        ui.click("0 queued").unwrap();
        assert_eq!(ui.into_messages().count(), 0);
    }
}
