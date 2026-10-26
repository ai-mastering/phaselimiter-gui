//! Transactional application preferences and their dialog lifecycle.
use crate::{App, Element, Field, Message, descriptions::Topic, model::AppSettings, theme};
use iced::widget::{
    button, column, container, pick_list, row, rule, scrollable, space, text, text_input,
};
use iced::{Fill, Task, window};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Event {
    Theme(String),
    Edit(Field, String),
    Browse(Field),
    Picked(window::Id, Field, Option<PathBuf>),
    UseBundledTools,
    Help(Field),
    Accept,
    Cancel,
    Focus,
}

pub struct Dialog {
    pub id: window::Id,
    pub draft: AppSettings,
    picker_pending: bool,
    help: Option<Field>,
    pub error: Option<String>,
}

pub fn is_field(field: Field) -> bool {
    matches!(
        field,
        Field::Backend | Field::Ffmpeg | Field::Cache | Field::ExportDir
    )
}

impl Dialog {
    fn field_mut(&mut self, field: Field) -> Option<&mut String> {
        match field {
            Field::Backend => Some(&mut self.draft.backend),
            Field::Ffmpeg => Some(&mut self.draft.ffmpeg),
            Field::Cache => Some(&mut self.draft.cache),
            Field::ExportDir => Some(&mut self.draft.export_dir),
            _ => None,
        }
    }
}

impl App {
    pub(super) fn current_theme(&self) -> theme::Theme {
        let settings = self
            .preferences
            .as_ref()
            .map_or(&self.library.app_settings, |dialog| &dialog.draft);
        theme::Theme::from_name(&settings.theme)
    }

    pub(super) fn open_preferences(&mut self) -> Task<Message> {
        if let Some(dialog) = &self.preferences {
            return window::gain_focus(dialog.id);
        }
        let (id, open) = window::open(window::Settings {
            size: iced::Size::new(600., 570.),
            min_size: Some(iced::Size::new(440., 320.)),
            position: window::Position::Centered,
            minimizable: false,
            exit_on_close_request: false,
            ..Default::default()
        });
        self.preferences = Some(Dialog {
            id,
            draft: self.library.app_settings.clone(),
            picker_pending: false,
            help: None,
            error: None,
        });
        self.menu = crate::menu::State::default();
        self.hovering = false;
        Task::batch([open.map(Message::WindowOpened), self.refresh_storage()])
    }

    pub(super) fn update_preferences(&mut self, event: Event) -> Task<Message> {
        let Some(dialog) = self.preferences.as_mut() else {
            return Task::none();
        };
        match event {
            Event::Theme(name) => dialog.draft.theme = name,
            Event::Edit(field, value) => {
                if let Some(target) = dialog.field_mut(field) {
                    *target = value;
                }
            }
            Event::UseBundledTools => {
                dialog.draft.backend.clear();
                dialog.draft.ffmpeg.clear();
                dialog.draft.cache.clear();
            }
            Event::Help(field) => dialog.help = (dialog.help != Some(field)).then_some(field),
            Event::Focus => return window::gain_focus(dialog.id),
            Event::Browse(field) => {
                if dialog.picker_pending {
                    return Task::none();
                }
                let title = match field {
                    Field::Backend => "Choose phase_limiter executable",
                    Field::Ffmpeg => "Choose FFmpeg executable",
                    Field::Cache => "Choose sound_quality2_cache",
                    Field::ExportDir => "Choose default export directory",
                    _ => return Task::none(),
                };
                let current = dialog
                    .field_mut(field)
                    .map(|value| PathBuf::from(value.as_str()))
                    .unwrap_or_default();
                dialog.picker_pending = true;
                let id = dialog.id;
                return window::run(id, move |handle| {
                    let mut picker = rfd::AsyncFileDialog::new()
                        .set_title(title)
                        .set_parent(&handle);
                    if current.is_dir() {
                        picker = picker.set_directory(&current);
                    } else if let Some(parent) = current.parent().filter(|path| path.is_dir()) {
                        picker = picker.set_directory(parent);
                    }
                    picker
                })
                .then(move |picker| {
                    Task::perform(
                        async move {
                            let picked = if field == Field::ExportDir {
                                picker.pick_folder().await
                            } else {
                                picker.pick_file().await
                            };
                            picked.map(|file| file.path().to_path_buf())
                        },
                        move |path| Message::Preferences(Event::Picked(id, field, path)),
                    )
                });
            }
            Event::Picked(id, field, path) => {
                if dialog.id != id {
                    return Task::none();
                }
                dialog.picker_pending = false;
                if let Some(path) = path
                    && let Some(target) = dialog.field_mut(field)
                {
                    *target = path.to_string_lossy().into_owned();
                }
                return window::gain_focus(id);
            }
            Event::Accept | Event::Cancel => {
                if matches!(event, Event::Accept) && dialog.picker_pending {
                    return Task::none();
                }
                let Some(mut dialog) = self.preferences.take() else {
                    return Task::none();
                };
                if matches!(event, Event::Accept) {
                    let previous =
                        std::mem::replace(&mut self.library.app_settings, dialog.draft.clone());
                    if self.root.is_none() || !self.save_sync() {
                        self.library.app_settings = previous;
                        dialog.error = Some(self.error.clone().unwrap_or_else(|| {
                            "Data directory unavailable; preferences could not be saved.".into()
                        }));
                        self.preferences = Some(dialog);
                        return Task::none();
                    }
                }
                return window::close(dialog.id).chain(window::gain_focus(self.main_window));
            }
        }
        Task::none()
    }

    pub(super) fn preferences_view(&self) -> Element<'_, Message> {
        let Some(dialog) = &self.preferences else {
            return space().into();
        };
        let mut content = column![
            row![
                text("Theme"),
                pick_list(
                    theme::Theme::choices(),
                    Some(self.current_theme().name().to_owned()),
                    |name| Message::Preferences(Event::Theme(name))
                )
                .width(Fill),
            ]
            .spacing(theme::SECTION_GAP)
            .align_y(iced::Center),
            rule::horizontal(1),
            text("Engine & resources").size(theme::HEADING_TEXT),
            text("Blank paths use the bundled tools. Otherwise, enter an absolute path.")
                .size(theme::SMALL_TEXT),
            button("Clear paths / use bundled tools")
                .on_press(Message::Preferences(Event::UseBundledTools))
                .style(theme::normal),
            self.preference_path(
                "phase_limiter executable",
                Field::Backend,
                &dialog.draft.backend
            ),
            self.preference_path(
                "FFmpeg executable (absolute path)",
                Field::Ffmpeg,
                &dialog.draft.ffmpeg
            ),
            self.preference_path("sound_quality2_cache", Field::Cache, &dialog.draft.cache),
            rule::horizontal(1),
            self.preference_path(
                "Default export directory",
                Field::ExportDir,
                &dialog.draft.export_dir
            ),
            rule::horizontal(1),
            text(format!(
                "Library: {:.1} MiB",
                self.storage_bytes as f64 / 1048576.
            )),
            text(
                self.library_folder()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            )
            .size(theme::SMALL_TEXT)
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            button("Open library folder")
                .on_press(Message::OpenLibrary)
                .style(theme::normal),
            text("Queued and running mastering jobs keep their existing settings.")
                .size(theme::SMALL_TEXT),
        ]
        .spacing(theme::SECTION_GAP)
        .width(Fill);
        if let Some(error) = &dialog.error {
            content = content.push(
                text(error)
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .style(theme::error),
            );
        }
        container(
            column![
                scrollable(content)
                    .id("preferences-fields")
                    .height(Fill)
                    .spacing(theme::SECTION_GAP),
                rule::horizontal(1),
                row![
                    space().width(Fill),
                    button("OK").width(80).style(theme::normal).on_press_maybe(
                        (!dialog.picker_pending).then_some(Message::Preferences(Event::Accept))
                    ),
                    button("Cancel")
                        .width(80)
                        .style(theme::normal)
                        .on_press(Message::Preferences(Event::Cancel)),
                ]
                .spacing(theme::SECTION_GAP),
            ]
            .spacing(theme::SECTION_GAP),
        )
        .padding(12)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn preference_path<'a>(
        &'a self,
        label: &'static str,
        field: Field,
        value: &'a str,
    ) -> Element<'a, Message> {
        let mut control = column![
            row![
                text(label),
                button(text("i").size(theme::SMALL_TEXT))
                    .padding([0, 5])
                    .style(theme::normal)
                    .on_press(Message::Preferences(Event::Help(field)))
            ]
            .spacing(theme::ITEM_GAP)
            .align_y(iced::Center),
            row![
                text_input(label, value)
                    .id(format!("preference-{field:?}"))
                    .on_input(move |value| Message::Preferences(Event::Edit(field, value)))
                    .on_submit(Message::Preferences(Event::Accept)),
                button("Browse…").style(theme::normal).on_press_maybe(
                    self.preferences
                        .as_ref()
                        .filter(|dialog| !dialog.picker_pending)
                        .map(|_| Message::Preferences(Event::Browse(field)))
                ),
            ]
            .spacing(theme::SECTION_GAP)
            .align_y(iced::Center),
        ]
        .spacing(theme::ITEM_GAP);
        if self
            .preferences
            .as_ref()
            .is_some_and(|dialog| dialog.help == Some(field))
        {
            control =
                control.push(text(self.description(Topic::Field(field))).size(theme::SMALL_TEXT));
        }
        control.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(dir: &std::path::Path) -> App {
        let mut app = App {
            root: Some(dir.to_path_buf()),
            ..App::default()
        };
        app.library.app_settings.backend = "saved engine".into();
        app.save();
        app
    }

    #[test]
    #[ignore = "Render theme previews for manual review"]
    fn theme_selector_visual_review() {
        let directory = std::env::var("THEME_REVIEW_DIR").expect("set THEME_REVIEW_DIR");
        let mut app = App::default();
        let _ = app.open_preferences();
        for name in ["Clear controls", "Light", "Dark"] {
            let _ = app.update_preferences(Event::Theme(name.into()));
            let mut ui = iced_test::Simulator::with_size(
                Default::default(),
                iced::Size::new(600., 570.),
                app.preferences_view(),
            );
            assert!(
                ui.snapshot(&app.current_theme())
                    .unwrap()
                    .matches_image(format!("{directory}/options-{name}.png"))
                    .unwrap()
            );
        }
    }

    #[test]
    fn theme_preview_cancels_and_accept_persists_for_the_next_launch() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        let saved = app.current_theme();
        let _ = app.open_preferences();
        let _ = app.update_preferences(Event::Theme("Dark".into()));
        assert_eq!(app.current_theme(), theme::Theme::from(iced::Theme::Dark));
        app.save();
        assert!(
            crate::Library::load(dir.path())
                .unwrap()
                .app_settings
                .theme
                .is_empty()
        );
        let _ = app.update_preferences(Event::Cancel);
        assert_eq!(app.current_theme(), saved);
        let _ = app.open_preferences();
        let _ = app.update_preferences(Event::Theme("Dracula".into()));
        let _ = app.update_preferences(Event::Accept);
        assert!(app.preferences.is_none());
        let reloaded = App {
            library: crate::Library::load(dir.path()).unwrap(),
            ..App::default()
        };
        assert_eq!(
            reloaded.current_theme(),
            theme::Theme::from(iced::Theme::Dracula)
        );
    }

    #[test]
    fn failed_theme_save_keeps_preview_local_and_cancel_restores_saved_theme() {
        let mut app = App::default();
        let _ = app.open_preferences();
        let _ = app.update_preferences(Event::Theme("Dark".into()));
        let _ = app.update_preferences(Event::Accept);
        assert!(app.preferences.as_ref().unwrap().error.is_some());
        assert!(app.library.app_settings.theme.is_empty());
        let _ = app.update_preferences(Event::Cancel);
        assert_eq!(app.current_theme(), theme::Theme::default());
    }

    #[test]
    fn edits_stay_local_during_background_saves_and_cancel_reopens_saved_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        let _ = app.open_preferences();
        let _ = app.update_preferences(Event::Edit(Field::Backend, "draft engine".into()));
        let _ = app.update_preferences(Event::UseBundledTools);
        app.save();
        assert_eq!(
            crate::Library::load(dir.path())
                .unwrap()
                .app_settings
                .backend,
            "saved engine"
        );
        let _ = app.update_preferences(Event::Cancel);
        assert_eq!(app.library.app_settings.backend, "saved engine");
        let _ = app.open_preferences();
        assert_eq!(
            app.preferences.as_ref().unwrap().draft.backend,
            "saved engine"
        );
        let _ = app.update_preferences(Event::Edit(Field::Backend, "new engine".into()));
        let _ = app.update_preferences(Event::Accept);
        assert!(app.preferences.is_none());
        assert_eq!(
            crate::Library::load(dir.path())
                .unwrap()
                .app_settings
                .backend,
            "new engine"
        );
    }

    #[test]
    fn late_picker_results_cannot_change_a_new_dialog_or_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        let _ = app.open_preferences();
        let old = app.preferences.as_ref().unwrap().id;
        let _ = app.update_preferences(Event::Cancel);
        let _ = app.update_preferences(Event::Picked(old, Field::Backend, Some("late".into())));
        assert_eq!(app.library.app_settings.backend, "saved engine");
        let _ = app.open_preferences();
        let current = app.preferences.as_ref().unwrap().id;
        assert_ne!(old, current);
        let _ = app.update_preferences(Event::Picked(old, Field::Backend, Some("late".into())));
        assert_eq!(
            app.preferences.as_ref().unwrap().draft.backend,
            "saved engine"
        );
        let _ = app.open_preferences();
        assert_eq!(app.preferences.as_ref().unwrap().id, current);
    }

    #[test]
    fn dialog_buttons_remain_clickable_at_minimum_height() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app(dir.path());
        let _ = app.open_preferences();
        let id = app.preferences.as_ref().unwrap().id;
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(440., 320.),
            app.window_view(id),
        );
        let fields = ui
            .find(iced::widget::Id::new("preferences-fields"))
            .unwrap()
            .visible_bounds()
            .unwrap();
        assert!(fields.height >= 240.);
        ui.click(iced::widget::Id::new("preference-Backend"))
            .unwrap();
        ui.typewrite("x");
        let cancel = ui.find("Cancel").unwrap().visible_bounds().unwrap();
        assert!(cancel.y + cancel.height < 320.);
        ui.click("Cancel").unwrap();
        let messages: Vec<_> = ui.into_messages().collect();
        assert!(messages.iter().any(|message| matches!(
            message,
            Message::Preferences(Event::Edit(Field::Backend, _))
        )));
        for message in messages {
            let _ = app.update(message);
        }
        assert!(app.preferences.is_none());
        assert_eq!(app.library.app_settings.backend, "saved engine");
    }
    #[test]
    fn failed_save_keeps_the_dialog_open_and_restores_applied_preferences() {
        let mut app = App::default();
        let _ = app.open_preferences();
        let _ = app.update_preferences(Event::Edit(Field::Backend, "unsaved engine".into()));
        let _ = app.update_preferences(Event::Accept);
        assert!(app.library.app_settings.backend.is_empty());
        let dialog = app.preferences.as_ref().unwrap();
        assert_eq!(dialog.draft.backend, "unsaved engine");
        assert!(dialog.error.is_some());
    }
}
