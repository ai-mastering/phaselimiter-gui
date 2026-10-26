//! Help audited against backend revision 6dd5105d7d9a7072fd3a374087ade0326786139d.
//! Sources: src/phase_limiter/{main.cpp,auto_mastering5.cpp,GradCalculator.h,
//! GradCore.h,pre_compression.cpp}; GUI: backend::arguments, model::Settings::parse,
//! model::AppSettings, reference, controls::enabled. Reaudit when updating the backend pin.
//!
//! Audit map (executable branches, not flag help text):
//! - main.cpp: PhaseLimitInplace, CalculateCeilingPeak, EncodeAvoidingClipping,
//!   FFMpegOutputFormatOptions, main's filtering/mastering/gain/output branches.
//! - auto_mastering5.cpp: AutoMastering5 search bounds, calc_eval objective,
//!   reference/no-reference branches and DE-PRMM/CMA-ES dispatch.
//! - GradCalculator.h: optimizeWithProgressCallback (iter2 is cumulative).
//! - GradCore.h: frequency-dependent evalFunc/noise weights.
//! - pre_compression.cpp: PreCompress threshold and compressor configuration.
//!
//! GUI intensity also sets the classic matching flags, but mastering_mode is
//! always mastering5, so those classic-only consumers do not describe this UI.
//! External backend overrides may behave differently from the pinned backend.
use crate::{
    App, Element, Field, Message,
    model::{CeilingMode, Format, NoiseMode, ReferenceMode, Solver},
    theme,
};
use iced::{
    Fill,
    widget::{button, column, container, row, text},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Plot(crate::plots::Help),
    Measurement(crate::measurements::Help),
    Field(Field),
    ReferenceMode,
    CeilingMode,
    NoiseMode,
    Mastering,
    PreserveBass,
    PreCompression,
    Solver,
    Format,
}

impl App {
    pub(super) fn changed_group_label(
        &self,
        label: &str,
        expanded: bool,
        fields: &[&str],
    ) -> String {
        let count = self.job().map_or(0, |job| {
            crate::settings_differences(&job.config, &self.library.draft)
                .iter()
                .filter(|(label, _, _)| fields.contains(label))
                .count()
        });
        let arrow = if expanded { "▾" } else { "▸" };
        if count == 0 {
            format!("{arrow} {label}")
        } else {
            format!(
                "{arrow} {label} · {count} {}",
                if count == 1 { "change" } else { "changes" }
            )
        }
    }

    pub(super) fn previous_setting(&self, topic: Topic) -> Option<String> {
        let label = match topic {
            Topic::Measurement(_) | Topic::Plot(_) => return None,
            Topic::ReferenceMode => "Volume reference",
            Topic::CeilingMode => "Peak control",
            Topic::NoiseMode => "Noise update",
            Topic::Mastering => "Mastering",
            Topic::PreserveBass => "Preserve bass",
            Topic::PreCompression => "Pre-compression",
            Topic::Solver => "Solver",
            Topic::Format => "Format",
            Topic::Field(field) => match field {
                Field::Loudness => "Target",
                Field::Ceiling => "Ceiling",
                Field::Intensity => "Intensity",
                Field::LowpassCut => "Lowpass peak Hz",
                Field::LowCut => "Low cut",
                Field::HighCut => "High cut",
                Field::Oversample => "Oversampling",
                Field::MaxIter1 => "Outer iterations",
                Field::MaxIter2 => "Inner iterations",
                Field::Evaluations => "Evaluations",
                Field::Seed => "Seed",
                Field::Workers => "Workers",
                Field::SampleRate => "Sample rate",
                Field::BitDepth => "WAV bits",
                Field::Reference => "Reference report",
                _ => return None,
            },
        };
        let job = self.job()?;
        let differences = crate::settings_differences(&job.config, &self.library.draft);
        if topic == Topic::Field(Field::Reference) {
            return differences
                .iter()
                .any(|(label, _, _)| {
                    matches!(
                        *label,
                        "Reference report" | "Use reference audio" | "Reference audio"
                    )
                })
                .then(|| self.reference_name());
        }
        differences
            .into_iter()
            .find(|(name, _, _)| *name == label)
            .map(|(_, old, _)| match topic {
                Topic::ReferenceMode => job.config.reference_mode.to_string(),
                Topic::CeilingMode => job.config.ceiling_mode.to_string(),
                Topic::NoiseMode => job.config.noise_mode.to_string(),
                Topic::Solver => job.config.solver.to_string(),
                Topic::Format => job.config.format.to_string(),
                _ => match old.as_str() {
                    "true" => "On".into(),
                    "false" => "Off".into(),
                    _ => old,
                },
            })
    }

    pub(super) fn help_button(&self, topic: Topic) -> Element<'_, Message> {
        let selected = self.open_help == Some(topic);
        button(
            text("i")
                .size(theme::SMALL_TEXT)
                .align_x(iced::Center)
                .width(Fill),
        )
        .width(18)
        .height(18)
        .padding(0)
        .style(move |theme, status| {
            let mut style = if selected {
                theme::selected(theme, status)
            } else {
                theme::normal(theme, status)
            };
            style.border.radius = 9.into();
            style
        })
        .on_press(Message::ToggleHelp(topic))
        .into()
    }

    pub(super) fn help_label(&self, label: &'static str, topic: Topic) -> Element<'_, Message> {
        row![
            text(label).color(match topic {
                Topic::Field(field) if !crate::controls::enabled(field, &self.library.draft) =>
                    theme::DISABLED_LABEL,
                _ => theme::LABEL,
            }),
            self.help_button(topic)
        ]
        .spacing(theme::ITEM_GAP)
        .align_y(iced::Center)
        .into()
    }

    pub(super) fn help_body<'a>(
        &'a self,
        topic: Topic,
        control: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let control = if let Some(previous) = self.previous_setting(topic) {
            column![
                control,
                text(format!("• Previous: {previous}"))
                    .size(theme::SMALL_TEXT)
                    .color(theme::DISABLED_LABEL)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            ]
            .spacing(theme::ITEM_GAP)
            .into()
        } else {
            control
        };
        if self.open_help != Some(topic) {
            return control;
        }
        column![
            control,
            container(text(self.description(topic)).size(theme::SMALL_TEXT))
                .padding(8)
                .width(Fill)
                .style(|_: &theme::Theme| iced::widget::container::Style {
                    background: Some(iced::Color::from_rgb8(242, 246, 252).into()),
                    border: iced::Border {
                        color: theme::DISABLED_LABEL,
                        width: 1.,
                        radius: 3.into()
                    },
                    ..Default::default()
                })
        ]
        .spacing(theme::ITEM_GAP)
        .into()
    }

    pub(super) fn explained_toggle<'a>(
        &'a self,
        topic: Topic,
        control: Element<'a, Message>,
    ) -> Element<'a, Message> {
        self.help_body(
            topic,
            row![control, self.help_button(topic)]
                .spacing(theme::ITEM_GAP)
                .align_y(iced::Center)
                .into(),
        )
    }

    pub(super) fn explained<'a>(
        &'a self,
        label: &'static str,
        topic: Topic,
        control: Element<'a, Message>,
    ) -> Element<'a, Message> {
        self.help_body(
            topic,
            column![self.help_label(label, topic), control]
                .spacing(theme::ITEM_GAP)
                .into(),
        )
    }

    pub(super) fn description(&self, topic: Topic) -> String {
        let s = &self.library.draft;
        let body = match topic {
            Topic::Measurement(help) => help.description(),
            Topic::Plot(help) => help.description(),
            Topic::Field(field) => match field {
                Field::Loudness => {
                    "Sets the gain target for the selected level measurement before phase limiting. The final measured level can differ after limiting and encoding."
                }
                Field::Ceiling => {
                    "Sets the limiter ceiling in dBFS. When encoding clipping checks run (MP3, AAC, or an output sample rate other than 44,100 Hz), the selected peak measurement is used to reduce gain if needed."
                }
                Field::LowpassCut => {
                    "Cutoff frequency used only when measuring lowpass true peak. This is separate from the audio band limits."
                }
                Field::Oversample => {
                    "Upsamples the audio around phase limiting, then downsamples it. Higher factors increase the number of samples processed. Internal limiter oversampling stays at 1 in this app."
                }
                Field::MaxIter1 => {
                    "Limits the outer limiter iterations. In Linear mode this also sets the schedule for reducing the optimization noise parameter. Increasing the limit allows more work; it does not guarantee a better result."
                }
                Field::MaxIter2 => {
                    "Limits the cumulative step-size reductions in the limiter’s inner search across all outer iterations. This counter does not reset at each outer iteration. Reaching the limit also stops the outer loop."
                }
                Field::Intensity => {
                    "Controls the mastering5 effect search bounds and penalties for changes to the input. Higher values permit larger changes. Zero still leaves a small search range; use Automatic mastering enabled to bypass this stage. This is not a wet/dry mix."
                }
                Field::LowCut | Field::HighCut => {
                    "Filters the audio below Low cut and above High cut at several processing stages. Set both to 0 to bypass filtering. High cut alone cannot be 0 when Low cut is nonzero."
                }
                Field::SampleRate => {
                    "Sets the output encoding sample rate. The main processing runs at 44,100 Hz, with separate oversampling around the limiter; changing this does not change that base processing rate."
                }
                Field::BitDepth => {
                    "WAV sample encoding: 16 and 24 are signed integer PCM; 32 is floating-point PCM. MP3 and AAC use their own encoders and do not use this WAV setting."
                }
                Field::Workers => {
                    "Sets backend parallelism. Zero uses the runtime's default concurrency; this app accepts up to 32. This controls work inside a run, not the number of queued runs processed at once."
                }
                Field::Evaluations => {
                    "Sets the mastering5 optimizer's evaluation budget. DE-PRMM receives the budget directly; CMA-ES reserves one evaluation for the initial candidate. More evaluations allow more search, without guaranteeing an improvement."
                }
                Field::Seed => {
                    "Sets the mastering optimizer's random seed. It is passed to DE-PRMM or CMA-ES. A fixed seed does not guarantee identical results across worker counts or execution environments."
                }
                Field::Reference => {
                    "Analyzes the selected audio and passes its feature JSON to mastering5. The optimizer minimizes feature distance to that reference. Without a reference it optimizes the cached sound-quality model instead. This is separate from the loudness target."
                }
                Field::Backend => {
                    "Executable used for mastering. A blank override resolves to the bundled phase_limiter. Otherwise use an absolute path, without shell quotes."
                }
                Field::Ffmpeg => {
                    "FFmpeg used for audio decoding and encoding. Blank selects the bundled executable. Otherwise use an absolute executable path, without shell quotes."
                }
                Field::Cache => {
                    "Sound-quality model cache loaded by mastering5, including when a reference is selected. Blank resolves to the bundled cache; an absolute path overrides it."
                }
                Field::ExportDir => {
                    "Suggests the destination directory when exporting a copy. It does not change the stored mastering output or get recalled from historical runs."
                }
            },
            Topic::ReferenceMode => match s.reference_mode {
                ReferenceMode::Loudness => {
                    "Measures EBU R128 loudness and applies the difference from Target level before limiting."
                }
                ReferenceMode::YoutubeLoudness => {
                    "Uses the backend's YouTube loudness calculation: 3-second blocks, 0.1-second steps, and its configured loudness gates. Applies the difference from Target level before limiting; this does not predict a platform's playback behavior."
                }
                ReferenceMode::Rms => {
                    "Measures RMS and applies the difference from Target level before limiting."
                }
                ReferenceMode::Peak => {
                    "Measures sample peak and applies the difference from Target level before limiting."
                }
                ReferenceMode::Zero => {
                    "Adds no target-level gain at this stage. Earlier normalization and other processing still run. Target level is unused."
                }
            },
            Topic::CeilingMode => match s.ceiling_mode {
                CeilingMode::Peak => {
                    "Uses the largest absolute sample value for output ceiling checks. These checks run for MP3, AAC, or an output sample rate other than 44,100 Hz."
                }
                CeilingMode::TruePeak => {
                    "Uses interpolated peaks with the backend's true-peak oversampling for output ceiling checks. These checks run for MP3, AAC, or an output sample rate other than 44,100 Hz."
                }
                CeilingMode::LowpassTruePeak => {
                    "Uses the larger of true peak and lowpass-filtered true peak for output ceiling checks. Set the filter cutoff below. These checks run for MP3, AAC, or an output sample rate other than 44,100 Hz."
                }
            },
            Topic::NoiseMode => match s.noise_mode {
                NoiseMode::Linear => {
                    "Reduces the limiter's optimization noise parameter geometrically according to outer-iteration progress. This parameter smooths the optimization; it is not noise added to the exported audio."
                }
                NoiseMode::Adaptive => {
                    "Reduces the limiter's optimization noise parameter in response to optimization progress instead of a fixed iteration schedule. This is not an audio noise-reduction effect."
                }
            },
            Topic::Mastering => {
                "Runs the mastering5 automatic effect optimization before level adjustment and phase limiting. Turning this off skips that stage; band filtering, level adjustment, pre-compression and limiting remain available."
            }
            Topic::PreserveBass => {
                "Weights the limiter's frequency-domain error by 1 / (1 + frequency / 100), giving low-frequency changes more weight relative to high-frequency changes. This changes the optimization objective; it does not guarantee unchanged bass."
            }
            Topic::PreCompression => {
                "Runs the backend's loudness-based compressor before the final gain adjustment and phase limiting. Its threshold is derived from the measured loudness plus the backend's threshold offset; this app exposes only the on/off switch."
            }
            Topic::Solver => match s.solver {
                Solver::DePrmm => {
                    "Uses DE-PRMM differential evolution to search the mastering5 effect parameters, with the configured evaluation budget and seed."
                }
                Solver::Cmaes => {
                    "Uses CMA-ES to search the mastering5 effect parameters, with the configured evaluation budget, seed and worker count."
                }
            },
            Topic::Format => match s.format {
                Format::Wav => {
                    "Encodes WAV using the selected sample rate and PCM bit depth; 32-bit uses floating point."
                }
                Format::Mp3 => {
                    "Encodes MP3 with libmp3lame at 320 kbit/s. The backend also pre-encodes before final limiting and checks the encoded output against the ceiling."
                }
                Format::Aac => {
                    "Encodes AAC at 256 kbit/s in an ADTS stream. The backend also pre-encodes before final limiting and checks the encoded output against the ceiling."
                }
            },
        };
        let inactive = match topic {
            Topic::Field(Field::BitDepth) if s.format != Format::Wav => {
                Some("Available for WAV output only.")
            }
            Topic::Field(Field::LowpassCut) if s.ceiling_mode != CeilingMode::LowpassTruePeak => {
                Some("Available in Lowpass true-peak mode only.")
            }
            Topic::Field(Field::Loudness) if s.reference_mode == ReferenceMode::Zero => {
                Some("Unused in Zero gain mode.")
            }
            Topic::Field(
                Field::Intensity | Field::Evaluations | Field::Seed | Field::Reference,
            )
            | Topic::Solver
                if !s.mastering =>
            {
                Some("Inactive while automatic mastering is off.")
            }
            _ => None,
        };
        match inactive {
            Some(reason) => format!("{reason}\n\n{body}"),
            None => body.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_opens_switches_and_survives_setting_edits_without_changing_settings() {
        let mut app = App::default();
        let before = serde_json::to_value(&app.library.draft).unwrap();
        let topic = Topic::Field(Field::Ceiling);
        let mut ui = iced_test::simulator(app.numeric("Ceiling (dBFS)", Field::Ceiling));
        ui.click("i").unwrap();
        let messages: Vec<_> = ui.into_messages().collect();
        assert_eq!(messages.len(), 1);
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.open_help, Some(topic));
        assert_eq!(serde_json::to_value(&app.library.draft).unwrap(), before);
        let body = app.description(topic);
        let mut ui = iced_test::simulator(app.numeric("Ceiling (dBFS)", Field::Ceiling));
        assert!(ui.find(body).unwrap().visible_bounds().is_some());
        drop(ui);
        let _ = app.update(Message::Edit(Field::Ceiling, "-2".into()));
        assert_eq!(app.open_help, Some(topic));
        let _ = app.update(Message::ToggleHelp(Topic::Format));
        assert_eq!(app.open_help, Some(Topic::Format));
        let mut ui = iced_test::simulator(app.numeric("Ceiling (dBFS)", Field::Ceiling));
        assert!(ui.find(app.description(topic)).is_err());
        drop(ui);
        let _ = app.update(Message::ToggleHelp(Topic::Format));
        assert_eq!(app.open_help, None);
    }

    #[test]
    fn disabled_controls_still_offer_help_and_explain_why() {
        let app = App {
            open_help: Some(Topic::Field(Field::BitDepth)),
            ..App::default()
        };
        let mut app = app;
        app.library.draft.format = Format::Mp3;
        let mut ui =
            iced_test::simulator(app.numeric_choice("WAV bits", Field::BitDepth, &[16, 24, 32]));
        assert!(
            ui.find(app.description(Topic::Field(Field::BitDepth)))
                .unwrap()
                .visible_bounds()
                .is_some()
        );
        ui.click("i").unwrap();
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::ToggleHelp(Topic::Field(Field::BitDepth))))
        );
    }

    #[test]
    fn open_explanation_follows_selected_mode() {
        let mut app = App {
            open_help: Some(Topic::ReferenceMode),
            ..App::default()
        };
        app.library.draft.reference_mode = ReferenceMode::Loudness;
        let previous = app.description(Topic::ReferenceMode);
        let _ = app.update(Message::ReferenceMode(ReferenceMode::Zero));
        assert_eq!(app.open_help, Some(Topic::ReferenceMode));
        let current = app.description(Topic::ReferenceMode);
        assert_ne!(previous, current);
        let mut ui = iced_test::simulator(app.explained(
            "Level measurement",
            Topic::ReferenceMode,
            text("selection").into(),
        ));
        assert!(ui.find(current).unwrap().visible_bounds().is_some());
        assert!(ui.find(previous).is_err());
    }
}
