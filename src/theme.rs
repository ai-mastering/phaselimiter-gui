//! Application styling uses iced's semantic palette and standard widget styles.
//! The wrapper adds visible control boundaries without replacing theme semantics.
use iced::widget::{
    button, checkbox, container, overlay::menu, pick_list, progress_bar, rule, scrollable, slider,
    text, text_editor, text_input,
};
use iced::{Border, Color, Shadow};

#[derive(Debug, Clone, PartialEq)]
pub struct Theme(iced::Theme);

impl Default for Theme {
    fn default() -> Self {
        Self(iced::Theme::custom(
            "Clear controls",
            iced::theme::Palette {
                background: iced::color!(0xEAEAEA),
                text: iced::color!(0x202020),
                primary: iced::color!(0x245DAD),
                success: iced::color!(0x357A32),
                danger: iced::color!(0xA52D28),
                ..iced::Theme::Light.palette()
            },
        ))
    }
}

impl From<iced::Theme> for Theme {
    fn from(theme: iced::Theme) -> Self {
        Self(theme)
    }
}

impl Theme {
    pub fn extended_palette(&self) -> &iced::theme::palette::Extended {
        self.0.extended_palette()
    }

    pub fn choices() -> Vec<String> {
        std::iter::once("Clear controls".to_owned())
            .chain(iced::Theme::ALL.iter().map(ToString::to_string))
            .collect()
    }

    /// Missing or unknown saved names fall back to the application's theme.
    pub fn from_name(name: &str) -> Self {
        iced::Theme::ALL
            .iter()
            .find(|theme| theme.to_string().eq_ignore_ascii_case(name))
            .cloned()
            .map(Self::from)
            .unwrap_or_default()
    }

    pub fn name(&self) -> &str {
        iced::theme::Base::name(&self.0)
    }
}

impl iced::theme::Base for Theme {
    fn default(_: iced::theme::Mode) -> Self {
        <Self as Default>::default()
    }
    fn mode(&self) -> iced::theme::Mode {
        iced::theme::Base::mode(&self.0)
    }
    fn base(&self) -> iced::theme::Style {
        iced::theme::Base::base(&self.0)
    }
    fn palette(&self) -> Option<iced::theme::Palette> {
        Some(self.0.palette())
    }
    fn name(&self) -> &str {
        iced::theme::Base::name(&self.0)
    }
}

pub const SMALL_TEXT: f32 = 12.;
pub const HEADING_TEXT: f32 = 18.;
pub const ITEM_GAP: f32 = 4.;
pub const SECTION_GAP: f32 = 8.;

pub fn label(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().background.base.text),
    }
}
pub fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(
            theme
                .extended_palette()
                .background
                .base
                .text
                .scale_alpha(0.6),
        ),
    }
}
pub fn error(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().danger.base.color),
    }
}

fn border(color: Color, width: f32) -> Border {
    Border {
        color,
        width,
        radius: 3.0.into(),
    }
}

pub fn status_bar(theme: &Theme) -> container::Style {
    let pair = theme.extended_palette().background.weakest;
    container::Style {
        background: Some(pair.color.into()),
        text_color: Some(pair.text),
        ..Default::default()
    }
}

pub fn normal(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    let mut style = button::background(&theme.0, status);
    style.border = border(
        if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            p.primary.base.color
        } else {
            p.secondary.base.color
        },
        1.0,
    );
    style
}

/// Persistent selections use the primary color and its paired foreground.
pub fn selected(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::primary(&theme.0, status);
    style.border = border(theme.extended_palette().primary.strong.color, 2.0);
    style
}

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(&theme.0, status);
    if matches!(status, text_input::Status::Focused { .. }) {
        style.border.width = 2.0;
    }
    style
}

pub fn invalid_group(theme: &Theme) -> container::Style {
    container::Style {
        border: border(theme.extended_palette().danger.base.color, 1.0),
        ..Default::default()
    }
}

/// Opaque surfaces use a matched background/foreground pair in every theme.
pub fn tooltip(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.base.color.into()),
        text_color: Some(p.background.base.text),
        border: border(p.secondary.base.color, 1.0),
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.18),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 6.0,
        },
        ..Default::default()
    }
}

pub fn help(theme: &Theme) -> container::Style {
    let pair = theme.extended_palette().background.weak;
    container::Style {
        background: Some(pair.color.into()),
        text_color: Some(pair.text),
        border: border(theme.extended_palette().secondary.base.color, 1.0),
        ..Default::default()
    }
}

pub fn invalid_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        border: border(theme.extended_palette().danger.base.color, 2.0),
        ..input(theme, status)
    }
}

pub fn disabled_slider(theme: &Theme, status: slider::Status) -> slider::Style {
    let p = theme.extended_palette();
    let mut style = seek(theme, status);
    style.rail.backgrounds = (
        p.background.strong.color.into(),
        p.background.weak.color.into(),
    );
    style.handle.background = p.background.strong.color.into();
    style.handle.border_color = p.secondary.base.color;
    style
}

fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut style = text_editor::default(&theme.0, status);
    if matches!(status, text_editor::Status::Focused { .. }) {
        style.border.width = 2.0;
    }
    style
}
fn check(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    checkbox::primary(&theme.0, status)
}
fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    pick_list::default(&theme.0, status)
}
fn dropdown(theme: &Theme) -> menu::Style {
    menu::default(&theme.0)
}
fn seek(theme: &Theme, status: slider::Status) -> slider::Style {
    slider::default(&theme.0, status)
}
fn scroll(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    scrollable::default(&theme.0, status)
}
fn progress(theme: &Theme) -> progress_bar::Style {
    progress_bar::primary(&theme.0)
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
catalog!(rule, |theme: &Theme| rule::default(&theme.0));

/// Flat history rows reserve primary for selection and danger for failures.
pub fn history_row(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    failed: bool,
) -> button::Style {
    let p = theme.extended_palette();
    let pair = if selected {
        p.primary.base
    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        p.background.weak
    } else {
        p.background.base
    };
    button::Style {
        background: Some(pair.color.into()),
        text_color: if failed && !selected {
            p.danger.base.color
        } else {
            pair.text
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Background;

    fn themes() -> impl Iterator<Item = Theme> {
        std::iter::once(Theme::default()).chain(iced::Theme::ALL.iter().cloned().map(Theme::from))
    }

    #[test]
    fn saved_theme_names_round_trip_and_old_preferences_use_the_default() {
        for name in Theme::choices() {
            assert_eq!(Theme::from_name(&name).name(), name);
        }
        assert_eq!(Theme::from_name(""), Theme::default());
        assert_eq!(Theme::from_name("unknown future theme"), Theme::default());
        let mut old = serde_json::to_value(crate::model::AppSettings::default()).unwrap();
        old.as_object_mut().unwrap().remove("theme");
        let restored: crate::model::AppSettings = serde_json::from_value(old).unwrap();
        assert_eq!(Theme::from_name(&restored.theme), Theme::default());
    }

    #[test]
    fn controls_keep_boundaries_and_interaction_states_across_themes() {
        for theme in themes() {
            let states = [
                button::Status::Active,
                button::Status::Hovered,
                button::Status::Pressed,
                button::Status::Disabled,
            ];
            let styles = states.map(|status| normal(&theme, status));
            for (i, style) in styles.iter().enumerate() {
                assert!(style.background.is_some());
                assert!(style.border.width >= 1.0 && style.border.color.a > 0.0);
                for other in &styles[..i] {
                    assert_ne!(style, other);
                }
                assert_ne!(*style, selected(&theme, states[i]));
            }
            let active = input(&theme, text_input::Status::Active);
            let focus = input(&theme, text_input::Status::Focused { is_hovered: false });
            assert!(active.border.width >= 1.0);
            assert_ne!(active.border.color, focus.border.color);
            assert!(focus.border.width > active.border.width);
            assert_ne!(input(&theme, text_input::Status::Disabled), active);
            assert_ne!(
                check(&theme, checkbox::Status::Active { is_checked: false }),
                check(&theme, checkbox::Status::Active { is_checked: true })
            );
            assert_ne!(
                pick(&theme, pick_list::Status::Active),
                pick(&theme, pick_list::Status::Opened { is_hovered: false })
            );
        }
    }

    #[test]
    fn selection_and_errors_follow_semantic_colors_and_paired_foregrounds() {
        for theme in themes() {
            let p = theme.extended_palette();
            let selected = selected(&theme, button::Status::Active);
            assert_eq!(selected.background, Some(p.primary.base.color.into()));
            assert_eq!(selected.text_color, p.primary.base.text);
            let history = history_row(&theme, button::Status::Active, true, true);
            assert_eq!(history.background, selected.background);
            assert_eq!(history.text_color, selected.text_color);
            assert_eq!(
                invalid_input(&theme, text_input::Status::Active)
                    .border
                    .color,
                p.danger.base.color
            );
            assert_eq!(error(&theme).color, Some(p.danger.base.color));
            assert_eq!(
                tooltip(&theme).background,
                Some(p.background.base.color.into())
            );
            assert_eq!(tooltip(&theme).text_color, Some(p.background.base.text));
        }
    }

    #[test]
    fn custom_palette_controls_are_neutral_and_wrapper_preserves_theme_mode() {
        let theme = Theme::default();
        let Some(Background::Color(color)) = normal(&theme, button::Status::Active).background
        else {
            panic!("expected a solid resting surface");
        };
        assert!((color.r - color.g).abs() < 0.0001 && (color.g - color.b).abs() < 0.0001);
        for base in iced::Theme::ALL {
            let theme = Theme::from(base.clone());
            assert_eq!(theme.extended_palette(), base.extended_palette());
            assert_eq!(
                iced::theme::Base::mode(&theme),
                iced::theme::Base::mode(base)
            );
            assert_eq!(
                iced::theme::Base::base(&theme).background_color,
                base.palette().background
            );
        }
    }
}
