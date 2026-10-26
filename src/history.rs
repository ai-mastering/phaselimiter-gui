//! Flat history ordering and keyboard navigation scoped to the history pane.
use crate::{App, Element, Message, ResultKey, Theme};

impl App {
    pub(crate) fn history_keys(&self) -> Vec<ResultKey> {
        let mut entries: Vec<_> = self
            .library
            .jobs
            .iter()
            .filter(|job| {
                self.library
                    .tracks
                    .iter()
                    .any(|track| track.id == job.track_id)
            })
            .map(|job| (job.created_at, (job.track_id, job.id)))
            .collect();
        entries.sort_by_key(|(time, _)| *time);
        entries.into_iter().rev().map(|(_, key)| key).collect()
    }
}

/// Keep the on-disk audio key compatible; originals are no longer history selections.
pub(crate) fn restore_selection(library: &crate::Library) -> Option<ResultKey> {
    let jobs = library
        .jobs
        .iter()
        .filter(|job| library.tracks.iter().any(|track| track.id == job.track_id));
    let selected = library.selected;
    jobs.clone()
        .find(|job| selected == Some((job.track_id, Some(job.id))))
        .or_else(|| {
            jobs.clone()
                .filter(|job| selected.is_some_and(|key| key.0 == job.track_id))
                .max_by_key(|job| job.created_at)
        })
        .or_else(|| jobs.max_by_key(|job| job.created_at))
        .map(|job| (job.track_id, job.id))
}

fn next_index(keys: &[ResultKey], selected: Option<ResultKey>, down: bool) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    Some(
        match selected.and_then(|key| keys.iter().position(|entry| *entry == key)) {
            Some(index) if down => (index + 1).min(keys.len() - 1),
            Some(index) => index.saturating_sub(1),
            None if down => 0,
            None => keys.len() - 1,
        },
    )
}

pub(crate) struct HistoryInput<'a> {
    pub content: Element<'a, Message>,
    pub keys: Vec<ResultKey>,
    pub selected: Option<ResultKey>,
    pub offset: f32,
}

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Length, Rectangle, Size, Vector};
impl Widget<Message, Theme, iced::Renderer> for HistoryInput<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<bool>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(false)
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
        let focused = tree.state.downcast_mut::<bool>();
        if matches!(event, iced::Event::Mouse(mouse::Event::ButtonPressed(_))) {
            *focused = cursor.is_over(layout.bounds());
        }
        if matches!(event, iced::Event::Window(iced::window::Event::Unfocused)) {
            *focused = false;
        }
        if *focused
            && let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key, modifiers, ..
            }) = event
        {
            use iced::keyboard::{Key, key::Named};
            if matches!(key, Key::Named(Named::Tab)) {
                *focused = false;
            }
            if modifiers.is_empty() && matches!(key, Key::Named(Named::ArrowUp | Named::ArrowDown))
            {
                let down = matches!(key, Key::Named(Named::ArrowDown));
                if let Some(index) = next_index(&self.keys, self.selected, down) {
                    let top = index as f32 * 32.;
                    let bottom = top + 32.;
                    let offset = if top < self.offset {
                        top
                    } else if bottom > self.offset + layout.bounds().height {
                        (bottom - layout.bounds().height).max(0.)
                    } else {
                        self.offset
                    };
                    shell.publish(Message::HistoryNavigate(self.keys[index], offset));
                }
                shell.capture_event();
                return;
            }
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
impl<'a> From<HistoryInput<'a>> for Element<'a, Message> {
    fn from(menu: HistoryInput<'a>) -> Self {
        Self::new(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn navigation_handles_boundaries_and_missing_selection() {
        let keys = [
            (Uuid::from_u128(1), Uuid::from_u128(11)),
            (Uuid::from_u128(2), Uuid::from_u128(12)),
        ];
        assert_eq!(next_index(&[], None, true), None);
        assert_eq!(next_index(&keys, None, true), Some(0));
        assert_eq!(next_index(&keys, None, false), Some(1));
        assert_eq!(next_index(&keys, Some(keys[0]), false), Some(0));
        assert_eq!(next_index(&keys, Some(keys[0]), true), Some(1));
        assert_eq!(next_index(&keys, Some(keys[1]), true), Some(1));
    }

    #[test]
    fn arrows_select_only_after_clicking_the_list() {
        let keys = vec![
            (Uuid::from_u128(1), Uuid::from_u128(11)),
            (Uuid::from_u128(2), Uuid::from_u128(12)),
        ];
        let list: Element<'_, Message> = HistoryInput {
            content: iced::widget::container(iced::widget::text("Rows"))
                .width(200)
                .height(64)
                .into(),
            keys: keys.clone(),
            selected: Some(keys[0]),
            offset: 0.,
        }
        .into();
        let mut ui = iced_test::simulator(iced::widget::row![
            list,
            iced::widget::text("Outside").width(200)
        ]);
        let down = iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::ArrowDown),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        ui.simulate([down.clone()]);
        ui.click("Rows").unwrap();
        ui.simulate([down.clone()]);
        ui.click("Outside").unwrap();
        ui.simulate([down]);
        let messages: Vec<_> = ui.into_messages().collect();
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], Message::HistoryNavigate(key, _) if key == keys[1]));
    }
}
