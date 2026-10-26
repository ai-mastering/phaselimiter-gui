//! Shared settings controls and numeric editing contracts. No custom widgets.
use crate::{
    App, Element, Field, Message,
    model::{self, CeilingMode, Format, ReferenceMode, Settings},
    theme,
};
use iced::{
    Fill,
    widget::{button, column, opaque, pick_list, row, slider, space, stack, text, text_input},
};

#[derive(Clone, Copy, Debug)]
pub enum Number {
    Unsigned { min: u64, max: u64 },
    Decimal { min: f64, max: f64, step: f64 },
}
impl Number {
    pub fn partial(self, s: &str) -> bool {
        matches!(self, Self::Decimal { min, .. } if min < 0.) && matches!(s, "-" | "-.")
            || matches!(self, Self::Decimal { .. }) && s == "."
    }
    pub fn valid(self, s: &str) -> bool {
        match self {
            Self::Unsigned { min, max } => {
                !s.is_empty()
                    && s.bytes().all(|b| b.is_ascii_digit())
                    && s.parse::<u64>().is_ok_and(|v| (min..=max).contains(&v))
            }
            Self::Decimal { min, max, .. } => {
                let digits = s.strip_prefix('-').unwrap_or(s);
                !digits.is_empty()
                    && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                    && digits.matches('.').count() <= 1
                    && digits.split_once('.').is_none_or(|(_, d)| d.len() <= 2)
                    && s.parse::<f64>()
                        .is_ok_and(|v| v.is_finite() && (min..=max).contains(&v))
            }
        }
    }
    pub fn adjust(self, s: &str, up: bool) -> Option<String> {
        match self {
            Self::Unsigned { min, max } => {
                let v = s.parse::<u64>().unwrap_or(min).clamp(min, max);
                let next = if up {
                    v.saturating_add(1).min(max)
                } else {
                    v.saturating_sub(1).max(min)
                };
                (next != v || !self.valid(s)).then(|| next.to_string())
            }
            Self::Decimal { min, max, step } => {
                let v = s
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .unwrap_or(min)
                    .clamp(min, max);
                let next = ((v + if up { step } else { -step }) * 100.).round() / 100.;
                let next = next.clamp(min, max);
                (next != v || !self.valid(s)).then(|| format!("{next:.2}"))
            }
        }
    }
    pub fn hint(self) -> String {
        match self {
            Self::Unsigned { min, max } => format!("Use {min}–{max} (unsigned)."),
            Self::Decimal { min, max, .. } => format!("Use {min}–{max} (up to 2 decimals)."),
        }
    }
}
pub fn enabled(field: Field, s: &Settings) -> bool {
    match field {
        Field::BitDepth => s.format == Format::Wav,
        Field::LowpassCut => s.ceiling_mode == CeilingMode::LowpassTruePeak,
        Field::Loudness => s.reference_mode != ReferenceMode::Zero,
        Field::Intensity | Field::Evaluations | Field::Seed | Field::Reference => s.mastering,
        _ => true,
    }
}
fn finite(s: &str, fallback: f64) -> f64 {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(fallback)
}
pub fn spec(field: Field, s: &Settings) -> Option<Number> {
    let decimal = |min, max, step| Some(Number::Decimal { min, max, step });
    let uint = |min, max| Some(Number::Unsigned { min, max });
    match field {
        Field::Loudness => decimal(-60., 0., 0.1),
        Field::Ceiling => decimal(-20., 0., 0.1),
        Field::Intensity => decimal(0., 1., 0.01),
        Field::LowpassCut => decimal(
            0.01,
            finite(&s.sample_rate, 44100.).clamp(1., 44100.) / 2.,
            1.,
        ),
        Field::LowCut => decimal(
            0.,
            ((finite(&s.high_cut, 20000.) - 0.01) * 100.)
                .round()
                .clamp(0., 2204999.)
                / 100.,
            1.,
        ),
        Field::HighCut => decimal(
            if finite(&s.low_cut, 20.) > 0. {
                ((finite(&s.low_cut, 20.) + 0.01) * 100.)
                    .round()
                    .min(2204999.)
                    / 100.
            } else {
                0.
            },
            22049.99,
            1.,
        ),
        Field::MaxIter1 | Field::MaxIter2 | Field::Evaluations => uint(1, i32::MAX as u64),
        Field::Workers => uint(0, model::MAX_WORKERS as u64),
        Field::Seed => uint(0, u64::MAX),
        _ => None,
    }
}
pub fn valid_choice(field: Field, value: &str, s: &Settings) -> bool {
    match field {
        Field::Oversample => value
            .parse()
            .is_ok_and(|v| model::OVERSAMPLING.contains(&v)),
        Field::SampleRate => value
            .parse()
            .is_ok_and(|v| model::sample_rates(s.format).contains(&v)),
        Field::BitDepth => {
            enabled(field, s)
                && value
                    .parse::<u32>()
                    .is_ok_and(|v| [16, 24, 32].contains(&v))
        }
        _ => true,
    }
}
pub fn choice<'a, T: Clone + PartialEq + ToString + 'a>(
    options: Vec<T>,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
    enabled: bool,
) -> Element<'a, Message> {
    if enabled {
        pick_list(options, selected, on_select)
            .placeholder("Choose…")
            .width(Fill)
            .into()
    } else {
        button(text(format!(
            "{} ▾",
            selected
                .map(|s| s.to_string())
                .unwrap_or_else(|| "—".into())
        )))
        .width(Fill)
        .padding(6)
        .into()
    }
}
impl App {
    /// The same setting guard is used by the button and execution path.
    pub(super) fn settings_issue(&self) -> Option<(Option<Field>, String)> {
        if let Some(field) = first_invalid(self) {
            let label = match field {
                Field::Loudness => "Target level",
                Field::Ceiling => "Ceiling",
                Field::LowpassCut => "Lowpass true-peak frequency",
                Field::Intensity => "Intensity",
                Field::LowCut => "Low cut",
                Field::HighCut => "High cut",
                Field::SampleRate => "Sample rate",
                Field::BitDepth => "WAV bits",
                Field::Oversample => "Oversampling",
                Field::MaxIter1 => "Outer iterations",
                Field::MaxIter2 => "Inner iterations",
                Field::Evaluations => "Evaluations",
                Field::Seed => "Seed",
                Field::Workers => "Workers",
                _ => unreachable!("first_invalid only returns settings fields"),
            };
            let reason = if self.numeric_edits.contains_key(&field) {
                format!("{label}: finish entering a value, or press Enter to restore it.")
            } else if let Some(spec) = spec(field, &self.library.draft) {
                format!("{label}: {}", spec.hint())
            } else {
                format!("{label}: choose a supported value.")
            };
            return Some((Some(field), reason));
        }
        if let Err(error) = self.library.app_settings.resolved_tools().validate() {
            return Some((None, error));
        }
        if !self.reference_ready() {
            let reason = if self.library.draft.reference_audio.is_empty() {
                "Choose a reference audio file.".into()
            } else {
                self.ready_reference()
                    .err()
                    .unwrap_or_else(|| "Reference analysis is not ready.".into())
            };
            return Some((Some(Field::Reference), format!("Reference: {reason}")));
        }
        self.library.draft.parse().err().map(|error| (None, error))
    }

    pub(super) fn numeric<'a>(&'a self, label: &'static str, field: Field) -> Element<'a, Message> {
        let enabled = enabled(field, &self.library.draft);
        let value = self.field_value(field);
        let spec = spec(field, &self.library.draft);
        let unfinished = enabled && self.numeric_edits.contains_key(&field);
        let invalid = enabled
            && (spec.is_some_and(|n| !n.valid(value))
                || unfinished && self.numeric_errors.contains_key(&field));
        let buffer = if enabled {
            self.numeric_edits
                .get(&field)
                .map(String::as_str)
                .unwrap_or(value)
        } else {
            value
        };
        let mut input = text_input(label, buffer)
            .id(format!("number-{field:?}"))
            .padding(5)
            .style(if invalid {
                theme::invalid_input
            } else {
                theme::input
            });
        if enabled {
            input = input
                .on_input(move |s| Message::Edit(field, s))
                .on_submit(Message::CommitNumber(field));
        }
        let up = spec.and_then(|s| s.adjust(value, true)).is_some() && enabled;
        let down = spec.and_then(|s| s.adjust(value, false)).is_some() && enabled;
        let arrows = column![
            button(text("▲").size(9))
                .padding([1, 6])
                .on_press_maybe(up.then_some(Message::Adjust(field, true))),
            button(text("▼").size(9))
                .padding([1, 6])
                .on_press_maybe(down.then_some(Message::Adjust(field, false))),
        ]
        .spacing(1);
        let mut content = column![
            self.help_label(label, crate::descriptions::Topic::Field(field)),
            row![input, arrows].spacing(3)
        ]
        .spacing(3)
        .width(Fill);
        if (invalid || enabled && self.numeric_errors.contains_key(&field))
            && let Some(spec) = spec
        {
            content = content.push(
                text(
                    self.numeric_errors
                        .get(&field)
                        .cloned()
                        .unwrap_or_else(|| spec.hint()),
                )
                .size(11)
                .color(if invalid {
                    theme::ERROR
                } else {
                    theme::DISABLED_LABEL
                }),
            );
        }
        if unfinished {
            content = content.push(
                text(format!(
                    "Unfinished input; current setting: {value}. Enter to restore."
                ))
                .size(11)
                .color(theme::DISABLED_LABEL),
            );
        }
        group(
            self.help_body(crate::descriptions::Topic::Field(field), content.into()),
            field,
            invalid,
        )
    }
    pub(super) fn numeric_choice(
        &self,
        label: &'static str,
        field: Field,
        options: &[u32],
    ) -> Element<'_, Message> {
        let enabled = enabled(field, &self.library.draft);
        let selected = self
            .field_value(field)
            .parse::<u32>()
            .ok()
            .filter(|v| options.contains(v));
        let invalid = enabled && selected.is_none();
        let mut col = column![
            self.help_label(label, crate::descriptions::Topic::Field(field)),
            choice(
                options.to_vec(),
                selected,
                move |v| Message::Edit(field, v.to_string()),
                enabled
            )
        ]
        .spacing(3)
        .width(Fill);
        if invalid {
            col = col.push(
                text("Choose a supported value.")
                    .size(11)
                    .color(theme::ERROR),
            );
        }
        group(
            self.help_body(crate::descriptions::Topic::Field(field), col.into()),
            field,
            invalid,
        )
    }
    pub(super) fn intensity_control(&self) -> Element<'_, Message> {
        let enabled = self.library.draft.mastering;
        let value = self
            .library
            .draft
            .intensity
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(1.)
            .clamp(0., 1.);
        let bar = slider(0..=100_u32, (value * 100.).round() as u32, |v| {
            Message::Edit(Field::Intensity, format!("{:.2}", v as f64 / 100.))
        })
        .step(1_u32);
        let bar: Element<'_, Message> = if enabled {
            bar.into()
        } else {
            stack![
                bar.style(theme::disabled_slider),
                opaque(space().width(Fill).height(Fill))
            ]
            .into()
        };
        let invalid = enabled
            && !spec(Field::Intensity, &self.library.draft)
                .is_some_and(|n| n.valid(&self.library.draft.intensity));
        let mut content = column![
            self.help_label(
                "Intensity (0–1)",
                crate::descriptions::Topic::Field(Field::Intensity)
            ),
            row![
                bar,
                text(format!("{value:.2}")).width(42).color(if enabled {
                    theme::LABEL
                } else {
                    theme::DISABLED_LABEL
                })
            ]
            .spacing(8)
            .align_y(iced::Center)
        ]
        .spacing(5);
        if invalid {
            content = content.push(text("Use 0.00–1.00.").size(11).color(theme::ERROR));
        }
        group(
            self.help_body(
                crate::descriptions::Topic::Field(Field::Intensity),
                content.into(),
            ),
            Field::Intensity,
            invalid,
        )
    }
}

fn group(content: Element<'_, Message>, field: Field, invalid: bool) -> Element<'_, Message> {
    iced::widget::container(content)
        .id(format!("group-{field:?}"))
        .style(if invalid {
            theme::invalid_group
        } else {
            iced::widget::container::transparent
        })
        .into()
}
pub fn first_invalid(app: &App) -> Option<Field> {
    [
        Field::Loudness,
        Field::Ceiling,
        Field::LowpassCut,
        Field::Intensity,
        Field::LowCut,
        Field::HighCut,
        Field::SampleRate,
        Field::BitDepth,
        Field::Oversample,
        Field::MaxIter1,
        Field::MaxIter2,
        Field::Evaluations,
        Field::Seed,
        Field::Workers,
    ]
    .into_iter()
    .find(|field| {
        enabled(*field, &app.library.draft)
            && (app.numeric_edits.contains_key(field)
                || spec(*field, &app.library.draft)
                    .is_some_and(|n| !n.valid(app.field_value(*field)))
                || !valid_choice(*field, app.field_value(*field), &app.library.draft))
    })
}

// Query standard widget geometry so validation can reveal the actual field,
// without guessed row offsets.
pub fn reveal(field: Field) -> iced::Task<Message> {
    use iced::advanced::widget::{Id, Operation, operation::Outcome};
    struct Locate {
        target: Id,
        field_y: Option<f32>,
        content_y: Option<f32>,
    }
    impl Operation<Option<f32>> for Locate {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<f32>>)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target) {
                self.field_y = Some(bounds.y);
            }
        }
        fn scrollable(
            &mut self,
            id: Option<&Id>,
            _: iced::Rectangle,
            content: iced::Rectangle,
            _: iced::Vector,
            _: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&Id::new("mastering-fields")) {
                self.content_y = Some(content.y);
            }
        }
        fn finish(&self) -> Outcome<Option<f32>> {
            Outcome::Some(
                self.field_y
                    .zip(self.content_y)
                    .map(|(y, origin)| (y - origin - 4.).max(0.)),
            )
        }
    }
    iced::advanced::widget::operate(Locate {
        target: Id::from(format!("group-{field:?}")),
        field_y: None,
        content_y: None,
    })
    .map(move |offset| Message::RevealNumber(field, offset))
}
impl App {
    pub(super) fn reveal_setting(&mut self, field: Field) -> iced::Task<Message> {
        if self.numeric_edits.contains_key(&field)
            && let Some(spec) = spec(field, &self.library.draft)
        {
            self.numeric_errors.insert(field, spec.hint());
        }
        match field {
            Field::Oversample | Field::MaxIter1 | Field::MaxIter2 => {
                self.show_limiter_details = true
            }
            Field::Evaluations | Field::Seed => self.show_optimization = true,
            _ => {}
        }
        reveal(field)
    }
    pub(super) fn reference_control(&self) -> Element<'_, Message> {
        let content = column![self.reference_controls()].spacing(3);
        let error = self
            .numeric_errors
            .get(&Field::Reference)
            .filter(|_| self.library.draft.mastering);
        let invalid = self.library.draft.mastering
            && (error.is_some()
                || self.library.draft.use_reference
                    && matches!(self.draft_reference.report, Some(Err(_))));
        group(content.into(), Field::Reference, invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stepping_at_decimal_filter_boundaries_keeps_the_pair_valid() {
        let s = Settings {
            low_cut: "0.01".into(),
            high_cut: "0.03".into(),
            ..Settings::default()
        };
        let low = spec(Field::LowCut, &s).unwrap();
        let next = low.adjust("0.01", true).unwrap();
        assert_eq!(next, "0.02");
        assert!(low.valid(&next));
        let high = spec(Field::HighCut, &s).unwrap();
        let next = high.adjust("0.03", false).unwrap();
        assert_eq!(next, "0.02");
        assert!(high.valid(&next));
    }
    #[test]
    fn inactive_saved_values_do_not_block_the_ui_run_guard() {
        let mut app = App::default();
        app.library.draft = Settings {
            format: Format::Mp3,
            mastering: false,
            reference_mode: ReferenceMode::Zero,
            ceiling_mode: CeilingMode::Peak,
            bit_depth: "bad".into(),
            intensity: "bad".into(),
            evaluations: "bad".into(),
            seed: "bad".into(),
            lowpass_cut: "bad".into(),
            loudness: "bad".into(),
            ..Settings::default()
        };
        assert!(first_invalid(&app).is_none());
        assert!(app.library.draft.parse().is_ok());
        app.library.draft.ceiling_mode = CeilingMode::LowpassTruePeak;
        assert_eq!(first_invalid(&app), Some(Field::LowpassCut));
    }
    #[test]
    fn old_invalid_dependent_values_never_panic_in_control_bounds() {
        for value in ["NaN", "-9999", "999999", "0", "invalid"] {
            let s = Settings {
                low_cut: value.into(),
                high_cut: value.into(),
                sample_rate: value.into(),
                ..Settings::default()
            };
            for field in [Field::LowCut, Field::HighCut, Field::LowpassCut] {
                let n = spec(field, &s).unwrap();
                let _ = n.adjust(value, true);
                let _ = n.adjust(value, false);
            }
        }
    }
    #[test]
    fn unsigned_steps_preserve_full_seed_precision_and_boundaries() {
        let n = Number::Unsigned {
            min: 0,
            max: u64::MAX,
        };
        assert_eq!(
            n.adjust("18446744073709551614", true).as_deref(),
            Some("18446744073709551615")
        );
        assert_eq!(n.adjust(&u64::MAX.to_string(), true), None);
        assert_eq!(n.adjust("0", false), None);
        for bad in ["-1", "0.5", "NaN", "18446744073709551616"] {
            assert!(!n.valid(bad));
        }
        assert!(n.valid(&u64::MAX.to_string()));
    }
    #[test]
    fn numeric_ranges_prevent_invalid_values_and_support_negative_editing() {
        let n = Number::Decimal {
            min: -20.,
            max: 0.,
            step: 0.1,
        };
        for partial in ["-", "-.", "."] {
            assert!(n.partial(partial));
            assert!(!n.valid(partial));
        }
        assert!(!n.partial(""));
        for bad in ["", "NaN", "inf", "banana", "-20.01", "0.01", "-1.234"] {
            assert!(!n.valid(bad));
        }
        assert!(n.valid("-1.20"));
        assert!(n.valid("-1."));
        assert_eq!(n.adjust("-0.05", true).as_deref(), Some("0.00"));
        assert_eq!(n.adjust("-20", false), None);
    }
    #[test]
    fn filter_ranges_follow_the_other_cutoff_and_zero_disables_both() {
        let mut s = Settings::default();
        assert!(!spec(Field::LowCut, &s).unwrap().valid("20000"));
        assert!(!spec(Field::HighCut, &s).unwrap().valid("20"));
        assert!(!spec(Field::HighCut, &s).unwrap().valid("0"));
        s.low_cut = "0".into();
        assert!(spec(Field::HighCut, &s).unwrap().valid("0"));
        s.high_cut = "0".into();
        assert_eq!(spec(Field::LowCut, &s).unwrap().adjust("0", true), None);
    }
    #[test]
    fn choices_follow_the_output_encoder() {
        let mut s = Settings::default();
        assert!(valid_choice(Field::SampleRate, "192000", &s));
        s.format = Format::Mp3;
        assert!(!valid_choice(Field::SampleRate, "96000", &s));
        assert!(valid_choice(Field::SampleRate, "48000", &s));
        assert!(!valid_choice(Field::Oversample, "3", &s));
        assert!(!valid_choice(Field::Oversample, "32", &s));
        for v in model::OVERSAMPLING {
            assert!(valid_choice(Field::Oversample, &v.to_string(), &s));
        }
    }
}
