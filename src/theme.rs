//! Controls that are recognizable as interactive even before hover.
//! Use visible borders and depth with iced 0.14 catalogs and standard styles.
//!
//! The standard button has no focus status; input/editor focus and persistent
//! selections use blue borders. No custom widgets or drawing are involved.
use iced::widget::{
    button, checkbox, container, overlay::menu, pick_list, progress_bar, rule, scrollable, slider,
    text, text_editor, text_input,
};
use iced::{Background, Border, Color, Shadow, gradient};

#[derive(Debug, Clone, Copy, Default)]
pub struct Theme;

const DESKTOP: Color = iced::color!(0xEAEAE7);
const INK: Color = iced::color!(0x202020);
const EDGE: Color = iced::color!(0x7F8C9D);
const BLUE: Color = iced::color!(0x245DAD);
const MUTED: Color = iced::color!(0x68685F);
const DISABLED: Color = iced::color!(0xE2E0D5);
pub const LABEL: Color = INK;
pub const DISABLED_LABEL: Color = MUTED;
pub const ERROR: Color = iced::color!(0xA52D28);

pub fn status_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::color!(0xF0F0EE).into()),
        text_color: Some(INK),
        ..Default::default()
    }
}

impl iced::theme::Base for Theme {
    fn default(_: iced::theme::Mode) -> Self {
        Self
    }
    fn mode(&self) -> iced::theme::Mode {
        iced::theme::Mode::Light
    }
    fn base(&self) -> iced::theme::Style {
        iced::theme::Style {
            background_color: DESKTOP,
            text_color: INK,
        }
    }
    fn palette(&self) -> Option<iced::theme::Palette> {
        Some(iced::theme::Palette {
            background: DESKTOP,
            text: INK,
            primary: BLUE,
            success: iced::color!(0x357A32),
            danger: ERROR,
            ..iced::Theme::Light.palette()
        })
    }
    fn name(&self) -> &str {
        "Clear controls"
    }
}

fn border(color: Color, width: f32) -> Border {
    Border {
        color,
        width,
        radius: 3.0.into(),
    }
}

fn vertical(top: Color, bottom: Color) -> Background {
    // iced uses a clockwise angle from the upward direction (PI points down).
    gradient::Linear::new(std::f32::consts::PI)
        .add_stop(0.0, top)
        .add_stop(1.0, bottom)
        .into()
}

pub fn normal(_: &Theme, status: button::Status) -> button::Style {
    button_style(status, false)
}

/// Persistent selections (history, A/B and tabs) stay distinct from hover.
pub fn selected(_: &Theme, status: button::Status) -> button::Style {
    button_style(status, true)
}

fn button_style(status: button::Status, selected: bool) -> button::Style {
    let (top, bottom, edge, ink) = match status {
        button::Status::Disabled => (DISABLED, DISABLED, iced::color!(0xAAA99F), MUTED),
        button::Status::Pressed => (iced::color!(0xB5C9E2), iced::color!(0xE0EAF5), BLUE, INK),
        button::Status::Hovered => (iced::color!(0xFFFAE8), iced::color!(0xEED9A1), BLUE, INK),
        button::Status::Active if selected => {
            (iced::color!(0xE2EDFC), iced::color!(0xACC9EF), BLUE, INK)
        }
        button::Status::Active => (iced::color!(0xFFFFFF), iced::color!(0xDEDAC9), EDGE, INK),
    };
    button::Style {
        background: Some(vertical(top, bottom)),
        text_color: ink,
        border: border(edge, if selected { 2.0 } else { 1.0 }),
        ..button::Style::default()
    }
}

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let (edge, width) = match status {
        text_input::Status::Focused { .. } => (BLUE, 2.0),
        text_input::Status::Hovered => (BLUE, 1.0),
        _ => (EDGE, 1.0),
    };
    let disabled = matches!(status, text_input::Status::Disabled);
    text_input::Style {
        background: if disabled { DISABLED } else { Color::WHITE }.into(),
        border: border(edge, width),
        icon: INK,
        placeholder: MUTED,
        value: if disabled { MUTED } else { INK },
        selection: iced::color!(0xA9CDF7),
    }
}

pub fn invalid_group(_: &Theme) -> container::Style {
    container::Style {
        border: border(ERROR, 1.0),
        ..container::Style::default()
    }
}

/// Hover details must obscure the labels underneath the overlay.
pub fn tooltip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.into()),
        text_color: Some(INK),
        border: border(EDGE, 1.0),
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.18),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
        ..container::Style::default()
    }
}
pub fn invalid_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        border: border(ERROR, 2.0),
        ..input(theme, status)
    }
}
pub fn disabled_slider(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = seek(theme, status);
    style.rail.backgrounds = (MUTED.into(), DISABLED.into());
    style.handle.background = DISABLED.into();
    style.handle.border_color = MUTED;
    style
}

fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let status = match status {
        text_editor::Status::Active => text_input::Status::Active,
        text_editor::Status::Hovered => text_input::Status::Hovered,
        text_editor::Status::Focused { is_hovered } => text_input::Status::Focused { is_hovered },
        text_editor::Status::Disabled => text_input::Status::Disabled,
    };
    let style = input(theme, status);
    text_editor::Style {
        background: style.background,
        border: style.border,
        placeholder: style.placeholder,
        value: style.value,
        selection: style.selection,
    }
}

fn check(_: &Theme, status: checkbox::Status) -> checkbox::Style {
    let (checked, hovered, disabled) = match status {
        checkbox::Status::Active { is_checked } => (is_checked, false, false),
        checkbox::Status::Hovered { is_checked } => (is_checked, true, false),
        checkbox::Status::Disabled { is_checked } => (is_checked, false, true),
    };
    checkbox::Style {
        background: if disabled {
            DISABLED.into()
        } else {
            vertical(iced::color!(0xE3E7EC), Color::WHITE)
        },
        icon_color: if disabled {
            MUTED
        } else {
            iced::color!(0x247333)
        },
        border: border(if hovered || checked { BLUE } else { EDGE }, 1.0),
        text_color: Some(if disabled { MUTED } else { INK }),
    }
}

fn pick(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: INK,
        placeholder_color: MUTED,
        handle_color: BLUE,
        background: vertical(Color::WHITE, iced::color!(0xE8E5D8)),
        border: border(
            if matches!(status, pick_list::Status::Active) {
                EDGE
            } else {
                BLUE
            },
            if matches!(status, pick_list::Status::Opened { .. }) {
                2.0
            } else {
                1.0
            },
        ),
    }
}

fn dropdown(_: &Theme) -> menu::Style {
    menu::Style {
        background: Color::WHITE.into(),
        border: border(EDGE, 1.0),
        text_color: INK,
        selected_text_color: Color::WHITE,
        selected_background: BLUE.into(),
        shadow: Shadow::default(),
    }
}

fn seek(_: &Theme, status: slider::Status) -> slider::Style {
    let active = matches!(status, slider::Status::Active);
    slider::Style {
        rail: slider::Rail {
            backgrounds: (BLUE.into(), iced::color!(0xCCC9BD).into()),
            width: 4.0,
            border: border(EDGE, 1.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Rectangle {
                width: 13,
                border_radius: 2.0.into(),
            },
            background: if matches!(status, slider::Status::Dragged) {
                iced::color!(0xACC9EF).into()
            } else {
                vertical(Color::WHITE, iced::color!(0xD4DCCB))
            },
            border_width: 1.0,
            border_color: if active { EDGE } else { BLUE },
        },
    }
}

fn scroll(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(&iced::Theme::Light, status);
    let (vertical_highlighted, horizontal_highlighted) = match status {
        scrollable::Status::Active { .. } => (false, false),
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
            ..
        } => (
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
        ),
        scrollable::Status::Dragged {
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
            ..
        } => (
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        ),
    };
    for (rail, highlighted) in [
        (&mut style.vertical_rail, vertical_highlighted),
        (&mut style.horizontal_rail, horizontal_highlighted),
    ] {
        rail.background = Some(iced::color!(0xE0DED3).into());
        rail.border = border(iced::color!(0xC4C1B5), 1.0);
        rail.scroller.background = if highlighted {
            iced::color!(0xB4CBE8).into()
        } else {
            vertical(iced::color!(0xF7F9FC), iced::color!(0xCAD4E3))
        };
        rail.scroller.border = border(if highlighted { BLUE } else { EDGE }, 1.0);
    }
    style.gap = Some(DESKTOP.into());
    style
}

fn progress(_: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Color::WHITE.into(),
        bar: vertical(iced::color!(0x9DCC72), iced::color!(0x458D36)),
        border: border(EDGE, 1.0),
    }
}

// Keep Catalog boilerplate together. StyleFn preserves iced's standard .style API.
macro_rules! status_catalog {
    ($module:ident, $style:ident) => {
        impl $module::Catalog for Theme {
            type Class<'a> = $module::StyleFn<'a, Self>;
            fn default<'a>() -> $module::StyleFn<'a, Self> {
                Box::new($style)
            }
            fn style(
                &self,
                class: &$module::StyleFn<'_, Self>,
                status: $module::Status,
            ) -> $module::Style {
                class(self, status)
            }
        }
    };
}
macro_rules! catalog {
    ($module:ident, $style:expr) => {
        impl $module::Catalog for Theme {
            type Class<'a> = $module::StyleFn<'a, Self>;
            fn default<'a>() -> $module::StyleFn<'a, Self> {
                Box::new($style)
            }
            fn style(&self, class: &$module::StyleFn<'_, Self>) -> $module::Style {
                class(self)
            }
        }
    };
}
status_catalog!(button, normal);
status_catalog!(text_input, input);
status_catalog!(text_editor, editor);
status_catalog!(checkbox, check);
status_catalog!(pick_list, pick);
status_catalog!(slider, seek);
status_catalog!(scrollable, scroll);
catalog!(menu, dropdown);
catalog!(progress_bar, progress);
catalog!(container, container::transparent);
catalog!(text, |_: &Theme| text::Style::default());
catalog!(rule, |_: &Theme| rule::Style {
    color: iced::color!(0xB7B3A5),
    ..rule::default(&iced::Theme::Light)
});

/// Flat list rows use a persistent selection color without button bevels.
pub fn history_row(
    _: &Theme,
    status: button::Status,
    selected: bool,
    failed: bool,
) -> button::Style {
    button::Style {
        background: Some(
            if selected {
                BLUE
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                iced::color!(0xE2EDFC)
            } else {
                Color::WHITE
            }
            .into(),
        ),
        text_color: if selected {
            Color::WHITE
        } else if failed {
            ERROR
        } else {
            INK
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_are_bounded_without_hover_and_states_remain_distinct() {
        let class = <Theme as button::Catalog>::default();
        let states = [
            button::Status::Active,
            button::Status::Hovered,
            button::Status::Pressed,
            button::Status::Disabled,
        ];
        let styles = states.map(|status| button::Catalog::style(&Theme, &class, status));
        for (index, style) in styles.iter().enumerate() {
            assert!(style.background.is_some());
            assert!(style.border.width >= 1.0);
            assert!(style.border.color.a > 0.0);
            for other in &styles[..index] {
                assert_ne!(style, other);
            }
        }
        assert_ne!(selected(&Theme, states[0]), styles[0]);
        assert_ne!(selected(&Theme, states[1]), styles[1]);
    }

    #[test]
    fn editable_fields_have_white_background_and_visible_focus() {
        let class = <Theme as text_input::Catalog>::default();
        let active = text_input::Catalog::style(&Theme, &class, text_input::Status::Active);
        let focused = text_input::Catalog::style(
            &Theme,
            &class,
            text_input::Status::Focused { is_hovered: false },
        );
        let disabled = text_input::Catalog::style(&Theme, &class, text_input::Status::Disabled);
        assert_eq!(active.background, Background::Color(Color::WHITE));
        assert!(active.border.width >= 1.0);
        assert_ne!(active.border.color, focused.border.color);
        assert!(focused.border.width > active.border.width);
        assert_ne!(disabled.background, active.background);
        assert_ne!(Background::Color(focused.selection), focused.background);
    }

    #[test]
    fn choices_and_scrollers_are_visible_before_hover() {
        let unchecked = check(&Theme, checkbox::Status::Active { is_checked: false });
        let checked = check(&Theme, checkbox::Status::Active { is_checked: true });
        assert!(unchecked.border.width >= 1.0);
        assert_ne!(checked.border, unchecked.border);
        assert_ne!(
            pick(&Theme, pick_list::Status::Active),
            pick(&Theme, pick_list::Status::Opened { is_hovered: false })
        );
        let style = scroll(
            &Theme,
            scrollable::Status::Active {
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        );
        for rail in [style.vertical_rail, style.horizontal_rail] {
            assert!(rail.background.is_some());
            assert!(rail.scroller.border.width >= 1.0);
            assert_ne!(rail.background, Some(rail.scroller.background));
        }
    }
}
