//! Read-only single-line fields and selectable logs with visible scrollbars.
use crate::{Element, Message, Theme};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::widget::{scrollable, text_editor};
use iced::{Length, Rectangle, Size, Vector};

/// Shared constructor for editable one-line fields; retains the native builder API.
pub fn input<'a, M: Clone>(
    placeholder: &str,
    value: &str,
) -> iced::widget::TextInput<'a, M, Theme> {
    iced::widget::TextInput::new(placeholder, value)
}

pub fn read_only_input<'a>(id: &'static str, value: &str) -> Element<'a, Message> {
    Element::new(ReadOnly {
        content: input("", value)
            .id(id)
            .size(crate::theme::SMALL_TEXT)
            .padding(5)
            .on_input(|_| Message::ReadOnlyEdit)
            .into(),
        scroll_id: None,
    })
}

pub fn read_only_editor<'a>(
    id: &'static str,
    content: &'a text_editor::Content,
    height: impl Into<Length>,
    on_action: impl Fn(text_editor::Action) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(ReadOnly {
        content: scrollable(
            text_editor(content)
                .id(format!("{id}-text"))
                .on_action(on_action)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .font(iced::Font::MONOSPACE)
                .size(crate::theme::SMALL_TEXT)
                .height(Length::Shrink),
        )
        .id(id)
        .spacing(crate::theme::ITEM_GAP)
        .height(height)
        .width(iced::Fill)
        .into(),
        scroll_id: Some(id),
    })
}

struct ReadOnly<'a> {
    content: Element<'a, Message>,
    scroll_id: Option<&'static str>,
}
#[derive(Default)]
struct State {
    focused: bool,
    dragging: bool,
}
struct FocusQuery(bool);
impl widget::Operation for FocusQuery {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
        operate(self);
    }
    fn focusable(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn widget::operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }
}

fn blocks_edit(event: &iced::Event) -> bool {
    use iced::keyboard::{Key, key::Named};
    match event {
        iced::Event::InputMethod(_) => true,
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            modifiers,
            physical_key,
            ..
        }) => {
            let copy_or_select =
                modifiers.command() && matches!(key.to_latin(*physical_key), Some('a' | 'c'));
            !copy_or_select
                && !matches!(
                    key,
                    Key::Named(
                        Named::ArrowLeft
                            | Named::ArrowRight
                            | Named::ArrowUp
                            | Named::ArrowDown
                            | Named::Home
                            | Named::End
                            | Named::PageUp
                            | Named::PageDown
                            | Named::Tab
                            | Named::Escape
                    )
                )
        }
        _ => false,
    }
}

impl Widget<Message, Theme, iced::Renderer> for ReadOnly<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
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
        let mut focus = FocusQuery(false);
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut focus);
        let state = tree.state.downcast_mut::<State>();
        state.focused = focus.0;
        let visible = layout.bounds().intersection(viewport).unwrap_or_default();
        if matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            state.focused = cursor.is_over(visible);
            state.dragging = self.scroll_id.is_some()
                && state.focused
                && cursor
                    .position()
                    .is_some_and(|p| p.x < visible.x + visible.width - 16.);
        }
        if matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | iced::Event::Window(iced::window::Event::Unfocused)
        ) {
            state.dragging = false;
        }
        if blocks_edit(event) {
            if state.focused {
                shell.capture_event();
            }
            return;
        }
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
        if self.scroll_id.is_some() {
            let mut scroll = 0.;
            if state.focused
                && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    ..
                }) = event
            {
                use iced::keyboard::{Key, key::Named};
                scroll = match key {
                    Key::Named(Named::ArrowUp) => -18.,
                    Key::Named(Named::ArrowDown) => 18.,
                    Key::Named(Named::PageUp) => -visible.height,
                    Key::Named(Named::PageDown) => visible.height,
                    Key::Named(Named::Home) if modifiers.command() => -f32::MAX,
                    Key::Named(Named::End) if modifiers.command() => f32::MAX,
                    _ => 0.,
                };
            }
            let drag_point = if state.dragging && visible.width > 24. && visible.height > 12. {
                cursor.position()
            } else {
                None
            };
            if let Some(point) = drag_point {
                if point.y < visible.y + 12. {
                    scroll = -12.;
                } else if point.y > visible.y + visible.height - 12. {
                    scroll = 12.;
                }
            }
            if scroll != 0. {
                let mut operation = widget::operation::scrollable::scroll_by::<()>(
                    self.scroll_id.unwrap_or_default().into(),
                    widget::operation::scrollable::AbsoluteOffset { x: 0., y: scroll },
                );
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut operation,
                );
                if let Some(point) = drag_point {
                    let point = iced::Point::new(
                        point
                            .x
                            .clamp(visible.x + 6., visible.x + visible.width - 18.),
                        point
                            .y
                            .clamp(visible.y + 6., visible.y + visible.height - 6.),
                    );
                    self.content.as_widget_mut().update(
                        &mut tree.children[0],
                        &iced::Event::Mouse(mouse::Event::CursorMoved { position: point }),
                        layout,
                        mouse::Cursor::Available(point),
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                    shell.request_redraw_at(
                        std::time::Instant::now() + std::time::Duration::from_millis(16),
                    );
                } else {
                    shell.request_redraw();
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Event, Point, keyboard, widget::Id};

    #[test]
    fn single_line_fields_block_edits_but_keep_selection_and_copy() {
        let value = "phase_limiter --input /long/path/日本語.wav ".repeat(40);
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            Size::new(240., 40.),
            read_only_input("command", &value),
        );
        let bounds = ui.find(Id::from("command")).unwrap().bounds();
        assert_eq!(bounds.width, 240.);
        ui.click(Id::from("command")).unwrap();
        ui.typewrite("cannot edit");
        ui.tap_key(keyboard::key::Named::Backspace);
        ui.simulate([Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::COMMAND,
        ))]);
        assert_eq!(
            ui.tap_key(keyboard::Key::Character("a".into())),
            iced::event::Status::Captured
        );
        assert_eq!(
            ui.tap_key(keyboard::Key::Character("c".into())),
            iced::event::Status::Captured
        );
        ui.tap_key(keyboard::Key::Character("v".into()));
        ui.tap_key(keyboard::Key::Character("x".into()));
        assert!(
            !ui.into_messages()
                .any(|m| matches!(m, Message::ReadOnlyEdit))
        );
    }

    fn log_offset(ui: &mut iced_test::Simulator<'_, Message, Theme>) -> f32 {
        match ui.find(Id::from("log-test")).unwrap() {
            iced_test::selector::Target::Scrollable { translation, .. } => translation.y,
            _ => panic!("log must be scrollable"),
        }
    }

    #[test]
    fn logs_have_scrollable_extent_and_selection_drag_scrolls_at_edges() {
        let content = text_editor::Content::with_text(
            &(0..100).map(|i| format!("line {i}\n")).collect::<String>(),
        );
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            Size::new(240., 160.),
            read_only_editor("log-test", &content, 160, Message::LogText),
        );
        let text_id = Id::from("log-test-text");
        let initial = ui.find(text_id.clone()).unwrap().bounds();
        assert!(initial.height > 160.);
        ui.point_at(Point::new(40., 70.));
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0., y: -3. },
        })]);
        let scrolled = log_offset(&mut ui);
        assert!(scrolled > 0.);
        ui.point_at(Point::new(10., 30.));
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        ui.point_at(Point::new(80., 180.));
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(80., 180.),
        })]);
        let dragged = log_offset(&mut ui);
        assert!(dragged > scrolled);
        ui.simulate([Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]);
        let continued = log_offset(&mut ui);
        assert!(continued > dragged);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))]);
        let released = log_offset(&mut ui);
        ui.simulate([Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]);
        assert_eq!(log_offset(&mut ui), released);
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::LogText(text_editor::Action::Drag(_))))
        );
    }
}
