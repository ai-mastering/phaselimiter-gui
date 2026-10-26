use super::*;
use iced::widget::{column, responsive};

const SETTINGS_WIDTH: f32 = 330.;
const HISTORY_WIDTH: f32 = 215.;
const WIDE_WIDTH: f32 = 1200.;

impl App {
    pub(super) fn view(&self) -> Element<'_, Message> {
        responsive(|size| self.workspace_view(size.width)).into()
    }

    fn workspace_view(&self, width: f32) -> Element<'_, Message> {
        let wide = width >= WIDE_WIDTH;
        let mut header = row![text("PHASELIMITER").size(15)]
            .spacing(8)
            .align_y(iced::Center);
        header = header
            .push(
                button("Add audio…")
                    .on_press(Message::BrowseAudio)
                    .style(theme::normal)
                    .padding(6),
            )
            .push(
                button(if self.show_audio_path {
                    "Hide path input"
                } else {
                    "Add from path…"
                })
                .on_press(Message::ToggleAudioPath)
                .style(theme::normal)
                .padding(6),
            )
            .push(iced::widget::Space::new().width(Fill))
            .push(
                button("App settings")
                    .on_press(Message::ToggleAppSettings)
                    .style(theme::normal)
                    .padding(6),
            );
        let mut body = column![header].spacing(8);
        if self.show_audio_path {
            body = body.push(
                row![
                    text_input("Paste an audio file path", &self.input)
                        .on_input(Message::Input)
                        .on_submit(Message::AddInput)
                        .padding(6),
                    button("Add")
                        .on_press_maybe((!self.input.is_empty()).then_some(Message::AddInput))
                        .style(theme::normal)
                        .padding(6),
                ]
                .spacing(6),
            );
        }
        if self.hovering {
            body = body.push(text("Release to import audio.").size(13));
        }
        if let Some(error) = &self.error {
            body = body.push(self.error_view(error.clone()));
        }
        if let Some(notice) = &self.notice {
            body = body.push(text(notice).size(12));
        }
        let mut workspace = row![self.history_view(), rule::vertical(1)]
            .spacing(10)
            .height(Fill);
        let mut center = column![].spacing(8).width(Fill).height(Fill);
        if let Some(track) = self.track() {
            let mut title = row![
                text(if let Some(job) = self.job() {
                    format!(
                        "{} / {} · {}",
                        track.name,
                        self.library.job_label(job),
                        job.status
                    )
                } else {
                    track.name.clone()
                })
                .size(17)
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
        if !wide {
            center = center.push(
                row![
                    button("Analysis").on_press(Message::ShowAnalysis).style(
                        if !self.show_settings && !self.show_app_settings {
                            theme::selected
                        } else {
                            theme::normal
                        }
                    ),
                    button("Mastering settings")
                        .on_press(Message::ShowMastering)
                        .style(if self.show_settings && !self.show_app_settings {
                            theme::selected
                        } else {
                            theme::normal
                        }),
                ]
                .spacing(6),
            );
        }
        if !wide && (self.show_settings || self.show_app_settings) {
            center = center.push(self.settings_panel(width - HISTORY_WIDTH - 32. >= 560.));
        } else {
            center = center.push(
                scrollable(self.analysis_view())
                    .spacing(4)
                    .height(Fill)
                    .width(Fill),
            );
        }
        workspace = workspace.push(center);
        if wide {
            workspace = workspace.push(rule::vertical(1)).push(
                container(self.settings_panel(false))
                    .width(SETTINGS_WIDTH)
                    .height(Fill),
            );
        }
        body = body.push(workspace).push(self.queue_view());
        container(body).padding(10).height(Fill).width(Fill).into()
    }

    fn history_view(&self) -> Element<'_, Message> {
        let mut history = column![].spacing(8);
        if self.library.tracks.is_empty() {
            history = history.push(text("Drop audio to begin.").size(13));
        }
        for track in self.library.tracks.iter().rev() {
            let key = (track.id, None);
            let mut entries = column![
                text(&track.name).size(14),
                button("Original")
                    .on_press(Message::Select(key))
                    .width(Fill)
                    .padding(5)
                    .style(if self.selected == Some(key) {
                        theme::selected
                    } else {
                        theme::normal
                    }),
            ]
            .spacing(4);
            let jobs: Vec<_> = self
                .library
                .jobs
                .iter()
                .filter(|j| j.track_id == track.id)
                .collect();
            for job in jobs.into_iter().rev() {
                let key = (track.id, Some(job.id));
                let label = self.library.job_label(job);
                let failed = matches!(job.status, Status::Failed | Status::Interrupted);
                let label = if failed { format!("! {label}") } else { label };
                entries = entries.push(
                    button(text(label).size(12).color(if failed {
                        theme::ERROR
                    } else {
                        theme::LABEL
                    }))
                    .width(Fill)
                    .padding(5)
                    .style(if self.selected == Some(key) {
                        theme::selected
                    } else {
                        theme::normal
                    })
                    .on_press(Message::Select(key)),
                );
                if job.status == Status::Processing {
                    entries = entries.push(progress_bar(0.0..=1.0, job.progress).girth(3));
                }
            }
            history = history.push(entries).push(rule::horizontal(1));
        }
        let mut pane = column![
            text("Audio & history").size(15),
            scrollable(history).spacing(4).height(Fill)
        ]
        .spacing(8);
        if self.importing > 0 {
            pane = pane.push(text(format!("Importing {}…", self.importing)).size(12));
        }
        if let Some(key) = self.selected {
            pane = pane
                .push(
                    button("Master this input")
                        .on_press(Message::Compose)
                        .width(Fill),
                )
                .push(
                    button("Remove selected")
                        .on_press(Message::Remove(key))
                        .style(theme::normal),
                );
        }
        pane.width(HISTORY_WIDTH).height(Fill).into()
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
                    button(text(format!("A: {}", self.comparison_label())).size(13))
                        .style(if self.output_side {
                            theme::normal
                        } else {
                            theme::selected
                        })
                        .on_press(Message::Side(false)),
                )
                .push(
                    button(text(format!("B: {}", self.library.job_label(job))).size(13))
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
        let mut transport = column![
            controls,
            canvas(plots::Plot {
                input: self
                    .listening_analysis()
                    .map_or_else(analysis::empty_series, analysis::Analysis::waveform),
                output: analysis::empty_series(),
                reference: analysis::empty_series(),
                waveform: true,
                logarithmic: false,
                position: (self.position / self.duration.max(0.001)) as f32,
                region: self.region.map(|(a, b)| (
                    (a / self.duration.max(0.001)) as f32,
                    (b / self.duration.max(0.001)) as f32
                )),
            })
            .width(Fill)
            .height(65),
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
        ]
        .spacing(5);
        let mut loop_row = row![text("Click: seek · Drag: loop").size(11)]
            .spacing(8)
            .align_y(iced::Center);
        if let Some((a, b)) = self.region {
            loop_row = loop_row
                .push(text(format!("{}–{}", clock(a), clock(b))).size(11))
                .push(
                    button("Clear loop")
                        .on_press(Message::ClearRegion)
                        .padding(2)
                        .style(theme::normal),
                );
        }
        transport = transport.push(loop_row);
        if self.loading {
            transport = transport
                .push(text("Preparing selection · current audio continues playing").size(12));
        } else if self.loaded && self.listening_key() != self.active_key {
            transport = transport
                .push(text("Selected audio unavailable · playing previous selection").size(12));
        }
        if self.equalize && self.listening_analysis().is_none() {
            transport = transport.push(text("Loudness matching awaits analysis.").size(12));
        }
        transport.into()
    }

    fn analysis_view(&self) -> Element<'_, Message> {
        let Some(track) = self.track() else {
            return column![
                text("Select an original or mastering result").size(18),
                text("History switches audio at the same playback position.").size(13)
            ]
            .spacing(8)
            .into();
        };
        let job = self.job();
        let baseline = self.comparison_job();
        let reference = self.reference_analysis();
        let has_reference = job.is_some_and(|j| j.config.reference.is_some());
        let mut detail = column![].spacing(7);
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
                .size(11),
            );
            let choices = self.comparison_choices();
            let selected = choices
                .iter()
                .find(|choice| choice.job == baseline.map(|j| j.id))
                .cloned();
            detail = detail.push(
                row![
                    text("Compare A").size(12),
                    pick_list(choices, selected, Message::Compare),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
        }
        let numeric_width = 99.;
        let mut heading = row![
            text("Measurement").size(12).width(172),
            text("Original").size(12).width(numeric_width)
        ]
        .spacing(6);
        if let Some(baseline) = baseline {
            heading = heading.push(
                text(format!("A: {}", self.library.job_label(baseline)))
                    .size(12)
                    .width(numeric_width),
            );
        }
        if let Some(job) = job {
            heading = heading.push(
                text(format!("B: {}", self.library.job_label(job)))
                    .size(12)
                    .width(numeric_width),
            );
        }
        if has_reference {
            heading = heading.push(text("Reference").size(12).width(numeric_width));
        }
        let mut table = column![heading, rule::horizontal(1)].spacing(3);
        for metric in measurements::ALL {
            let mut values = row![
                text(metric.label).size(12).width(172),
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
                text("Limiting error").size(12).width(172),
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
                .size(11),
        );
        if track.analysis.as_ref().is_some_and(|a| {
            a.number("sound_quality").is_none() && a.sound_quality_error().is_none()
        }) {
            detail = detail
                .push(text("Reanalyze to calculate missing Professionality scores.").size(11));
        }
        for (label, analysis) in [
            ("Original", track.analysis.as_ref()),
            ("Compare A", baseline.and_then(|j| j.analysis.as_ref())),
            ("Selected", job.and_then(|j| j.analysis.as_ref())),
            ("Reference", reference),
        ] {
            if let Some(error) = analysis.and_then(analysis::Analysis::sound_quality_error) {
                detail = detail.push(self.error_view(format!("{label} Professionality: {error}")));
            }
        }
        for (key, error) in [
            ((track.id, None), track.analysis_error.as_ref()),
            (
                (track.id, job.map(|j| j.id)),
                job.and_then(|j| j.analysis_error.as_ref()),
            ),
        ]
        .into_iter()
        .take(if job.is_some() { 2 } else { 1 })
        {
            if self.analyzing == Some(key) {
                detail = detail.push(text("Analyzing…").size(12));
            }
            if let Some(error) = error {
                detail = detail.push(self.error_view(format!(
                    "{} analysis: {error}",
                    if key.1.is_some() {
                        "Selected"
                    } else {
                        "Original"
                    }
                )));
            }
        }
        if has_reference {
            let name = self.reference_name();
            detail = detail.push(text(format!("Reference: {name}")).size(12));
            if let Some((_, Err(error))) = &self.reference_report {
                detail = detail
                    .push(self.error_view(format!("Reference analysis unavailable: {error}")));
            } else if reference.is_none() {
                detail = detail.push(text("Loading reference analysis…").size(12));
            }
        }
        let mut tabs = row![].spacing(4);
        for kind in plots::Kind::ALL {
            tabs = tabs.push(
                button(text(kind.to_string()).size(12))
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
                        .push(text(label).size(12))
                        .push(iced::widget::image(path).width(Fill));
                }
            }
            detail = detail.push(
                text("Distribution images require analysis generated with this application.")
                    .size(11),
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
                    .size(11),
                )
                .push(
                    canvas(plots::Plot {
                        input: series(self.comparison_analysis()),
                        output: series(job.and_then(|j| j.analysis.as_ref())),
                        reference: series(reference),
                        waveform: false,
                        logarithmic: self.graph == plots::Kind::Spectrum,
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
                    .size(11),
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
        let mut details = column![text(format!("Status: {}", job.status)).size(13),
            text(config_summary(&job.config)).size(12),
            text(format!("Reference mode: {} · Peak: {} · Lowpass: {} Hz\nCuts: {}–{} Hz · Oversampling: {}\nIterations: {} / {} · Noise: {}\n{} · Seed {} · Evaluations {} · Workers {}\nBass: {} · Pre-compression: {}\nOutput: {} Hz / {} / {} bit",
                job.config.reference_mode, job.config.ceiling_mode, job.config.lowpass_cut,
                job.config.low_cut, job.config.high_cut, job.config.oversample, job.config.max_iter1, job.config.max_iter2,
                job.config.noise_mode, job.config.solver, job.config.seed, job.config.evaluations, job.config.workers,
                job.config.preserve_bass, job.config.pre_compression, job.config.sample_rate, job.config.format, job.config.bit_depth)).size(12),
        ].spacing(6);
        let image = job.output.with_extension("limiting-error.png");
        if image.is_file() {
            details = details
                .push(text("Limiting-error spectrogram").size(13))
                .push(iced::widget::image(image).width(Fill));
        }
        if let Some(command) = &job.command {
            details = details
                .push(
                    row![
                        text("Executed command").size(13),
                        button("Copy command")
                            .on_press(Message::CopyCommand(job.id))
                            .padding(3)
                    ]
                    .spacing(8),
                )
                .push(scrollable(text(command).size(12)).spacing(4).height(100));
        }
        details
            .push(
                row![
                    text("Backend log").size(13),
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
                    .size(12),
                )
                .spacing(4)
                .height(160),
            )
            .into()
    }

    fn settings_panel(&self, wide_form: bool) -> Element<'_, Message> {
        if self.show_app_settings {
            return column![
                text("Application settings").size(15),
                scrollable(self.app_settings_view()).spacing(4).height(Fill),
            ]
            .spacing(8)
            .height(Fill)
            .into();
        }
        let target = self.selected.map(|k| k.0);
        let track = self.library.tracks.iter().find(|t| Some(t.id) == target);
        let mut panel = column![
            text("Mastering settings").size(15),
            scrollable(self.mastering_settings_view(wide_form))
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
                Some((None, "Select an input first.".into()))
            } else if self.root.is_none() {
                Some((None, "Data directory unavailable.".into()))
            } else {
                None
            }
        });
        if let Some((_, reason)) = &issue {
            panel = panel.push(text(format!("Cannot run mastering: {reason}")).size(12));
        }
        panel
            .push(
                button(if self.active.is_some() {
                    "Run mastering · queue next"
                } else {
                    "Run mastering"
                })
                .on_press_maybe(issue.is_none().then_some(Message::Master))
                .width(Fill)
                .padding(9),
            )
            .into()
    }

    fn run_changes_view<'a>(&'a self, job: &'a Job) -> Element<'a, Message> {
        let differences = settings_differences(&job.config, &self.library.draft);
        let height = (differences.len().max(1) as f32 * 22.).min(110.);
        let mut changes = column![].spacing(3);
        if differences.is_empty() {
            changes = changes.push(text("No mastering parameter changes").size(12));
        }
        for (label, mut old, mut new) in differences {
            if label == "Reference report" {
                old = self.reference_name();
                new = if self.library.draft.use_reference {
                    self.library.draft.reference_audio.clone()
                } else {
                    "Auto (no reference)".into()
                };
            }
            changes = changes.push(text(format!("{label}: {old} → {new}")).size(12));
        }
        column![
            rule::horizontal(1),
            text("Changes").size(13),
            scrollable(changes).spacing(4).height(height),
        ]
        .spacing(4)
        .into()
    }

    fn mastering_settings_view(&self, wide_form: bool) -> Element<'_, Message> {
        let target_label = match self.library.draft.reference_mode {
            ReferenceMode::Loudness | ReferenceMode::YoutubeLoudness => "Target level (LUFS)",
            _ => "Target level (dBFS)",
        };
        let loudness = column![
            text("Loudness").size(14),
            pick_list(
                ReferenceMode::ALL,
                Some(self.library.draft.reference_mode),
                Message::ReferenceMode
            )
            .width(Fill),
            self.numeric(target_label, Field::Loudness),
        ]
        .spacing(7)
        .width(Fill);
        let peak = column![
            text("Peak").size(14),
            pick_list(
                CeilingMode::ALL,
                Some(self.library.draft.ceiling_mode),
                Message::CeilingMode
            )
            .width(Fill),
            self.numeric("Ceiling (dBFS)", Field::Ceiling),
            self.numeric("Lowpass true-peak frequency (Hz)", Field::LowpassCut),
        ]
        .spacing(7)
        .width(Fill);
        let primary: Element<'_, Message> = if wide_form {
            row![loudness, peak].spacing(12).into()
        } else {
            column![loudness, rule::horizontal(1), peak]
                .spacing(7)
                .into()
        };
        let mut settings = column![
            text("Level & limiter").size(15),
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
                    checkbox(self.library.draft.preserve_bass)
                        .label("Preserve bass")
                        .on_toggle(Message::PreserveBass),
                    self.numeric_choice("Oversampling", Field::Oversample, &model::OVERSAMPLING),
                    text("Noise update").size(12),
                    pick_list(
                        NoiseMode::ALL,
                        Some(self.library.draft.noise_mode),
                        Message::NoiseMode
                    )
                    .width(Fill),
                    row![
                        self.numeric("Outer iterations", Field::MaxIter1),
                        self.numeric("Inner iterations", Field::MaxIter2)
                    ]
                    .spacing(8),
                    rule::horizontal(1),
                    text("Pre-compression").size(14),
                    checkbox(self.library.draft.pre_compression)
                        .label("Pre-compression enabled")
                        .on_toggle(Message::PreCompression),
                ]
                .spacing(7),
            );
        }
        settings = settings
            .push(rule::horizontal(1))
            .push(text("Automatic mastering").size(15))
            .push(
                checkbox(self.library.draft.mastering)
                    .label("Automatic mastering enabled")
                    .on_toggle(Message::Mastering),
            )
            .push(self.intensity_control())
            .push(text("Reference").size(14))
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
            .push(self.reference_control())
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
                    controls::choice(
                        Solver::ALL.to_vec(),
                        Some(self.library.draft.solver),
                        Message::Solver,
                        self.library.draft.mastering
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
            .push(text("Band limits").size(15))
            .push(
                row![
                    self.numeric("Low cut (Hz)", Field::LowCut),
                    self.numeric("High cut (Hz)", Field::HighCut)
                ]
                .spacing(8),
            )
            .push(rule::horizontal(1))
            .push(text("Output").size(15))
            .push(
                pick_list(
                    Format::ALL,
                    Some(self.library.draft.format),
                    Message::Format,
                )
                .width(Fill),
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
            .push(text("Execution").size(15))
            .push(self.numeric("Workers (0: auto, max: 32)", Field::Workers))
            .into()
    }

    fn app_settings_view(&self) -> Element<'_, Message> {
        column![
            text("Engine & resources").size(14),
            text("Blank: bundled with this app. Non-empty: use the specified path.").size(12),
            button("Clear paths / use bundled tools").on_press(Message::UseBundledTools).style(theme::normal),
            field(
                "phase_limiter executable",
                &self.library.app_settings.backend,
                Field::Backend
            ),
            field(
                "FFmpeg executable / PATH name",
                &self.library.app_settings.ffmpeg,
                Field::Ffmpeg
            ),
            field("sound_quality2_cache", &self.library.app_settings.cache, Field::Cache),
            text("Professionality uses audio_detector beside phase_limiter and resource/detection_preparation.json with its analysis_data corpus.")
                .size(12),
            rule::horizontal(1),
            field("Default export directory", &self.library.app_settings.export_dir, Field::ExportDir),
            rule::horizontal(1),
            text(format!(
                "Library: {:.1} MiB",
                self.storage_bytes as f64 / 1048576.
            ))
            .size(14),
            text(
                self.library_folder()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            )
            .size(12),
            button("Open library folder").on_press(Message::OpenLibrary),
        ]
        .spacing(8)
        .into()
    }

    fn queue_view(&self) -> Element<'_, Message> {
        let waiting = self
            .library
            .jobs
            .iter()
            .filter(|j| j.status == Status::Waiting)
            .count();
        let mut footer = row![].spacing(8).align_y(iced::Center);
        if let Some(active) = &self.active
            && let Some(job) = self.library.jobs.iter().find(|j| j.id == active.id)
        {
            footer = footer
                .push(text(format!("Processing {}", self.library.job_label(job))).size(12))
                .push(container(progress_bar(0.0..=1.0, job.progress).girth(5)).width(100));
        }
        footer = footer.push(text(format!("{waiting} queued")).size(12));
        if waiting > 0 && !self.running {
            footer = footer.push(button("Resume queue").on_press(Message::Start).padding(4));
        }
        if self.active.is_some() {
            footer = footer.push(
                button("Cancel current & pause")
                    .on_press(Message::Cancel)
                    .padding(4)
                    .style(theme::normal),
            );
        }
        footer.into()
    }
}

fn numeric_cell<'a>(value: String, width: f32) -> Element<'a, Message> {
    text(value)
        .size(12)
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
