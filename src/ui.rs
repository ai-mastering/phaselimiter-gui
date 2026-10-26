use super::*;
use crate::descriptions::Topic;
use iced::widget::{column, responsive};

const SETTINGS_WIDTH: f32 = 330.;
const HISTORY_WIDTH: f32 = 300.;

impl App {
    fn plot_heading(&self, help: plots::Help) -> Element<'_, Message> {
        let topic = Topic::Plot(help);
        self.help_body(topic, self.help_label(help.label(), topic))
    }

    fn measurement_label(
        &self,
        label: &'static str,
        help: measurements::Help,
    ) -> Element<'_, Message> {
        row![
            text(label)
                .size(theme::SMALL_TEXT)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(Fill),
            self.help_button(Topic::Measurement(help)),
        ]
        .spacing(theme::ITEM_GAP)
        .align_y(iced::Center)
        .width(172)
        .into()
    }

    pub(super) fn view(&self) -> Element<'_, Message> {
        // Only menu popup placement needs the viewport dimensions.
        let content: Element<'_, Message> =
            responsive(|size| self.menu_layer(self.workspace_view(), size)).into();
        let modal = self.preferences.is_some();
        let content: Element<'_, Message> = if modal {
            let shield = iced::widget::mouse_area(
                container(iced::widget::space().width(Fill).height(Fill)).style(|_: &Theme| {
                    container::Style {
                        background: Some(iced::Color::from_rgba(0., 0., 0., 0.06).into()),
                        ..Default::default()
                    }
                }),
            )
            .on_press(Message::Preferences(preferences::Event::Focus))
            .on_right_press(Message::Preferences(preferences::Event::Focus))
            .on_scroll(|_| Message::Preferences(preferences::Event::Focus));
            iced::widget::stack![content, shield].into()
        } else {
            iced::widget::stack![content].into()
        };
        content.map(move |message| {
            if modal {
                Message::Preferences(preferences::Event::Focus)
            } else {
                message
            }
        })
    }

    fn workspace_view(&self) -> Element<'_, Message> {
        let mut body = column![self.menu_bar()].spacing(theme::SECTION_GAP);
        if self.show_audio_path {
            body = body.push(
                row![
                    text_input("Paste an audio file path", &self.input)
                        .on_input(Message::Input)
                        .on_submit(Message::AddInput),
                    button("Master")
                        .on_press_maybe((!self.input.is_empty()).then_some(Message::AddInput))
                        .style(theme::normal),
                ]
                .spacing(theme::SECTION_GAP),
            );
        }
        let mut workspace = row![self.history_view(), rule::vertical(1)]
            .spacing(theme::SECTION_GAP)
            .height(Fill);
        let mut center = column![]
            .spacing(theme::SECTION_GAP)
            .width(Fill)
            .height(Fill);
        if let Some(track) = self.track() {
            let mut title = row![
                column![
                    compact_text(
                        track.name.clone(),
                        track.name.clone(),
                        theme::HEADING_TEXT,
                        true
                    ),
                    text(
                        self.job()
                            .map(|job| format!(
                                "{} · {}",
                                self.library.job_time_label(job),
                                job.status
                            ))
                            .unwrap_or_default()
                    )
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                ]
                .spacing(theme::ITEM_GAP)
                .width(Fill)
            ]
            .spacing(theme::SECTION_GAP)
            .align_y(iced::Center);
            if self.job().is_some_and(|j| j.status == Status::Succeeded) {
                title = title.push(
                    button("Export")
                        .on_press(Message::Export)
                        .style(theme::normal),
                );
            }
            center = center.push(title);
            if let Some(job) = self.job() {
                center = center.push(
                    text_input("Notes", &job.memo)
                        .on_input(move |memo| Message::EditMemo(job.id, memo))
                        .on_submit(Message::SaveMemo),
                );
            }
            if self.job().is_some() {
                let choices = self.comparison_choices();
                let selected_id = self.comparison_job().map(|j| j.id);
                let selected_full = self.comparison_job().map_or_else(
                    || "Original".into(),
                    |job| format!("{}\n{}", self.library.job_time_label(job), job.memo),
                );
                center = center.push(
                    row![
                        text("Source A").size(theme::SMALL_TEXT),
                        responsive(move |bounds| {
                            let mut visible = choices.clone();
                            for choice in &mut visible {
                                choice.label = abbreviate(
                                    &choice.label,
                                    (bounds.width - 44.).max(0.),
                                    false,
                                    |s| {
                                        text_width(s, iced::Settings::default().default_text_size.0)
                                    },
                                );
                            }
                            let selected = visible.iter().find(|c| c.job == selected_id).cloned();
                            let original = choices.iter().find(|c| c.job == selected_id);
                            let show_tooltip = needs_tooltip(
                                selected.as_ref().map_or("", |c| c.label.as_str()),
                                original.map_or("", |c| c.label.as_str()),
                                &selected_full,
                            );
                            let control =
                                pick_list(visible, selected, Message::Compare).width(Fill);
                            if !show_tooltip {
                                return control.into();
                            }
                            iced::widget::tooltip(
                                control,
                                container(
                                    text(selected_full.clone())
                                        .size(theme::SMALL_TEXT)
                                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                                        .width(Fill),
                                )
                                .padding(8)
                                .width(320.),
                                iced::widget::tooltip::Position::Bottom,
                            )
                            .style(theme::tooltip)
                            .snap_within_viewport(true)
                            .into()
                        })
                        .height(30.),
                    ]
                    .spacing(theme::SECTION_GAP)
                    .align_y(iced::Center),
                );
            }

            center = center.push(self.transport_view());
        }
        center = center.push(
            scrollable(container(self.analysis_view()).padding(iced::Padding {
                bottom: 10.,
                ..iced::Padding::ZERO
            }))
            .spacing(theme::ITEM_GAP)
            .height(Fill)
            .width(Fill),
        );
        workspace = workspace
            .push(container(center).clip(true).width(Fill).height(Fill))
            .push(rule::vertical(1))
            .push(
                container(self.settings_panel())
                    .padding(iced::Padding {
                        bottom: 10.,
                        ..iced::Padding::ZERO
                    })
                    .clip(true)
                    .width(SETTINGS_WIDTH)
                    .height(Fill),
            );
        body = body.push(workspace);
        column![
            container(body)
                .padding(iced::Padding {
                    bottom: 0.,
                    ..iced::Padding::new(10.)
                })
                .height(Fill)
                .width(Fill),
            self.status_bar(),
        ]
        .height(Fill)
        .width(Fill)
        .into()
    }

    fn history_view(&self) -> Element<'_, Message> {
        let keys = self.history_keys();
        let mut history = column![].spacing(0);
        for &key in &keys {
            let Some(track) = self.library.tracks.iter().find(|track| track.id == key.0) else {
                continue;
            };
            let Some(job) = self.library.jobs.iter().find(|job| job.id == key.1) else {
                continue;
            };
            let label = self.library.job_label(job);
            let detail = job.memo.clone();
            let failed = matches!(job.status, Status::Failed | Status::Interrupted);
            let label = if failed { format!("! {label}") } else { label };
            let cells = row![
                container(compact_text(label, detail, theme::SMALL_TEXT, false)).width(132),
                container(compact_text(
                    track.name.clone(),
                    track.name.clone(),
                    theme::SMALL_TEXT,
                    true
                ))
                .width(Fill),
            ]
            .spacing(theme::SECTION_GAP)
            .align_y(iced::Center);
            let selected = self.selected == Some(key);
            let row = button(cells)
                .on_press(Message::Select(key))
                .width(Fill)
                .height(32)
                .padding([6, 5])
                .style(move |theme, status| theme::history_row(theme, status, selected, failed));
            let row: Element<'_, Message> = if job.status == Status::Processing {
                iced::widget::stack![
                    row,
                    container(progress_bar(0.0..=1.0, job.progress).girth(2))
                        .height(Fill)
                        .align_bottom(Fill)
                ]
                .into()
            } else {
                row.into()
            };
            history = history.push(
                iced::widget::mouse_area(
                    container(row).id(format!("history-{}-{:?}", key.0, key.1)),
                )
                .on_right_press(Message::Menu(menu::Event::Context(key))),
            );
        }
        let list: Element<'_, Message> = scrollable(history)
            .id("history-list")
            .on_scroll(|viewport| Message::HistoryScroll(viewport.absolute_offset().y))
            .spacing(theme::ITEM_GAP)
            .height(Fill)
            .into();
        let list = crate::history::HistoryInput {
            content: list,
            keys,
            selected: self.selected,
            offset: self.history_offset,
        };
        let pane = column![
            text("Audio & history").size(theme::HEADING_TEXT),
            button(
                row![
                    text("Master audio…"),
                    iced::widget::space().width(Fill),
                    text("or drop audio").size(theme::SMALL_TEXT),
                ]
                .spacing(theme::SECTION_GAP)
                .align_y(iced::Center),
            )
            .on_press(Message::BrowseAudio)
            .width(Fill),
            container(
                row![
                    text("Time").size(theme::SMALL_TEXT).width(132),
                    text("Input audio").size(theme::SMALL_TEXT).width(Fill)
                ]
                .spacing(theme::SECTION_GAP)
            )
            .padding([0, 5]),
            rule::horizontal(1),
            Element::from(list),
        ]
        .spacing(theme::SECTION_GAP);
        container(pane)
            .clip(true)
            .width(HISTORY_WIDTH)
            .height(Fill)
            .into()
    }

    fn transport_view(&self) -> Element<'_, Message> {
        let playing = self.player.status().2;
        let side_button =
            |output: bool, label: String, notes: String, enabled: bool| -> Element<'_, Message> {
                let selected = self.output_side == output;
                let active = playing && self.active_key == self.side_key(output);
                let indicator = if selected && self.loading {
                    "…"
                } else if active {
                    "Ⅱ"
                } else {
                    "▶"
                };
                container(
                    button(
                        row![
                            text(indicator).size(theme::SMALL_TEXT),
                            text(if output { "B" } else { "A" }),
                            compact_text(
                                label,
                                notes,
                                iced::Settings::default().default_text_size.0,
                                false
                            ),
                        ]
                        .spacing(theme::ITEM_GAP)
                        .align_y(iced::Center),
                    )
                    .width(Fill)
                    .style(if active || !playing && selected {
                        theme::selected
                    } else {
                        theme::normal
                    })
                    .on_press_maybe(enabled.then_some(Message::Side(output))),
                )
                .id(if output { "play-b" } else { "play-a" })
                .width(Fill)
                .into()
            };
        let mut controls = row![side_button(
            false,
            self.comparison_label(),
            self.comparison_job()
                .map_or_else(|| "Original".into(), |j| j.memo.clone()),
            true,
        )]
        .spacing(theme::SECTION_GAP)
        .align_y(iced::Center);
        if let Some(job) = self.job() {
            controls = controls.push(side_button(
                true,
                self.library.job_label(job),
                job.memo.clone(),
                job.status == Status::Succeeded,
            ));
        }
        controls = controls.push(
            checkbox(self.equalize)
                .label("Match loudness")
                .on_toggle(Message::Equalize),
        );
        let mut transport = column![controls, self.plot_heading(plots::Help::Waveform)];
        transport = transport.push(
            canvas(plots::Plot {
                input: self
                    .listening_analysis()
                    .map_or_else(analysis::empty_series, analysis::Analysis::waveform),
                output: analysis::empty_series(),
                reference: analysis::empty_series(),
                waveform: true,
                logarithmic: false,
                x_decibels: false,
                position: (self.position / self.duration.max(0.001)) as f32,
                region: self.region.map(|(a, b)| {
                    (
                        (a / self.duration.max(0.001)) as f32,
                        (b / self.duration.max(0.001)) as f32,
                    )
                }),
            })
            .width(Fill)
            .height(65),
        );
        transport = transport
            .push(
                row![
                    slider(
                        0.0..=self.duration.max(0.001),
                        self.position.min(self.duration),
                        Message::Seek
                    )
                    .step(0.05),
                    text(format!(
                        "{} / {}",
                        clock(self.position),
                        clock(self.duration)
                    ))
                    .size(theme::SMALL_TEXT),
                ]
                .spacing(theme::SECTION_GAP)
                .align_y(iced::Center),
            )
            .spacing(theme::ITEM_GAP);
        if let Some((a, b)) = self.region {
            transport = transport.push(
                text(format!("{}–{}", clock(a), clock(b)))
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );
        }
        transport.into()
    }

    fn analysis_view(&self) -> Element<'_, Message> {
        let Some(track) = self.track() else {
            return column![
                text("Master audio to create a result")
                    .size(theme::HEADING_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                text("History switches audio at the same playback position.")
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            ]
            .spacing(theme::SECTION_GAP)
            .into();
        };
        let job = self.job();
        let baseline = self.comparison_job();
        let reference = self.reference_analysis();
        let has_reference = job.is_some_and(|j| j.config.reference.is_some());
        let mut detail = column![].spacing(theme::SECTION_GAP).width(Fill);
        let numeric_width = 99.;
        let mut heading = row![
            text("Measurement")
                .size(theme::SMALL_TEXT)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(172),
            text("Original")
                .size(theme::SMALL_TEXT)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(numeric_width)
        ]
        .spacing(theme::SECTION_GAP);
        if let Some(baseline) = baseline {
            heading = heading.push(
                text(format!("A\n{}", self.library.job_time_label(baseline)))
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(numeric_width),
            );
        }
        if let Some(job) = job {
            heading = heading.push(
                text(format!("B\n{}", self.library.job_time_label(job)))
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(numeric_width),
            );
        }
        if has_reference {
            heading = heading.push(
                text("Reference")
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(numeric_width),
            );
        }
        let mut table = column![heading, rule::horizontal(1)].spacing(theme::ITEM_GAP);
        for metric in measurements::ALL {
            let mut values = row![
                self.measurement_label(metric.label, metric.help),
                numeric_cell(metric.format(track.analysis.as_ref()), numeric_width)
            ]
            .spacing(theme::SECTION_GAP);
            if let Some(baseline) = baseline {
                values = values.push(numeric_cell(
                    metric.format(baseline.analysis.as_ref()),
                    numeric_width,
                ));
            }
            if let Some(job) = job {
                values = values.push(numeric_cell(
                    metric.format(job.analysis.as_ref()),
                    numeric_width,
                ));
            }
            if has_reference {
                values = values.push(numeric_cell(metric.format(reference), numeric_width));
            }
            table = table.push(self.help_body(Topic::Measurement(metric.help), values.into()));
        }
        if let Some(job) = job {
            let mut values = row![
                self.measurement_label("Limiting error", measurements::Help::LimitingError),
                numeric_cell("—".into(), numeric_width)
            ]
            .spacing(theme::SECTION_GAP);
            if let Some(baseline) = baseline {
                values = values.push(numeric_cell(
                    error_value(baseline.limiting_error),
                    numeric_width,
                ));
            }
            values = values.push(numeric_cell(error_value(job.limiting_error), numeric_width));
            if has_reference {
                values = values.push(numeric_cell("—".into(), numeric_width));
            }
            table = table.push(self.help_body(
                Topic::Measurement(measurements::Help::LimitingError),
                values.into(),
            ));
        }
        detail = detail.push(
            scrollable(table)
                .horizontal()
                .spacing(theme::ITEM_GAP)
                .width(Fill),
        );
        if track.analysis.as_ref().is_some_and(|a| {
            a.number("sound_quality").is_none() && a.sound_quality_error().is_none()
        }) {
            detail = detail.push(
                text("Reanalyze to calculate missing Professionality scores.")
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );
        }
        if has_reference {
            let name = self.reference_name();
            detail = detail.push(compact_text(
                format!("Reference: {name}"),
                String::new(),
                theme::SMALL_TEXT,
                true,
            ));
        }
        for kind in plots::Kind::ALL {
            detail = detail
                .push(rule::horizontal(1))
                .push(self.plot_heading(plots::Help::Chart(kind)));
            if kind == plots::Kind::Distribution {
                for (label, analysis) in [
                    (self.comparison_label(), self.comparison_analysis()),
                    ("Selected".into(), job.and_then(|j| j.analysis.as_ref())),
                    ("Reference".into(), reference),
                ] {
                    if let Some(path) = analysis
                        .and_then(|a| a.path())
                        .map(|p| p.with_extension("frequency.png"))
                        .filter(|p| p.is_file())
                    {
                        detail = detail
                            .push(compact_text(label.clone(), label, theme::SMALL_TEXT, false))
                            .push(iced::widget::image(path).width(Fill));
                    }
                }
            } else {
                let series = |a: Option<&analysis::Analysis>| {
                    a.map_or_else(analysis::empty_series, |a| match kind {
                        plots::Kind::Spectrum => a.series("spectrum", "hz", "db"),
                        plots::Kind::Loudness => a.series("loudness_time_series", "sec", "db"),
                        plots::Kind::Histogram => a.series("histogram", "db", "frequency"),
                        plots::Kind::Distribution => analysis::empty_series(),
                    })
                };
                detail = detail
                    .push(
                        text(format!(
                            "Gray: {} · Blue: selected{}",
                            self.comparison_label(),
                            if has_reference {
                                " · Orange: reference"
                            } else {
                                ""
                            }
                        ))
                        .size(theme::SMALL_TEXT)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    )
                    .push(
                        canvas(plots::Plot {
                            input: series(self.comparison_analysis()),
                            output: series(job.and_then(|j| j.analysis.as_ref())),
                            reference: series(reference),
                            waveform: false,
                            logarithmic: kind == plots::Kind::Spectrum,
                            x_decibels: kind == plots::Kind::Histogram,
                            position: 0.,
                            region: None,
                        })
                        .width(Fill)
                        .height(220),
                    )
                    .push(
                        text(match kind {
                            plots::Kind::Spectrum => "Frequency (Hz, log) / level (dB)",
                            plots::Kind::Loudness => "Time (s) / loudness (dB)",
                            _ => "Loudness (dB) / frequency",
                        })
                        .size(theme::SMALL_TEXT)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    );
            }
        }
        detail = detail.push(
            button("Reanalyze")
                .on_press(Message::Analyze)
                .style(theme::normal),
        );
        if let Some(job) = job {
            detail = detail
                .push(rule::horizontal(1))
                .push(text("Processing details"))
                .push(self.processing_details(job));
        }
        detail = detail.push(
            column![
                text("Source path").size(theme::SMALL_TEXT),
                crate::text_fields::read_only_input(
                    "source-path",
                    &track.source.display().to_string()
                ),
            ]
            .spacing(theme::ITEM_GAP),
        );
        detail.into()
    }

    fn processing_details<'a>(&'a self, job: &'a Job) -> Element<'a, Message> {
        let config = &job.config;
        let level_unit = match config.reference_mode {
            ReferenceMode::Loudness | ReferenceMode::YoutubeLoudness => "LUFS",
            _ => "dB",
        };
        let enabled = |value| if value { "On" } else { "Off" }.to_owned();
        let values = [
            ("Status", job.status.to_string()),
            ("Mastering", enabled(config.mastering)),
            ("Target level", format!("{} {level_unit}", config.loudness)),
            ("Intensity", config.intensity.to_string()),
            ("Ceiling", format!("{} dB", config.ceiling)),
            ("Reference mode", config.reference_mode.to_string()),
            ("Peak mode", config.ceiling_mode.to_string()),
            ("Lowpass frequency", format!("{} Hz", config.lowpass_cut)),
            ("Low cut", format!("{} Hz", config.low_cut)),
            ("High cut", format!("{} Hz", config.high_cut)),
            ("Oversampling", config.oversample.to_string()),
            ("Iterations (stage 1)", config.max_iter1.to_string()),
            ("Iterations (stage 2)", config.max_iter2.to_string()),
            ("Noise mode", config.noise_mode.to_string()),
            ("Solver", config.solver.to_string()),
            ("Seed", config.seed.to_string()),
            ("Evaluations", config.evaluations.to_string()),
            ("Workers", config.workers.to_string()),
            ("Preserve bass", enabled(config.preserve_bass)),
            ("Pre-compression", enabled(config.pre_compression)),
            ("Sample rate", format!("{} Hz", config.sample_rate)),
            ("Format", config.format.to_string()),
            ("Bit depth", format!("{} bit", config.bit_depth)),
        ];
        let value_width = values
            .iter()
            .map(|(_, value)| {
                number_and_unit(value).map_or_else(
                    || text_width(value, 12.) + UNIT_WIDTH + UNIT_GAP,
                    |(number, _)| number.chars().count() as f32 * 8. + UNIT_WIDTH + UNIT_GAP,
                )
            })
            .fold(99., f32::max)
            .ceil();
        let mut settings = column![
            row![
                text("Setting").size(theme::SMALL_TEXT).width(172),
                text("Value")
                    .size(theme::SMALL_TEXT)
                    .width(value_width - UNIT_WIDTH - UNIT_GAP)
                    .align_x(iced::Right),
            ]
            .spacing(theme::SECTION_GAP),
            rule::horizontal(1),
        ]
        .spacing(theme::ITEM_GAP)
        .width(172. + 6. + value_width);
        for (label, value) in values {
            settings = settings.push(
                row![
                    text(label)
                        .size(theme::SMALL_TEXT)
                        .width(172)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    numeric_cell(value, value_width),
                ]
                .spacing(theme::SECTION_GAP),
            );
        }
        let mut details = column![
            scrollable(settings)
                .horizontal()
                .spacing(theme::ITEM_GAP)
                .width(Fill)
        ]
        .spacing(theme::SECTION_GAP);
        let image = job.output.with_extension("limiting-error.png");
        if image.is_file() {
            details = details
                .push(self.plot_heading(plots::Help::LimitingError))
                .push(iced::widget::image(image).width(Fill));
        }
        if let Some(command) = &job.command {
            details =
                details
                    .push(text("Executed command"))
                    .push(crate::text_fields::read_only_input(
                        "executed-command",
                        command,
                    ));
        }
        details
            .push(text("Backend log"))
            .push(crate::text_fields::read_only_editor(
                "backend-log-scroll",
                &self.log_text,
                160,
                Message::LogText,
            ))
            .into()
    }

    fn settings_panel(&self) -> Element<'_, Message> {
        let target = self.selected.map(|k| k.0);
        let track = self.library.tracks.iter().find(|t| Some(t.id) == target);
        let mut fields = column![].spacing(theme::SECTION_GAP);
        fields = fields.push(self.mastering_settings_view());
        let panel = column![
            text("Mastering settings").size(theme::HEADING_TEXT),
            scrollable(fields.padding(iced::Padding {
                bottom: 10.,
                ..iced::Padding::ZERO
            }))
            .id("mastering-fields")
            .spacing(theme::ITEM_GAP)
            .height(Fill)
            .width(Fill),
        ]
        .spacing(theme::SECTION_GAP)
        .height(Fill);
        let issue = self.settings_issue().or_else(|| {
            if track.is_none() {
                Some((None, "Master new audio or select a result.".into()))
            } else if self.root.is_none() {
                Some((None, "Data directory unavailable.".into()))
            } else {
                None
            }
        });
        panel
            .push(
                button(text(self.variation_label()))
                    .on_press_maybe(issue.is_none().then_some(Message::Master))
                    .width(Fill),
            )
            .into()
    }

    pub(super) fn variation_label(&self) -> String {
        let count = self.job().map_or(0, |job| {
            settings_differences(&job.config, &self.library.draft).len()
        });
        let mut label = "Create variation".to_owned();
        if count > 0 {
            label.push_str(&format!(
                " · {count} {}",
                if count == 1 { "change" } else { "changes" }
            ));
        }
        if self.active.is_some() {
            label.push_str(" · queue next");
        }
        label
    }

    fn mastering_settings_view(&self) -> Element<'_, Message> {
        let target_label = match self.library.draft.reference_mode {
            ReferenceMode::Loudness | ReferenceMode::YoutubeLoudness => "Target level (LUFS)",
            _ => "Target level (dBFS)",
        };
        let loudness = column![
            text("Loudness").wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            self.explained(
                "Level measurement",
                Topic::ReferenceMode,
                (pick_list(
                    ReferenceMode::ALL,
                    Some(self.library.draft.reference_mode),
                    Message::ReferenceMode
                )
                .width(Fill))
                .into()
            ),
            self.numeric(target_label, Field::Loudness),
        ]
        .spacing(theme::SECTION_GAP)
        .width(Fill);
        let peak = column![
            text("Peak").wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            self.explained(
                "Peak measurement",
                Topic::CeilingMode,
                (pick_list(
                    CeilingMode::ALL,
                    Some(self.library.draft.ceiling_mode),
                    Message::CeilingMode
                )
                .width(Fill))
                .into()
            ),
            self.numeric("Ceiling (dBFS)", Field::Ceiling),
            self.numeric("Lowpass true-peak frequency (Hz)", Field::LowpassCut),
        ]
        .spacing(theme::SECTION_GAP)
        .width(Fill);
        let primary = column![loudness, rule::horizontal(1), peak].spacing(theme::SECTION_GAP);
        let mut settings = column![
            text("Level & limiter")
                .size(theme::HEADING_TEXT)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            primary,
            button(text(self.changed_group_label(
                "Limiter details",
                self.show_limiter_details,
                &[
                    "Preserve bass",
                    "Oversampling",
                    "Noise update",
                    "Outer iterations",
                    "Inner iterations",
                    "Pre-compression"
                ]
            )))
            .on_press(Message::ToggleLimiterDetails)
            .style(theme::normal),
        ]
        .spacing(theme::SECTION_GAP);
        if self.show_limiter_details {
            settings = settings.push(
                column![
                    self.explained_toggle(
                        Topic::PreserveBass,
                        (checkbox(self.library.draft.preserve_bass)
                            .label("Preserve bass")
                            .on_toggle(Message::PreserveBass))
                        .into()
                    ),
                    self.numeric_choice("Oversampling", Field::Oversample, &model::OVERSAMPLING),
                    self.explained(
                        "Noise update",
                        Topic::NoiseMode,
                        (pick_list(
                            NoiseMode::ALL,
                            Some(self.library.draft.noise_mode),
                            Message::NoiseMode
                        )
                        .width(Fill))
                        .into()
                    ),
                    row![
                        self.numeric("Outer iterations", Field::MaxIter1),
                        self.numeric("Inner iterations", Field::MaxIter2)
                    ]
                    .spacing(theme::SECTION_GAP),
                    rule::horizontal(1),
                    text("Pre-compression").wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    self.explained_toggle(
                        Topic::PreCompression,
                        (checkbox(self.library.draft.pre_compression)
                            .label("Pre-compression enabled")
                            .on_toggle(Message::PreCompression))
                        .into()
                    ),
                ]
                .spacing(theme::SECTION_GAP),
            );
        }
        settings = settings
            .push(rule::horizontal(1))
            .push(
                text("Automatic mastering")
                    .size(theme::HEADING_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .push(
                self.explained_toggle(
                    Topic::Mastering,
                    (checkbox(self.library.draft.mastering)
                        .label("Automatic mastering enabled")
                        .on_toggle(Message::Mastering))
                    .into(),
                ),
            )
            .push(self.intensity_control())
            .push(self.help_label("Reference", Topic::Field(Field::Reference)))
            .push(
                checkbox(self.library.draft.use_reference)
                    .label("Use reference audio")
                    .on_toggle_maybe(
                        self.library
                            .draft
                            .mastering
                            .then_some(Message::ReferenceEnabled),
                    ),
            )
            .push(self.help_body(Topic::Field(Field::Reference), self.reference_control()))
            .push(
                button(text(self.changed_group_label(
                    "Optimization",
                    self.show_optimization,
                    &["Solver", "Evaluations", "Seed"],
                )))
                .on_press(Message::ToggleOptimization)
                .style(theme::normal),
            );
        if self.show_optimization {
            settings = settings.push(
                column![
                    self.explained(
                        "Optimizer",
                        Topic::Solver,
                        controls::choice(
                            Solver::ALL.to_vec(),
                            Some(self.library.draft.solver),
                            Message::Solver,
                            self.library.draft.mastering
                        )
                    ),
                    row![
                        self.numeric("Evaluations", Field::Evaluations),
                        self.numeric("Seed", Field::Seed)
                    ]
                    .spacing(theme::SECTION_GAP),
                ]
                .spacing(theme::SECTION_GAP),
            );
        }
        settings
            .push(rule::horizontal(1))
            .push(
                text("Band limits")
                    .size(theme::HEADING_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .push(
                row![
                    self.numeric("Low cut (Hz)", Field::LowCut),
                    self.numeric("High cut (Hz)", Field::HighCut)
                ]
                .spacing(theme::SECTION_GAP),
            )
            .push(rule::horizontal(1))
            .push(
                text("Output")
                    .size(theme::HEADING_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .push(
                self.explained(
                    "File format",
                    Topic::Format,
                    (pick_list(
                        Format::ALL,
                        Some(self.library.draft.format),
                        Message::Format,
                    )
                    .width(Fill))
                    .into(),
                ),
            )
            .push(
                row![
                    self.numeric_choice(
                        "Sample rate (Hz)",
                        Field::SampleRate,
                        model::sample_rates(self.library.draft.format)
                    ),
                    self.numeric_choice("WAV bits (32: float)", Field::BitDepth, &[16, 24, 32]),
                ]
                .spacing(theme::SECTION_GAP),
            )
            .push(rule::horizontal(1))
            .push(
                text("Execution")
                    .size(theme::HEADING_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .push(self.numeric("Workers (0: auto, max: 32)", Field::Workers))
            .into()
    }
}

const UNIT_WIDTH: f32 = 36.;
const UNIT_GAP: f32 = 4.;

fn number_and_unit(value: &str) -> Option<(&str, &str)> {
    let (number, unit) = value.split_once(' ').unwrap_or((value, ""));
    (number == "—" || number.parse::<f64>().is_ok()).then_some((number, unit))
}

fn numeric_cell<'a>(value: String, width: f32) -> Element<'a, Message> {
    let numeric = number_and_unit(&value);
    let (number, unit) = numeric.unwrap_or((&value, ""));
    row![
        text(number.to_owned())
            .size(theme::SMALL_TEXT)
            .font(if numeric.is_some() {
                iced::Font::MONOSPACE
            } else {
                iced::Font::default()
            })
            .width(width - UNIT_WIDTH - UNIT_GAP)
            .align_x(iced::Right),
        text(unit.to_owned())
            .size(theme::SMALL_TEXT)
            .width(UNIT_WIDTH),
    ]
    .spacing(UNIT_GAP)
    .width(width)
    .into()
}
fn error_value(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "—".into(), |v| format!("{v:.3} dB"))
}

/// Measure the same shaped text we render; character counts do not predict widths.
fn text_width(value: &str, size: f32) -> f32 {
    use iced::advanced::text::{Paragraph, Renderer};
    <iced::Renderer as Renderer>::Paragraph::with_text(iced::advanced::Text {
        content: value,
        bounds: iced::Size::INFINITE,
        size: size.into(),
        line_height: Default::default(),
        font: iced::Font::default(),
        align_x: Default::default(),
        align_y: iced::alignment::Vertical::Top,
        shaping: iced::widget::text::Shaping::Advanced,
        wrapping: iced::widget::text::Wrapping::None,
    })
    .min_width()
}

fn abbreviate(value: &str, width: f32, middle: bool, measure: impl Fn(&str) -> f32) -> String {
    if measure(value) <= width {
        return value.into();
    }
    if measure("…") > width {
        return String::new();
    }
    let chars: Vec<char> = value.chars().collect();
    let candidate = |keep: usize| {
        let tail = if middle { keep / 2 } else { 0 };
        let head = keep - tail;
        format!(
            "{}…{}",
            chars[..head].iter().collect::<String>(),
            chars[chars.len() - tail..].iter().collect::<String>()
        )
    };
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if measure(&candidate(mid)) <= width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    candidate(low)
}

/// A tooltip is useful only when truncation or supplemental information hides content.
fn needs_tooltip(displayed: &str, value: &str, full: &str) -> bool {
    let normalize = |s: &str| {
        s.split(|c: char| c.is_whitespace() || c == '·')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    displayed != value || !normalize(value).contains(&normalize(full))
}

fn compact_text<'a>(value: String, full: String, size: f32, middle: bool) -> Element<'a, Message> {
    responsive(move |bounds| {
        let label = abbreviate(&value, bounds.width, middle, |s| text_width(s, size));
        let show_tooltip = needs_tooltip(&label, &value, &full);
        let content = container(
            text(label)
                .size(size)
                .shaping(iced::widget::text::Shaping::Advanced)
                .wrapping(iced::widget::text::Wrapping::None)
                .width(Fill),
        )
        .clip(true)
        .width(Fill);
        if !show_tooltip {
            return content.into();
        }
        iced::widget::tooltip(
            content,
            container(
                scrollable(
                    text(if full.is_empty() || full == value {
                        value.clone()
                    } else {
                        format!("{value}\n{full}")
                    })
                    .size(theme::SMALL_TEXT)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(Fill),
                )
                .height(iced::Length::Shrink),
            )
            .padding(8)
            .width(iced::Length::Fixed(320.)),
            iced::widget::tooltip::Position::Bottom,
        )
        .style(theme::tooltip)
        .snap_within_viewport(true)
        .into()
    })
    .height(
        iced::widget::text::LineHeight::default()
            .to_absolute(size.into())
            .0,
    )
    .into()
}

#[cfg(test)]
mod long_name_tests {
    use super::*;
    fn measure(s: &str) -> f32 {
        s.chars().count() as f32
    }
    #[test]
    fn numeric_columns_keep_units_separate_and_align_missing_values() {
        let content = iced::widget::column![
            numeric_cell("-12.3 LUFS".into(), 99.),
            numeric_cell("82 %".into(), 99.),
            numeric_cell("0.25".into(), 99.),
            numeric_cell("—".into(), 99.),
        ];
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(300., 200.),
            content,
        );
        let number_bounds = ui.find("-12.3").unwrap().bounds();
        for value in ["82", "0.25", "—"] {
            let bounds = ui.find(value).unwrap().bounds();
            assert_eq!(
                bounds.x + bounds.width,
                number_bounds.x + number_bounds.width
            );
        }
        let unit_bounds = ui.find("LUFS").unwrap().bounds();
        assert_eq!(ui.find("%").unwrap().bounds().x, unit_bounds.x);
        assert!(unit_bounds.x > number_bounds.x + number_bounds.width);
        assert_eq!(number_and_unit("Lowpass true peak"), None);
    }

    #[test]
    fn hover_only_adds_hidden_information() {
        assert!(!needs_tooltip("Original", "Original", "Original"));
        assert!(!needs_tooltip(
            "10/26 12:34:56 · piano eq",
            "10/26 12:34:56 · piano eq",
            "10/26 12:34:56\npiano  eq"
        ));
        assert!(needs_tooltip("10/26…", "10/26 12:34:56", ""));
        assert!(!needs_tooltip("mix.wav", "mix.wav", "mix.wav"));
        assert!(needs_tooltip(
            "long…wav",
            "long_filename.wav",
            "long_filename.wav"
        ));
        assert!(needs_tooltip(
            "time · memo…",
            "time · memo…",
            "memo with additional text"
        ));
    }

    #[test]
    fn filenames_keep_both_ends_and_extension() {
        let name = "mymusic_とても長い名前_realfinal.wav";
        let short = abbreviate(name, 20., true, measure);
        assert!(short.starts_with("mymusic"));
        assert!(short.ends_with("final.wav"));
        assert!(short.contains('…'));
        assert!(measure(&short) <= 20.);
        assert_eq!(abbreviate(name, 200., true, measure), name);
    }
    #[test]
    fn measured_long_names_fit_at_history_and_center_widths() {
        for name in [
            format!("{}.wav", "a".repeat(240)),
            format!("20261026_mymusic_{}_realfinal.wav", "piano_eq_".repeat(8)),
            format!("曲名_{}_最終版.wav", "ドラム調整".repeat(20)),
        ] {
            for width in [180., 215., 450.] {
                let short = abbreviate(&name, width, true, |s| text_width(s, 14.));
                assert!(text_width(&short, 14.) <= width);
                assert!(short.ends_with(".wav"));
                assert!(short.contains('…'));
            }
        }
    }

    #[test]
    fn labels_keep_the_time_and_handle_tiny_widths() {
        assert_eq!(
            abbreviate("10/26 12:34:56 memo", 15., false, measure),
            "10/26 12:34:56…"
        );
        assert_eq!(abbreviate("long", 0., true, measure), "");
        assert_eq!(abbreviate("long", 1., true, measure), "…");
    }
}

#[cfg(test)]
mod measurement_help_tests {
    use super::*;

    #[test]
    fn every_measurement_help_opens_and_closes_without_editing_settings() {
        let mut app = App::default();
        let before = serde_json::to_value(&app.library.draft).unwrap();
        for (label, help) in measurements::ALL
            .iter()
            .map(|metric| (metric.label, metric.help))
            .chain([("Limiting error", measurements::Help::LimitingError)])
        {
            let topic = Topic::Measurement(help);
            let mut ui =
                iced_test::simulator(app.help_body(topic, app.measurement_label(label, help)));
            assert!(ui.find(help.description()).is_err());
            ui.click("i").unwrap();
            let messages: Vec<_> = ui.into_messages().collect();
            assert_eq!(messages.len(), 1);
            for message in messages {
                let _ = app.update(message);
            }
            assert_eq!(app.open_help, Some(topic));
            assert!(app.previous_setting(topic).is_none());
            let mut ui =
                iced_test::simulator(app.help_body(topic, app.measurement_label(label, help)));
            assert!(
                ui.find(help.description())
                    .unwrap()
                    .visible_bounds()
                    .is_some()
            );
            ui.click("i").unwrap();
            for message in ui.into_messages() {
                let _ = app.update(message);
            }
            assert_eq!(app.open_help, None);
        }
        assert_eq!(serde_json::to_value(&app.library.draft).unwrap(), before);
    }
}

#[cfg(test)]
mod plot_help_tests {
    use super::*;

    #[test]
    fn chart_and_image_help_switches_without_changing_playback_or_settings() {
        let mut app = App::default();
        let before = serde_json::to_value(&app.library.draft).unwrap();
        app.position = 12.;
        app.region = Some((5., 15.));
        for help in [plots::Help::Waveform]
            .into_iter()
            .chain(plots::Kind::ALL.map(plots::Help::Chart))
            .chain([plots::Help::LimitingError])
        {
            let mut ui = iced_test::simulator(app.plot_heading(help));
            ui.find(help.label()).unwrap();
            assert!(ui.find(help.description()).is_err());
            ui.click("i").unwrap();
            for message in ui.into_messages() {
                let _ = app.update(message);
            }
            assert_eq!(app.open_help, Some(Topic::Plot(help)));
            let mut ui = iced_test::simulator(app.plot_heading(help));
            assert!(
                ui.find(help.description())
                    .unwrap()
                    .visible_bounds()
                    .is_some()
            );
        }
        let _ = app.update(Message::ToggleHelp(Topic::Plot(plots::Help::LimitingError)));
        assert_eq!(app.open_help, None);
        assert_eq!(app.position, 12.);
        assert_eq!(app.region, Some((5., 15.)));
        assert_eq!(serde_json::to_value(&app.library.draft).unwrap(), before);
    }
}
