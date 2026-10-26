use super::*;
use crate::descriptions::Topic;
use iced::widget::{column, responsive};

const SETTINGS_WIDTH: f32 = 330.;
const HISTORY_WIDTH: f32 = 300.;

impl App {
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
        let mut body = column![self.menu_bar()].spacing(8);
        if self.show_audio_path {
            body = body.push(
                row![
                    text_input("Paste an audio file path", &self.input)
                        .on_input(Message::Input)
                        .on_submit(Message::AddInput)
                        .padding(6),
                    button("Master")
                        .on_press_maybe((!self.input.is_empty()).then_some(Message::AddInput))
                        .style(theme::normal)
                        .padding(6),
                ]
                .spacing(6),
            );
        }
        let mut workspace = row![self.history_view(), rule::vertical(1)]
            .spacing(10)
            .height(Fill);
        let mut center = column![].spacing(8).width(Fill).height(Fill);
        if let Some(track) = self.track() {
            let mut title = row![
                column![
                    compact_text(track.name.clone(), source_tooltip(track), 17., true),
                    text(
                        self.job()
                            .map(|job| format!(
                                "{} · {}",
                                self.library.job_time_label(job),
                                job.status
                            ))
                            .unwrap_or_default()
                    )
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                ]
                .spacing(3)
                .width(Fill)
            ]
            .spacing(6)
            .align_y(iced::Center);
            if self.job().is_some_and(|j| j.status == Status::Succeeded) {
                title = title.push(
                    button("Export")
                        .on_press(Message::Export)
                        .style(theme::normal)
                        .padding(4),
                );
            }
            center = center.push(title);
            center = center.push(self.transport_view());
        }
        center = center.push(
            scrollable(self.analysis_view())
                .spacing(4)
                .height(Fill)
                .width(Fill),
        );
        workspace = workspace
            .push(container(center).clip(true).width(Fill).height(Fill))
            .push(rule::vertical(1))
            .push(
                container(self.settings_panel())
                    .clip(true)
                    .width(SETTINGS_WIDTH)
                    .height(Fill),
            );
        body = body.push(workspace).push(self.status_bar());
        container(body).padding(10).height(Fill).width(Fill).into()
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
            let detail = format!("{} · {}", job.status, job.memo);
            let failed = matches!(job.status, Status::Failed | Status::Interrupted);
            let label = if failed { format!("! {label}") } else { label };
            let cells = row![
                container(compact_text(label, detail, 12., false)).width(132),
                container(compact_text(
                    track.name.clone(),
                    source_tooltip(track),
                    12.,
                    true
                ))
                .width(Fill),
            ]
            .spacing(8)
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
            .spacing(4)
            .height(Fill)
            .into();
        let list = crate::history::HistoryInput {
            content: list,
            keys,
            selected: self.selected,
            offset: self.history_offset,
        };
        let pane = column![
            text("Audio & history").size(15),
            button("Master audio…")
                .on_press(Message::BrowseAudio)
                .width(Fill)
                .padding(6),
            text("Drop audio to master").size(12),
            container(
                row![
                    text("Time").size(12).width(132),
                    text("Input audio").size(12).width(Fill)
                ]
                .spacing(8)
            )
            .padding([0, 5]),
            rule::horizontal(1),
            Element::from(list),
        ]
        .spacing(8);
        container(pane)
            .clip(true)
            .width(HISTORY_WIDTH)
            .height(Fill)
            .into()
    }

    fn transport_view(&self) -> Element<'_, Message> {
        let mut controls = row![
            button(if self.player.status().2 {
                "Pause"
            } else {
                "Play"
            })
            .on_press_maybe(self.loaded.then_some(Message::Play))
        ]
        .spacing(6)
        .align_y(iced::Center);
        if let Some(job) = self.job() {
            controls = controls
                .push(
                    button(compact_text(
                        format!("A: {}", self.comparison_label()),
                        self.comparison_job()
                            .map_or_else(|| "Original".into(), |j| j.memo.clone()),
                        13.,
                        false,
                    ))
                    .width(Fill)
                    .style(if self.output_side {
                        theme::normal
                    } else {
                        theme::selected
                    })
                    .on_press(Message::Side(false)),
                )
                .push(
                    button(compact_text(
                        format!("B: {}", self.library.job_label(job)),
                        job.memo.clone(),
                        13.,
                        false,
                    ))
                    .width(Fill)
                    .style(if self.output_side {
                        theme::selected
                    } else {
                        theme::normal
                    })
                    .on_press_maybe(
                        (job.status == Status::Succeeded).then_some(Message::Side(true)),
                    ),
                );
        }
        controls = controls.push(iced::widget::space().width(Fill)).push(
            checkbox(self.equalize)
                .label("Match loudness")
                .on_toggle(Message::Equalize),
        );
        let mut transport = column![controls];
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
                    .size(12),
                ]
                .spacing(8)
                .align_y(iced::Center),
            )
            .spacing(5);
        let mut loop_row = row![text("Click: seek · Drag: loop").size(11)]
            .spacing(8)
            .align_y(iced::Center);
        if let Some((a, b)) = self.region {
            loop_row = loop_row
                .push(
                    text(format!("{}–{}", clock(a), clock(b)))
                        .size(11)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                )
                .push(
                    button("Clear loop")
                        .on_press(Message::ClearRegion)
                        .padding(2)
                        .style(theme::normal),
                );
        }
        transport = transport.push(loop_row);
        transport.into()
    }

    fn analysis_view(&self) -> Element<'_, Message> {
        let Some(track) = self.track() else {
            return column![
                text("Master audio to create a result")
                    .size(18)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                text("History switches audio at the same playback position.")
                    .size(13)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
            ]
            .spacing(8)
            .into();
        };
        let job = self.job();
        let baseline = self.comparison_job();
        let reference = self.reference_analysis();
        let has_reference = job.is_some_and(|j| j.config.reference.is_some());
        let mut detail = column![
            button("Copy source path")
                .on_press(Message::CopyText(track.source.display().to_string()))
                .padding(3)
                .style(theme::normal),
        ]
        .spacing(7)
        .width(Fill);
        if let Some(job) = job {
            detail = detail.push(
                text_input("Memo", &job.memo)
                    .on_input(move |memo| Message::EditMemo(job.id, memo))
                    .on_submit(Message::SaveMemo),
            );
            detail = detail.push(
                text(job.created_at.map_or_else(
                    || "Created: unknown (imported history)".into(),
                    |created| {
                        format!(
                            "Created: {}",
                            created
                                .with_timezone(&chrono::Local)
                                .format("%Y/%m/%d %H:%M:%S")
                        )
                    },
                ))
                .size(11)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );
            let choices = self.comparison_choices();
            let selected_id = baseline.map(|j| j.id);
            let selected_full = baseline.map_or_else(
                || "Original".into(),
                |job| format!("{}\n{}", self.library.job_time_label(job), job.memo),
            );
            detail = detail.push(
                row![
                    text("Compare A").size(12),
                    responsive(move |bounds| {
                        let mut visible = choices.clone();
                        for choice in &mut visible {
                            choice.label = abbreviate(
                                &choice.label,
                                (bounds.width - 44.).max(0.),
                                false,
                                |s| text_width(s, 12.),
                            );
                        }
                        let selected = visible.iter().find(|c| c.job == selected_id).cloned();
                        let original = choices.iter().find(|c| c.job == selected_id);
                        let show_tooltip = needs_tooltip(
                            selected.as_ref().map_or("", |c| c.label.as_str()),
                            original.map_or("", |c| c.label.as_str()),
                            &selected_full,
                        );
                        let control = pick_list(visible, selected, Message::Compare)
                            .text_size(12)
                            .width(Fill);
                        if !show_tooltip {
                            return control.into();
                        }
                        iced::widget::tooltip(
                            control,
                            container(
                                text(selected_full.clone())
                                    .size(12)
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
                .spacing(8)
                .align_y(iced::Center),
            );
        }
        let numeric_width = 99.;
        let mut heading = row![
            text("Measurement")
                .size(12)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(172),
            text("Original")
                .size(12)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(numeric_width)
        ]
        .spacing(6);
        if let Some(baseline) = baseline {
            heading = heading.push(
                text(format!("A\n{}", self.library.job_time_label(baseline)))
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(numeric_width),
            );
        }
        if let Some(job) = job {
            heading = heading.push(
                text(format!("B\n{}", self.library.job_time_label(job)))
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(numeric_width),
            );
        }
        if has_reference {
            heading = heading.push(
                text("Reference")
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(numeric_width),
            );
        }
        let mut table = column![heading, rule::horizontal(1)].spacing(3);
        for metric in measurements::ALL {
            let mut values = row![
                text(metric.label)
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(172),
                numeric_cell(metric.format(track.analysis.as_ref()), numeric_width)
            ]
            .spacing(6);
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
            table = table.push(values);
        }
        if let Some(job) = job {
            let mut values = row![
                text("Limiting error")
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .width(172),
                numeric_cell("—".into(), numeric_width)
            ]
            .spacing(6);
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
            table = table.push(values);
        }
        detail = detail
            .push(scrollable(table).horizontal().spacing(4).width(Fill))
            .push(
            text("* YouTube compensation: website estimate (−10.3 reference). Missing values: —.")
                .size(11).wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        );
        if track.analysis.as_ref().is_some_and(|a| {
            a.number("sound_quality").is_none() && a.sound_quality_error().is_none()
        }) {
            detail = detail.push(
                text("Reanalyze to calculate missing Professionality scores.")
                    .size(11)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );
        }
        if has_reference {
            let name = self.reference_name();
            detail = detail.push(compact_text(
                format!("Reference: {name}"),
                job.and_then(|j| j.config.reference_audio.as_ref())
                    .map_or(name.clone(), |p| p.display().to_string()),
                12.,
                true,
            ));
        }
        let mut tabs = row![].spacing(4);
        for kind in plots::Kind::ALL {
            tabs = tabs.push(
                button(
                    text(kind.to_string())
                        .size(12)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                )
                .padding(5)
                .style(if self.graph == kind {
                    theme::selected
                } else {
                    theme::normal
                })
                .on_press(Message::Graph(kind)),
            );
        }
        detail = detail.push(tabs);
        if self.graph == plots::Kind::Distribution {
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
                        .push(compact_text(label.clone(), label, 12., false))
                        .push(iced::widget::image(path).width(Fill));
                }
            }
            detail = detail.push(
                text("Distribution images require analysis generated with this application.")
                    .size(11)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );
        } else {
            let series = |a: Option<&analysis::Analysis>| {
                a.map_or_else(analysis::empty_series, |a| match self.graph {
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
                    .size(11)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                )
                .push(
                    canvas(plots::Plot {
                        input: series(self.comparison_analysis()),
                        output: series(job.and_then(|j| j.analysis.as_ref())),
                        reference: series(reference),
                        waveform: false,
                        logarithmic: self.graph == plots::Kind::Spectrum,
                        x_decibels: self.graph == plots::Kind::Histogram,
                        position: 0.,
                        region: None,
                    })
                    .width(Fill)
                    .height(220),
                )
                .push(
                    text(match self.graph {
                        plots::Kind::Spectrum => "Frequency (Hz, log) / level (dB)",
                        plots::Kind::Loudness => "Time (s) / loudness (dB)",
                        _ => "Loudness (dB) / frequency",
                    })
                    .size(11)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                );
        }
        detail = detail.push(
            button("Reanalyze")
                .on_press(Message::Analyze)
                .style(theme::normal),
        );
        if let Some(job) = job {
            detail = detail.push(
                button(if self.show_details {
                    "Hide processing details"
                } else {
                    "Processing details"
                })
                .on_press(Message::Details)
                .style(theme::normal),
            );
            if self.show_details {
                detail = detail.push(self.processing_details(job));
            }
        }
        detail.into()
    }

    fn processing_details<'a>(&'a self, job: &'a Job) -> Element<'a, Message> {
        let mut details = column![text(format!("Status: {}", job.status)).size(13).wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            text(config_summary(&job.config)).size(12).wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            text(format!("Reference mode: {} · Peak: {} · Lowpass: {} Hz\nCuts: {}–{} Hz · Oversampling: {}\nIterations: {} / {} · Noise: {}\n{} · Seed {} · Evaluations {} · Workers {}\nBass: {} · Pre-compression: {}\nOutput: {} Hz / {} / {} bit",
                job.config.reference_mode, job.config.ceiling_mode, job.config.lowpass_cut,
                job.config.low_cut, job.config.high_cut, job.config.oversample, job.config.max_iter1, job.config.max_iter2,
                job.config.noise_mode, job.config.solver, job.config.seed, job.config.evaluations, job.config.workers,
                job.config.preserve_bass, job.config.pre_compression, job.config.sample_rate, job.config.format, job.config.bit_depth)).size(12).wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        ].spacing(6);
        let image = job.output.with_extension("limiting-error.png");
        if image.is_file() {
            details = details
                .push(
                    text("Limiting-error spectrogram")
                        .size(13)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                )
                .push(iced::widget::image(image).width(Fill));
        }
        if let Some(command) = &job.command {
            details = details
                .push(
                    row![
                        text("Executed command")
                            .size(13)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                        button("Copy command")
                            .on_press(Message::CopyCommand(job.id))
                            .padding(3)
                    ]
                    .spacing(8),
                )
                .push(
                    scrollable(
                        text(command)
                            .size(12)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    )
                    .spacing(4)
                    .height(100),
                );
        }
        details
            .push(
                row![
                    text("Backend log")
                        .size(13)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    button("Copy log")
                        .on_press(Message::CopyLog(job.id))
                        .padding(3),
                ]
                .spacing(8)
                .align_y(iced::Center),
            )
            .push(
                scrollable(
                    text(
                        job.log[job.log.len().saturating_sub(400)..]
                            .iter()
                            .map(|line| line.chars().take(2000).collect::<String>())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                    .size(12)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                )
                .spacing(4)
                .height(160),
            )
            .into()
    }

    fn settings_panel(&self) -> Element<'_, Message> {
        let target = self.selected.map(|k| k.0);
        let track = self.library.tracks.iter().find(|t| Some(t.id) == target);
        let mut fields = column![self.mastering_settings_view()].spacing(8);
        if self.show_changes
            && let Some(job) = self.job()
        {
            fields = fields.push(self.run_changes_details(job));
        }
        let mut panel = column![
            text("Mastering settings").size(15),
            scrollable(fields)
                .id("mastering-fields")
                .spacing(4)
                .height(Fill)
                .width(Fill),
        ]
        .spacing(8)
        .height(Fill);
        if let Some(job) = self.job() {
            panel = panel.push(self.run_changes_view(job));
        }
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
                button(if self.active.is_some() {
                    "Create variation · queue next"
                } else {
                    "Create variation"
                })
                .on_press_maybe(issue.is_none().then_some(Message::Master))
                .width(Fill)
                .height(40)
                .padding(9),
            )
            .into()
    }

    fn run_changes_view<'a>(&'a self, job: &'a Job) -> Element<'a, Message> {
        let differences = settings_differences(&job.config, &self.library.draft);
        if differences.is_empty() {
            return text("No mastering parameter changes").size(12).into();
        }
        let count = differences.len();
        let heading = button(
            text(format!(
                "{} Changes · {count}",
                if self.show_changes { "▾" } else { "▸" }
            ))
            .size(13),
        )
        .on_press(Message::ToggleChanges)
        .style(theme::normal);
        heading.into()
    }

    fn run_changes_details<'a>(&'a self, job: &'a Job) -> Element<'a, Message> {
        let differences = settings_differences(&job.config, &self.library.draft);
        let mut changes = column![].spacing(3);
        for (label, mut old, mut new) in differences {
            if label == "Reference report" {
                old = self.reference_name();
                new = if self.library.draft.use_reference {
                    self.library.draft.reference_audio.clone()
                } else {
                    "Auto (no reference)".into()
                };
            }
            changes = changes.push(
                column![
                    text(label)
                        .size(12)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    compact_text(format!("From: {old}"), old, 12., true),
                    compact_text(format!("To: {new}"), new, 12., true),
                ]
                .spacing(2),
            );
        }
        changes.into()
    }

    fn mastering_settings_view(&self) -> Element<'_, Message> {
        let target_label = match self.library.draft.reference_mode {
            ReferenceMode::Loudness | ReferenceMode::YoutubeLoudness => "Target level (LUFS)",
            _ => "Target level (dBFS)",
        };
        let loudness = column![
            text("Loudness")
                .size(14)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
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
        .spacing(7)
        .width(Fill);
        let peak = column![
            text("Peak")
                .size(14)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
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
        .spacing(7)
        .width(Fill);
        let primary = column![loudness, rule::horizontal(1), peak].spacing(7);
        let mut settings = column![
            text("Level & limiter")
                .size(15)
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            primary,
            button(if self.show_limiter_details {
                "▾ Limiter details"
            } else {
                "▸ Limiter details"
            })
            .on_press(Message::ToggleLimiterDetails)
            .padding(4)
            .style(theme::normal),
        ]
        .spacing(7);
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
                    .spacing(8),
                    rule::horizontal(1),
                    text("Pre-compression")
                        .size(14)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    self.explained_toggle(
                        Topic::PreCompression,
                        (checkbox(self.library.draft.pre_compression)
                            .label("Pre-compression enabled")
                            .on_toggle(Message::PreCompression))
                        .into()
                    ),
                ]
                .spacing(7),
            );
        }
        settings = settings
            .push(rule::horizontal(1))
            .push(
                text("Automatic mastering")
                    .size(15)
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
                button(if self.show_optimization {
                    "▾ Optimization"
                } else {
                    "▸ Optimization"
                })
                .on_press(Message::ToggleOptimization)
                .padding(4)
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
                    .spacing(8),
                ]
                .spacing(7),
            );
        }
        settings
            .push(rule::horizontal(1))
            .push(
                text("Band limits")
                    .size(15)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .push(
                row![
                    self.numeric("Low cut (Hz)", Field::LowCut),
                    self.numeric("High cut (Hz)", Field::HighCut)
                ]
                .spacing(8),
            )
            .push(rule::horizontal(1))
            .push(
                text("Output")
                    .size(15)
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
                .spacing(8),
            )
            .push(rule::horizontal(1))
            .push(
                text("Execution")
                    .size(15)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .push(self.numeric("Workers (0: auto, max: 32)", Field::Workers))
            .into()
    }
}

fn numeric_cell<'a>(value: String, width: f32) -> Element<'a, Message> {
    text(value)
        .size(12)
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
        .font(iced::Font::MONOSPACE)
        .width(width)
        .align_x(iced::alignment::Horizontal::Right)
        .into()
}
fn error_value(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "—".into(), |v| format!("{v:.3} dB"))
}
fn config_summary(c: &model::Config) -> String {
    format!(
        "{} {} · peak {} dB · {}",
        c.loudness,
        match c.reference_mode {
            ReferenceMode::Loudness | ReferenceMode::YoutubeLoudness => "LUFS",
            _ => "dB",
        },
        c.ceiling,
        if c.mastering {
            format!("intensity {}", c.intensity)
        } else {
            "mastering off".into()
        }
    )
}

fn source_tooltip(track: &library::Track) -> String {
    track.source.display().to_string()
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
                    .size(12)
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
    .height(size * 1.3)
    .into()
}

#[cfg(test)]
mod long_name_tests {
    use super::*;
    fn measure(s: &str) -> f32 {
        s.chars().count() as f32
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
        assert!(needs_tooltip("mix.wav", "mix.wav", "/Desktop/mix.wav"));
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
