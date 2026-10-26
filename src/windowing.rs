//! Window routing and parent-modal interaction, independent of background work.
use crate::{App, Element, Message, menu, preferences, report};
use iced::{Task, event, keyboard, window};

impl App {
    pub(super) fn boot() -> (Self, Task<Message>) {
        let (mut app, load) = Self::load();
        let (id, open) = window::open(window::Settings {
            size: iced::Size::new(1280., 800.),
            min_size: Some(iced::Size::new(1000., 480.)),
            exit_on_close_request: false,
            ..Default::default()
        });
        app.main_window = id;
        (app, Task::batch([load, open.map(Message::WindowOpened)]))
    }

    pub(super) fn window_title(&self, id: window::Id) -> String {
        if id == self.main_window {
            "Phaselimiter"
        } else if self.report.as_ref().is_some_and(|report| report.id == id) {
            "Diagnostics"
        } else {
            "Options"
        }
        .into()
    }

    pub(super) fn window_view(&self, id: window::Id) -> Element<'_, Message> {
        if self.report.as_ref().is_some_and(|report| report.id == id) {
            return self.report_view();
        }
        if self
            .preferences
            .as_ref()
            .is_some_and(|dialog| dialog.id == id)
        {
            self.preferences_view()
        } else if id == self.main_window {
            self.view()
        } else {
            iced::widget::space().into()
        }
    }

    pub(super) fn window_opened(&self, id: window::Id) -> Task<Message> {
        if id == self.main_window {
            Task::none()
        } else if self.report.as_ref().is_some_and(|report| report.id == id)
            || self
                .preferences
                .as_ref()
                .is_some_and(|dialog| dialog.id == id)
        {
            window::gain_focus(id)
        } else {
            window::close(id)
        }
    }

    pub(super) fn window_input(
        &mut self,
        id: window::Id,
        event: iced::Event,
        status: event::Status,
    ) -> Task<Message> {
        if self.report.as_ref().is_some_and(|report| report.id == id) {
            return match event {
                iced::Event::Window(window::Event::CloseRequested | window::Event::Closed)
                | iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape),
                    ..
                }) => self.update_report(report::Event::Close),
                _ => Task::none(),
            };
        }
        if let Some(dialog) = &self.preferences {
            if id == dialog.id {
                return match event {
                    iced::Event::Window(window::Event::CloseRequested | window::Event::Closed)
                    | iced::Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::Escape),
                        ..
                    }) => self.update_preferences(preferences::Event::Cancel),
                    iced::Event::Keyboard(keyboard::Event::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::Enter),
                        ..
                    }) if status == event::Status::Ignored => {
                        self.update_preferences(preferences::Event::Accept)
                    }
                    _ => Task::none(),
                };
            }
            if id == self.main_window {
                if matches!(event, iced::Event::Window(window::Event::Closed)) {
                    let close = self.update_preferences(preferences::Event::Cancel);
                    return Task::batch([close, self.update(Message::Close)]);
                }
                return match event {
                    iced::Event::Window(
                        window::Event::Focused
                        | window::Event::CloseRequested
                        | window::Event::FileDropped(_),
                    )
                    | iced::Event::Keyboard(_) => window::gain_focus(dialog.id),
                    _ => Task::none(),
                };
            }
        }
        if id != self.main_window {
            return Task::none();
        }
        let message = match event {
            iced::Event::Window(window::Event::CloseRequested | window::Event::Closed) => {
                Message::Close
            }
            iced::Event::Window(window::Event::FileDropped(path)) => Message::FileDropped(path),
            iced::Event::Window(window::Event::FileHovered(_)) => Message::Hover(true),
            iced::Event::Window(window::Event::FilesHoveredLeft) => Message::Hover(false),
            iced::Event::Window(window::Event::Unfocused) => Message::Menu(menu::Event::Close),
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if status == event::Status::Ignored =>
            {
                Message::Menu(menu::Event::Key(key, modifiers))
            }
            iced::Event::Keyboard(keyboard::Event::KeyReleased {
                key: keyboard::Key::Named(keyboard::key::Named::Alt),
                ..
            }) if status == event::Status::Ignored => Message::Menu(menu::Event::AltReleased),
            _ => return Task::none(),
        };
        self.update(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_interactions_are_blocked_while_background_updates_continue() {
        let mut app = App {
            running: true,
            ..App::default()
        };
        let _ = app.open_preferences();
        let dialog = app.preferences.as_ref().unwrap().id;
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(1000., 550.),
            app.window_view(app.main_window),
        );
        ui.click("Add audio…").unwrap();
        let before = ui.find("Loudness").unwrap().bounds();
        let fields = ui
            .find(iced::widget::Id::new("mastering-fields"))
            .unwrap()
            .visible_bounds()
            .unwrap();
        ui.point_at(fields.center());
        ui.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
            delta: iced::mouse::ScrollDelta::Lines { x: 0., y: -10. },
        })]);
        assert_eq!(before, ui.find("Loudness").unwrap().bounds());
        for message in ui.into_messages() {
            assert!(matches!(
                message,
                Message::Preferences(preferences::Event::Focus)
            ));
            let _ = app.update(message);
        }
        let _ = app.window_input(
            app.main_window,
            iced::Event::Window(window::Event::CloseRequested),
            event::Status::Ignored,
        );
        let _ = app.window_input(
            app.main_window,
            iced::Event::Window(window::Event::FileDropped("ignored.wav".into())),
            event::Status::Ignored,
        );
        assert!(!app.closing);
        assert_eq!(app.importing, 0);
        assert!(app.running);
        let _ = app.update(Message::StorageSize(1234));
        let _ = app.update(Message::Tick);
        assert_eq!(app.storage_bytes, 1234);
        assert!(app.preferences.is_some());
        let _ = app.window_input(
            dialog,
            iced::Event::Window(window::Event::CloseRequested),
            event::Status::Ignored,
        );
        assert!(app.preferences.is_none());
        assert!(!app.closing);
    }

    #[test]
    fn escape_discards_edits_and_late_close_events_do_not_close_the_app() {
        let mut app = App::default();
        let _ = app.open_preferences();
        let id = app.preferences.as_ref().unwrap().id;
        let _ = app.update_preferences(preferences::Event::Edit(
            crate::Field::Backend,
            "discarded".into(),
        ));
        let mut ui = iced_test::simulator(app.window_view(id));
        ui.tap_key(keyboard::key::Named::Escape);
        // Runtime subscriptions receive the event even if an editor captured it.
        let _ = ui.into_messages().count();
        let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::key::Named::Escape.into(),
            modified_key: keyboard::key::Named::Escape.into(),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
            text: None,
            repeat: false,
        });
        let _ = app.window_input(id, event, event::Status::Captured);
        assert!(app.preferences.is_none());
        assert!(app.library.app_settings.backend.is_empty());
        let _ = app.window_input(
            id,
            iced::Event::Window(window::Event::Closed),
            event::Status::Ignored,
        );
        assert!(!app.closing);
    }
}
