//! Window-local application menu: definitions, navigation, and overlay live here.
use crate::{App, Element, Message, Theme};
use iced::widget::{column, container, mouse_area, opaque, row, rule, space, stack, text};
use iced::{Color, Fill, Task, keyboard};

const BAR_HEIGHT: f32 = 22.;
const ITEM_HEIGHT: f32 = 22.;
const FONT_SIZE: f32 = 13.;
const ROOTS: [(&str, char, f32, f32); 4] = [
    ("File", 'f', 36., 238.),
    ("Edit", 'e', 38., 190.),
    ("Tools", 't', 48., 238.),
    ("Playback", 'p', 72., 178.),
];

#[derive(Debug, Clone)]
pub enum Event {
    Toggle(usize),
    Hover(usize),
    LeaveRoot(usize),
    Select(Option<usize>),
    LeaveItem(usize),
    Context(crate::ResultKey),
    Pointer(iced::Point),
    Close,
    Activate(usize),
    Key(keyboard::Key, keyboard::Modifiers),
    AltReleased,
}

#[derive(Default)]
pub struct State {
    focused: Option<usize>,
    open: Option<usize>,
    selected: Option<usize>,
    hovered: Option<usize>,
    alt_armed: bool,
    pointer: iced::Point,
    context_position: Option<iced::Point>,
}

struct Entry {
    label: &'static str,
    mnemonic: char,
    shortcut: &'static str,
    separator: bool,
    checked: bool,
    command: Option<Message>,
}

impl Entry {
    fn new(label: &'static str, mnemonic: char, command: Option<Message>) -> Self {
        Self {
            label,
            mnemonic,
            command,
            shortcut: "",
            separator: false,
            checked: false,
        }
    }
}

impl App {
    fn menu_entries(&self, index: usize) -> Vec<Entry> {
        let mut entries = match index {
            0 => vec![
                Entry::new("Master audio…", 'a', Some(Message::BrowseAudio)),
                Entry::new(
                    if self.show_audio_path {
                        "Hide path input"
                    } else {
                        "Master audio from path…"
                    },
                    'p',
                    Some(Message::ToggleAudioPath),
                ),
                Entry::new(
                    "Export selected result…",
                    'e',
                    self.job()
                        .filter(|j| j.status == crate::Status::Succeeded)
                        .map(|_| Message::Export),
                ),
                Entry::new("Exit", 'x', Some(Message::Close)),
            ],
            1 => vec![Entry::new(
                "Remove selected",
                'r',
                self.selected.map(Message::Remove),
            )],
            2 => vec![
                Entry::new(
                    "Resume queue",
                    'r',
                    (!self.running
                        && self
                            .library
                            .jobs
                            .iter()
                            .any(|job| job.status == crate::Status::Waiting))
                    .then_some(Message::Start),
                ),
                Entry::new(
                    "Cancel current & pause",
                    'c',
                    self.active.as_ref().map(|_| Message::Cancel),
                ),
                Entry::new(
                    "Diagnostics",
                    'd',
                    Some(Message::Report(crate::report::Event::Open)),
                ),
                Entry::new("Options", 'o', Some(Message::OpenAppSettings)),
            ],
            _ => vec![
                Entry::new("Play / Pause", 'p', self.loaded.then_some(Message::Play)),
                Entry::new("Clear loop", 'c', self.region.map(|_| Message::ClearRegion)),
            ],
        };
        if index == 1 && self.menu.context_position.is_some() {
            entries.truncate(1);
            return entries;
        }
        match index {
            0 => {
                entries[0].shortcut = "Ctrl+O";
                entries[2].separator = true;
                entries[3].separator = true;
            }
            2 => {
                entries[2].separator = true;
                entries[3].separator = true;
            }
            _ => {}
        }
        entries
    }

    fn open_menu(&mut self, index: usize, from_end: bool) {
        self.menu.context_position = None;
        self.menu.focused = Some(index);
        self.menu.open = Some(index);
        let entries = self.menu_entries(index);
        self.menu.selected = if from_end {
            entries.iter().rposition(|entry| entry.command.is_some())
        } else {
            entries.iter().position(|entry| entry.command.is_some())
        };
    }

    pub(super) fn update_menu(&mut self, event: Event) -> Task<Message> {
        let mut command = None;
        match event {
            Event::Toggle(index) => {
                if self.menu.open == Some(index) {
                    self.menu = State::default();
                } else {
                    self.menu.context_position = None;
                    self.menu.focused = Some(index);
                    self.menu.open = Some(index);
                    self.menu.selected = None;
                }
            }
            Event::Hover(index) => {
                self.menu.hovered = Some(index);
                if self.menu.open.is_some() && self.menu.open != Some(index) {
                    self.menu.context_position = None;
                    self.menu.focused = Some(index);
                    self.menu.open = Some(index);
                    self.menu.selected = None;
                }
            }
            Event::LeaveRoot(index) => {
                if self.menu.hovered == Some(index) {
                    self.menu.hovered = None;
                }
            }
            Event::Select(index) => self.menu.selected = index,
            Event::LeaveItem(index) => {
                if self.menu.selected == Some(index) {
                    self.menu.selected = None;
                }
            }
            Event::Pointer(position) => self.menu.pointer = position,
            Event::Context(key) => {
                let task = self.update(Message::Select(key));
                self.menu.context_position = Some(self.menu.pointer);
                self.menu.focused = Some(1);
                self.menu.open = Some(1);
                self.menu.selected = None;
                return task;
            }
            Event::Close => self.menu = State::default(),
            Event::Activate(index) => {
                if let Some(open) = self.menu.open {
                    command = self
                        .menu_entries(open)
                        .get(index)
                        .and_then(|entry| entry.command.clone());
                }
                if command.is_some() {
                    self.menu = State::default();
                }
            }
            Event::AltReleased => {
                if self.menu.alt_armed {
                    self.menu.alt_armed = false;
                    if self.menu.focused.is_some() {
                        self.menu = State::default();
                    } else {
                        self.menu.focused = Some(0);
                    }
                }
            }
            Event::Key(key, modifiers) => {
                use keyboard::key::Named;
                if key == keyboard::Key::Named(Named::Alt) {
                    self.menu.alt_armed = true;
                    return Task::none();
                }
                self.menu.alt_armed = false;
                if key == keyboard::Key::Named(Named::F10) {
                    if self.menu.focused.is_some() {
                        self.menu = State::default();
                    } else {
                        self.menu.focused = Some(0);
                    }
                } else if modifiers.alt()
                    && let keyboard::Key::Character(character) = &key
                    && let Some(index) = ROOTS
                        .iter()
                        .position(|root| character.eq_ignore_ascii_case(&root.1.to_string()))
                {
                    self.open_menu(index, false);
                } else if (modifiers.command() || modifiers.control())
                    && key.as_ref() == keyboard::Key::Character("o")
                {
                    self.menu = State::default();
                    command = Some(Message::BrowseAudio);
                } else if let Some(focused) = self.menu.focused {
                    match key {
                        keyboard::Key::Named(Named::Escape) => {
                            if self.menu.context_position.is_some() {
                                self.menu = State::default();
                            } else if self.menu.open.take().is_some() {
                                self.menu.selected = None;
                            } else {
                                self.menu = State::default();
                            }
                        }
                        keyboard::Key::Named(Named::ArrowLeft | Named::ArrowRight) => {
                            if self.menu.context_position.is_some() {
                                return Task::none();
                            }
                            let step = if key == keyboard::Key::Named(Named::ArrowRight) {
                                1
                            } else {
                                ROOTS.len() - 1
                            };
                            let next = (focused + step) % ROOTS.len();
                            if self.menu.open.is_some() {
                                self.open_menu(next, false);
                            } else {
                                self.menu.focused = Some(next);
                            }
                        }
                        keyboard::Key::Named(Named::ArrowDown | Named::ArrowUp) => {
                            let backwards = key == keyboard::Key::Named(Named::ArrowUp);
                            if let Some(open) = self.menu.open {
                                let entries = self.menu_entries(open);
                                let count = entries.len();
                                let step = if backwards { count - 1 } else { 1 };
                                let mut selected = self.menu.selected.unwrap_or(if backwards {
                                    0
                                } else {
                                    count - 1
                                });
                                for _ in 0..count {
                                    selected = (selected + step) % count;
                                    if entries[selected].command.is_some() {
                                        self.menu.selected = Some(selected);
                                        break;
                                    }
                                }
                            } else {
                                self.open_menu(focused, backwards);
                            }
                        }
                        keyboard::Key::Named(Named::Enter | Named::Space) => {
                            if let Some(selected) = self.menu.selected {
                                return self.update_menu(Event::Activate(selected));
                            }
                            if let Some(open) = self.menu.open {
                                self.menu.selected = self
                                    .menu_entries(open)
                                    .iter()
                                    .position(|entry| entry.command.is_some());
                            } else {
                                self.open_menu(focused, false);
                            }
                        }
                        keyboard::Key::Named(Named::Tab) => self.menu = State::default(),
                        keyboard::Key::Character(character) => {
                            if let Some(open) = self.menu.open {
                                if let Some(index) =
                                    self.menu_entries(open).iter().position(|entry| {
                                        character.eq_ignore_ascii_case(&entry.mnemonic.to_string())
                                            && entry.command.is_some()
                                    })
                                {
                                    return self.update_menu(Event::Activate(index));
                                }
                            } else if let Some(index) = ROOTS.iter().position(|root| {
                                character.eq_ignore_ascii_case(&root.1.to_string())
                            }) {
                                self.open_menu(index, false);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        command.map_or_else(Task::none, |message| self.update(message))
    }

    pub(super) fn menu_bar(&self) -> Element<'_, Message> {
        let mut bar = row![];
        for (index, (label, _, width, _)) in ROOTS.into_iter().enumerate() {
            let pressed = self.menu.context_position.is_none() && self.menu.open == Some(index);
            let highlighted = pressed
                || (self.menu.context_position.is_none() && self.menu.focused == Some(index))
                || self.menu.hovered == Some(index);
            bar = bar.push(
                mouse_area(
                    container(text(label).size(FONT_SIZE))
                        .width(width)
                        .height(BAR_HEIGHT)
                        .center_y(BAR_HEIGHT)
                        .padding([0, 8])
                        .style(move |theme: &Theme| {
                            let p = theme.extended_palette();
                            let pair = if pressed {
                                p.primary.weak
                            } else {
                                p.background.weak
                            };
                            container::Style {
                                background: highlighted.then_some(pair.color.into()),
                                text_color: highlighted.then_some(pair.text),
                                border: iced::Border {
                                    color: if highlighted {
                                        p.secondary.base.color
                                    } else {
                                        Color::TRANSPARENT
                                    },
                                    width: 1.,
                                    radius: 0.into(),
                                },
                                ..Default::default()
                            }
                        }),
                )
                .on_press(Message::Menu(Event::Toggle(index)))
                .on_enter(Message::Menu(Event::Hover(index)))
                .on_exit(Message::Menu(Event::LeaveRoot(index))),
            );
        }
        bar.height(BAR_HEIGHT).into()
    }

    pub(super) fn menu_layer<'a>(
        &'a self,
        content: Element<'a, Message>,
        size: iced::Size,
    ) -> Element<'a, Message> {
        let Some(open) = self.menu.open else {
            return MenuInput {
                content: stack![content].into(),
                active: self.menu.focused.is_some(),
                popup_open: false,
            }
            .into();
        };
        let entries = self.menu_entries(open);
        let popup_height = entries.len() as f32 * ITEM_HEIGHT
            + entries.iter().filter(|entry| entry.separator).count() as f32 * 7.
            + 6.;
        let mut items = column![].padding(3).width(ROOTS[open].3);
        for (index, entry) in entries.into_iter().enumerate() {
            if entry.separator {
                items = items.push(container(rule::horizontal(1)).padding([3, 2]));
            }
            let enabled = entry.command.is_some();
            let highlighted = enabled && self.menu.selected == Some(index);
            let line = row![
                text(if entry.checked { "✓" } else { "" })
                    .size(FONT_SIZE)
                    .width(20),
                text(entry.label).size(FONT_SIZE),
                space().width(Fill),
                text(entry.shortcut).size(FONT_SIZE),
            ]
            .align_y(iced::Center);
            let item = mouse_area(
                container(line)
                    .width(Fill)
                    .height(ITEM_HEIGHT)
                    .center_y(ITEM_HEIGHT)
                    .padding([0, 8])
                    .style(move |theme: &Theme| {
                        let p = theme.extended_palette();
                        container::Style {
                            text_color: Some(if !enabled {
                                p.background.base.text.scale_alpha(0.6)
                            } else if highlighted {
                                p.primary.base.text
                            } else {
                                p.background.base.text
                            }),
                            background: highlighted.then_some(p.primary.base.color.into()),
                            ..Default::default()
                        }
                    }),
            )
            .on_press(Message::Menu(Event::Select(enabled.then_some(index))))
            .on_enter(Message::Menu(Event::Select(enabled.then_some(index))))
            .on_exit(Message::Menu(Event::LeaveItem(index)));
            items = items.push(if enabled {
                item.on_release(Message::Menu(Event::Activate(index)))
            } else {
                item
            });
        }
        let offset: f32 = ROOTS[..open].iter().map(|root| root.2).sum();
        let popup = opaque(container(items).style(crate::theme::tooltip));
        let overlay: Element<'_, Message> = if let Some(position) = self.menu.context_position {
            let x = position.x.min(size.width - ROOTS[open].3 - 4.).max(0.);
            let y = position.y.min(size.height - popup_height - 4.).max(0.);
            column![space().height(y), row![space().width(x), popup]].into()
        } else {
            container(column![self.menu_bar(), row![space().width(offset), popup]])
                .padding(10)
                .into()
        };
        MenuInput {
            content: stack![
                content,
                mouse_area(space().width(Fill).height(Fill))
                    .on_press(Message::Menu(Event::Close))
                    .on_right_press(Message::Menu(Event::Close)),
                overlay,
            ]
            .into(),
            active: true,
            popup_open: true,
        }
        .into()
    }
}
// Intercept menu navigation before focused editors receive it. Keep the existing
// widget tree alive so closing a menu preserves the editor's focus and cursor.
struct MenuInput<'a> {
    content: Element<'a, Message>,
    active: bool,
    popup_open: bool,
}

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Length, Rectangle, Size, Vector};
impl Widget<Message, Theme, iced::Renderer> for MenuInput<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
        ) && let Some(position) = cursor.position()
        {
            shell.publish(Message::Menu(Event::Pointer(position)));
        }
        if self.active
            && !self.popup_open
            && matches!(event, iced::Event::Mouse(mouse::Event::ButtonPressed(_)))
            && cursor
                .position()
                .is_some_and(|position| position.y >= layout.bounds().y + 10. + BAR_HEIGHT)
        {
            shell.publish(Message::Menu(Event::Close));
        }
        if self.active
            && matches!(
                event,
                iced::Event::Keyboard(keyboard::Event::KeyReleased {
                    key: keyboard::Key::Named(keyboard::key::Named::Alt),
                    ..
                })
            )
        {
            shell.publish(Message::Menu(Event::AltReleased));
            shell.capture_event();
        } else if self.active
            && let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
        {
            shell.publish(Message::Menu(Event::Key(key.clone(), *modifiers)));
            shell.capture_event();
        } else {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
impl<'a> From<MenuInput<'a>> for Element<'a, Message> {
    fn from(menu: MenuInput<'a>) -> Self {
        Self::new(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_click_navigation_dismissal_and_command_dispatch() {
        let mut app = App::default();
        let mut ui = iced_test::simulator(app.view());
        ui.click("File").unwrap();
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert_eq!(app.menu.open, Some(0));
        let mut ui = iced_test::simulator(app.view());
        if let Ok(directory) = std::env::var("LAYOUT_REVIEW_DIR") {
            ui.snapshot(&Theme::default())
                .unwrap()
                .matches_image(format!("{directory}/menu.png"))
                .unwrap();
        }
        ui.click("Master audio from path…").unwrap();
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert!(app.show_audio_path);
        assert!(app.menu.open.is_none());
        let _ = app.update_menu(Event::Key(
            keyboard::key::Named::F10.into(),
            keyboard::Modifiers::default(),
        ));
        let mut ui = iced_test::simulator(app.view());
        ui.tap_key(keyboard::key::Named::ArrowRight);
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert_eq!(app.menu.focused, Some(1));
        assert!(app.menu.open.is_none());
        let _ = app.update_menu(Event::Key(
            keyboard::key::Named::ArrowDown.into(),
            keyboard::Modifiers::default(),
        ));
        assert_eq!(app.menu.open, Some(1));
        let _ = app.update_menu(Event::Activate(0));
        assert_eq!(
            app.menu.open,
            Some(1),
            "Disabled entries must not activate or close the menu"
        );
        let mut ui = iced_test::simulator(app.view());
        ui.tap_key(keyboard::key::Named::ArrowRight);
        ui.typewrite("o");
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert!(app.preferences.is_some());
        assert!(app.menu.open.is_none());
        let _ = app.update_preferences(crate::preferences::Event::Cancel);
        let _ = app.update_menu(Event::Toggle(0));
        let mut ui = iced_test::simulator(app.view());
        ui.point_at(iced::Point::new(600., 400.));
        ui.simulate(iced_test::simulator::click());
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert!(app.menu.open.is_none());
        let _ = app.update_menu(Event::Toggle(0));
        let mut ui = iced_test::simulator(app.view());
        ui.tap_key(keyboard::key::Named::Escape);
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert!(app.menu.open.is_none());
    }

    #[test]
    fn keyboard_focus_opening_escape_and_mnemonics_follow_menu_modes() {
        use keyboard::key::Named;
        let mut app = App::default();
        let _ = app.update_menu(Event::Key(
            Named::Alt.into(),
            keyboard::Modifiers::default(),
        ));
        let _ = app.update_menu(Event::AltReleased);
        assert_eq!(app.menu.focused, Some(0));
        assert!(app.menu.open.is_none());
        let _ = app.update_menu(Event::Key(
            Named::ArrowDown.into(),
            keyboard::Modifiers::default(),
        ));
        assert_eq!(app.menu.open, Some(0));
        assert_eq!(app.menu.selected, Some(0));
        let _ = app.update_menu(Event::Key(
            Named::ArrowUp.into(),
            keyboard::Modifiers::default(),
        ));
        assert_eq!(app.menu.selected, Some(3));
        let _ = app.update_menu(Event::Key(
            Named::ArrowRight.into(),
            keyboard::Modifiers::default(),
        ));
        assert_eq!(app.menu.open, Some(1));
        assert_eq!(app.menu.selected, None);
        let _ = app.update_menu(Event::Key(
            Named::Escape.into(),
            keyboard::Modifiers::default(),
        ));
        assert_eq!(app.menu.focused, Some(1));
        assert!(app.menu.open.is_none());
        let _ = app.update_menu(Event::Key(
            Named::Escape.into(),
            keyboard::Modifiers::default(),
        ));
        assert!(app.menu.focused.is_none());
        let _ = app.update_menu(Event::Key(
            keyboard::Key::Character("f".into()),
            keyboard::Modifiers::ALT,
        ));
        let _ = app.update_menu(Event::AltReleased);
        assert_eq!(app.menu.open, Some(0));
        let _ = app.update_menu(Event::Key(
            keyboard::Key::Character("p".into()),
            keyboard::Modifiers::default(),
        ));
        assert!(app.show_audio_path);
        assert!(app.menu.focused.is_none());
    }

    #[test]
    fn compact_menu_opens_on_press_and_accepts_drag_release() {
        let mut app = App::default();
        let mut ui = iced_test::simulator(app.view());
        let file = ui.find("File").unwrap().bounds();
        let edit = ui.find("Edit").unwrap().bounds();
        assert_eq!(edit.x - file.x, 36.);
        assert!(file.height < 18.);
        ui.point_at(file.center());
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert_eq!(app.menu.open, Some(0));
        assert!(app.menu.selected.is_none());
        let mut ui = iced_test::simulator(app.view());
        let item = ui.find("Master audio from path…").unwrap().bounds();
        let exit = ui.find("Exit").unwrap().bounds();
        assert!(exit.y - item.y < 60.);
        assert!(ui.find("Ctrl+O").unwrap().bounds().x > item.x);
        ui.point_at(item.center());
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))]);
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert!(app.show_audio_path);
        assert!(app.menu.open.is_none());
    }

    #[test]
    fn history_context_menu_opens_at_pointer_and_stays_inside_window() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.wav");
        std::fs::write(&source, b"audio").unwrap();
        let track = crate::library::import(dir.path(), source, crate::Uuid::new_v4()).unwrap();
        let mut app = App {
            root: Some(dir.path().to_path_buf()),
            ..App::default()
        };
        let mut job = crate::model::test_job(
            track.input.clone(),
            dir.path().join("output.wav"),
            app.library.draft.parse().unwrap(),
            1,
        );
        job.track_id = track.id;
        app.library.jobs.push(job);
        app.library.tracks.push(track);
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(640., 480.),
            app.view(),
        );
        let target = ui.find("Date unknown").unwrap().bounds().center();
        ui.point_at(target);
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Right,
        ))]);
        for message in ui.into_messages() {
            let _ = app.update(message);
        }
        assert_eq!(app.menu.context_position, Some(target));
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(640., 480.),
            app.view(),
        );
        let remove = ui
            .find("Remove selected")
            .unwrap()
            .visible_bounds()
            .unwrap();
        assert!(remove.x >= target.x && remove.y >= target.y);
        assert!(ui.find("Options").is_err());
        drop(ui);
        app.menu.context_position = Some(iced::Point::new(639., 479.));
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(640., 480.),
            app.view(),
        );
        let remove = ui
            .find("Remove selected")
            .unwrap()
            .visible_bounds()
            .unwrap();
        assert!(remove.x + remove.width <= 640. && remove.y + remove.height <= 480.);
    }
}
