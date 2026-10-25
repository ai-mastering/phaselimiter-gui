use super::*;
use iced::widget::column;

impl App {
    pub(super) fn view(&self) -> Element<'_, Message> {
        let header = row![
            column![
                text("PHASELIMITER").size(12),
                text("Listen, master, compare").size(26)
            ]
            .spacing(4),
            iced::widget::space().width(Fill),
            button("Settings").on_press(Message::ToggleSettings)
        ]
        .align_y(iced::Center);
        let add = row![
            text_input("Drop audio here, or paste a file path", &self.input)
                .on_input(Message::Input)
                .on_submit(Message::AddInput)
                .padding(9),
            button("Add audio").on_press(Message::AddInput).padding(9)
        ]
        .spacing(8);
        let mut history = Column::new().spacing(12);
        if self.library.tracks.is_empty() {
            history =
                history.push(text("Add audio to start. Each import keeps its own copy.").size(14));
        }
        for track in self.library.tracks.iter().rev() {
            let source_key = (track.id, None);
            let mut entries = column![
                button(text(format!("{}  · #{}", track.name, track.id)).size(15))
                    .width(Fill)
                    .style(if self.selected == Some(source_key) {
                        button::primary
                    } else {
                        button::text
                    })
                    .on_press(Message::Select(source_key)),
                button("Original")
                    .style(button::text)
                    .on_press(Message::Select(source_key)),
            ]
            .spacing(4);
            for (index, job) in self
                .library
                .jobs
                .iter()
                .filter(|j| j.track_id == track.id)
                .enumerate()
            {
                let key = (track.id, Some(job.id));
                let label = format!(
                    "Master {} · {} LUFS · {}",
                    index + 1,
                    job.config.loudness,
                    job.status
                );
                entries = entries.push(
                    button(text(label).size(13))
                        .width(Fill)
                        .style(if self.selected == Some(key) {
                            button::primary
                        } else {
                            button::text
                        })
                        .on_press(Message::Select(key)),
                );
                if job.status == Status::Processing {
                    entries = entries.push(progress_bar(0.0..=1.0, job.progress).girth(4));
                }
            }
            history = history.push(
                container(entries)
                    .padding(8)
                    .width(Fill)
                    .style(container::rounded_box),
            );
        }
        let mut left = column![
            text("Audio & history").size(19),
            scrollable(history).spacing(12).height(Fill)
        ]
        .spacing(12)
        .width(310);
        if self.importing > 0 {
            left = left.push(text(format!("Importing {} file(s)…", self.importing)).size(12));
        }
        if let Some(key) = self.selected {
            left = left.push(
                button("Master this input")
                    .on_press(Message::Compose)
                    .width(Fill),
            );
            left = left.push(
                button("Remove selected")
                    .on_press(Message::Remove(key))
                    .style(button::text),
            );
        }
        let mut detail = Column::new().spacing(14).width(Fill);
        if let Some(track) = self.track() {
            let job = self.job();
            detail = detail.push(
                text(if let Some(job) = job {
                    format!("{} / Master #{}", track.name, job.id)
                } else {
                    track.name.clone()
                })
                .size(22),
            );
            let mut transport = row![
                button(if self.player.status().2 {
                    "Pause"
                } else {
                    "Play"
                })
                .on_press_maybe(self.loaded.then_some(Message::Play))
            ]
            .spacing(10)
            .align_y(iced::Center);
            if let Some(job) = job {
                transport = transport
                    .push(
                        button("Original")
                            .style(if self.output_side {
                                button::secondary
                            } else {
                                button::primary
                            })
                            .on_press(Message::Side(false)),
                    )
                    .push(
                        button("Mastered")
                            .style(if self.output_side {
                                button::primary
                            } else {
                                button::secondary
                            })
                            .on_press_maybe(
                                (job.status == Status::Succeeded).then_some(Message::Side(true)),
                            ),
                    );
            }
            transport = transport.push(iced::widget::space().width(Fill)).push(
                checkbox(self.equalize)
                    .label("Match loudness")
                    .on_toggle(Message::Equalize),
            );
            detail = detail.push(transport);
            if self.equalize && self.listening_analysis().is_none() {
                detail = detail.push(
                    text("Loudness matching is available after analysis completes.").size(12),
                );
            }
            if self.loading {
                detail = detail
                    .push(text("Preparing selection… Current audio continues playing.").size(12));
            }
            detail = detail.push(
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
                    .size(13)
                ]
                .spacing(10),
            );
            let listening = self.listening_analysis();
            detail = detail.push(
                canvas(plots::Plot {
                    input: listening
                        .map_or_else(analysis::empty_series, analysis::Analysis::waveform),
                    output: analysis::empty_series(),
                    waveform: true,
                    logarithmic: false,
                    position: (self.position / self.duration.max(0.001)) as f32,
                    region: self.region.map(|(a, b)| {
                        (
                            (a / self.duration.max(0.001)) as f32,
                            (b / self.duration.max(0.001)) as f32,
                        )
                    }),
                })
                .width(Fill)
                .height(100),
            );
            let mut loop_row =
                row![text("Click to seek · Drag to loop (playback only)").size(12)].spacing(12);
            if let Some((a, b)) = self.region {
                loop_row = loop_row
                    .push(text(format!("{}–{}", clock(a), clock(b))).size(12))
                    .push(
                        button("Clear loop")
                            .on_press(Message::ClearRegion)
                            .style(button::text),
                    );
            }
            detail = detail.push(loop_row);
            let input = track.analysis.as_ref();
            let output = job.and_then(|j| j.analysis.as_ref());
            let mut metrics = column![row![
                text("Measurement").width(Fill),
                text("Original").width(110),
                text(if job.is_some() { "Mastered" } else { "" }).width(110)
            ]]
            .spacing(7);
            for (label, key, unit) in [
                ("Loudness", "loudness", " LUFS"),
                ("True peak", "true_peak", " dBTP"),
                ("Loudness range", "loudness_range", " LU"),
                ("RMS", "rms", " dB"),
                ("Sound quality (engine score)", "sound_quality2", ""),
            ] {
                metrics = metrics.push(row![
                    text(label).size(13).width(Fill),
                    text(measure(input, key, unit)).size(13).width(110),
                    text(if job.is_some() {
                        measure(output, key, unit)
                    } else {
                        String::new()
                    })
                    .size(13)
                    .width(110)
                ]);
            }
            detail = detail.push(container(metrics).padding(12).style(container::rounded_box));
            let mut status = Column::new().spacing(4);
            for (key, error) in [
                ((track.id, None), track.analysis_error.as_ref()),
                (
                    (track.id, job.map(|j| j.id)),
                    job.and_then(|j| j.analysis_error.as_ref()),
                ),
            ] {
                if self.analyzing == Some(key) {
                    status = status.push(text("Analyzing audio…").size(12));
                }
                if let Some(error) = error {
                    status = status.push(
                        text(error)
                            .size(12)
                            .color(iced::Color::from_rgb8(165, 45, 40)),
                    );
                }
            }
            detail = detail.push(status).push(
                button("Analyze / retry analysis")
                    .on_press(Message::Analyze)
                    .style(button::text),
            );
            detail = detail.push(
                row![
                    pick_list(plots::Kind::ALL, Some(self.graph), Message::Graph),
                    text("Gray: original  ·  Blue: mastered").size(12)
                ]
                .spacing(12)
                .align_y(iced::Center),
            );
            let series = |a: Option<&analysis::Analysis>| {
                a.map_or_else(analysis::empty_series, |a| match self.graph {
                    plots::Kind::Spectrum => a.series("spectrum", "hz", "db"),
                    plots::Kind::Loudness => a.series("loudness_time_series", "sec", "db"),
                    plots::Kind::Histogram => a.series("histogram", "db", "frequency"),
                })
            };
            detail = detail.push(
                canvas(plots::Plot {
                    input: series(input),
                    output: series(output),
                    waveform: false,
                    logarithmic: self.graph == plots::Kind::Spectrum,
                    position: 0.,
                    region: None,
                })
                .width(Fill)
                .height(190),
            );
            detail = detail.push(
                text(match self.graph {
                    plots::Kind::Spectrum => "Frequency (Hz, log scale) / level (dB)",
                    plots::Kind::Loudness => "Time (seconds) / loudness (dB)",
                    plots::Kind::Histogram => "Loudness (dB) / frequency",
                })
                .size(12),
            );
            if let Some(job) = job {
                detail = detail.push(
                    button(if self.show_details {
                        "▾ Processing details"
                    } else {
                        "▸ Processing details"
                    })
                    .on_press(Message::Details)
                    .style(button::text),
                );
                if self.show_details {
                    detail=detail.push(text(format!("Status: {}\nTarget: {} LUFS · Ceiling: {} dBTP\nIntensity: {} · Stereo: {}\n{} · Seed {} · {} evaluations · {} workers\nBass preservation: {} · Pre-compression: {}\nOutput: {} Hz / {} / {} bits\nReference: {}",job.status,job.config.loudness,job.config.ceiling,job.config.intensity,job.config.stereo,job.config.solver,job.config.seed,job.config.evaluations,job.config.workers,job.config.preserve_bass,job.config.pre_compression,job.config.sample_rate,job.config.format,job.config.bit_depth,if job.config.reference.is_some(){"Custom audio"}else{"Default"})).size(13));
                    if let Some(error) = job.limiting_error {
                        detail =
                            detail.push(text(format!("Limiting error: {error:.3} dB")).size(13));
                    }
                    detail = detail
                        .push(button("Use these settings").on_press(Message::UseSettings(job.id)));
                    let image_path = job.output.with_extension("limiting-error.png");
                    if image_path.is_file() {
                        detail = detail
                            .push(text("Limiting error spectrogram").size(14))
                            .push(iced::widget::image(image_path).width(Fill));
                    }
                    detail = detail
                        .push(text("Backend log").size(14))
                        .push(scrollable(text(job.log.join("\n")).size(12)).height(180));
                }
                if job.status == Status::Succeeded {
                    detail = detail
                        .push(rule::horizontal(1))
                        .push(text("Export a copy").size(15))
                        .push(
                            row![
                                text_input("Destination filename", &self.export_path)
                                    .on_input(Message::ExportPath)
                                    .padding(8),
                                button("Export").on_press(Message::Export)
                            ]
                            .spacing(8),
                        );
                }
            } else {
                detail = detail.push(
                    text(format!(
                        "Added: Unix time {} · Source: {}",
                        track.added,
                        track.source.display()
                    ))
                    .size(12),
                );
            }
        } else {
            detail=detail.push(text("Select an original or a mastering result").size(24)).push(text("Click history items to switch audio at the same playback position. No comparison list to assemble."));
        }
        let mut body = column![header, add];
        if self.hovering {
            body = body.push(text("Release to import an independent copy."));
        }
        if let Some(error) = &self.error {
            body = body.push(
                text(error)
                    .color(iced::Color::from_rgb8(170, 40, 40))
                    .size(13),
            );
        }
        if let Some(notice) = &self.notice {
            body = body.push(text(notice).size(12));
        }
        body = body.push(
            row![
                left,
                rule::vertical(1),
                scrollable(detail).spacing(12).height(Fill).width(Fill)
            ]
            .spacing(18)
            .height(Fill),
        );
        if self.show_settings {
            let target = self
                .library
                .tracks
                .iter()
                .find(|t| Some(t.id) == self.composer_track)
                .map(|t| t.name.as_str())
                .unwrap_or("Select an input in history");
            body = body
                .push(rule::horizontal(1))
                .push(
                    row![
                        text(format!("Mastering settings · {target}")).size(17),
                        iced::widget::space().width(Fill),
                        button("Close settings").on_press(Message::ToggleSettings)
                    ]
                    .align_y(iced::Center),
                )
                .push(scrollable(self.settings_view()).spacing(12).height(270))
                .push(
                    button("Master with these settings")
                        .on_press_maybe(self.composer_track.map(|_| Message::Master))
                        .padding(10),
                );
        }
        let waiting = self
            .library
            .jobs
            .iter()
            .filter(|j| j.status == Status::Waiting)
            .count();
        let mut footer = row![
            text(format!("{waiting} queued")).size(12),
            button("Start queue")
                .on_press_maybe((waiting > 0 && self.active.is_none()).then_some(Message::Start)),
            button("Cancel current & pause")
                .on_press_maybe(self.active.as_ref().map(|_| Message::Cancel))
                .style(button::secondary)
        ]
        .spacing(10)
        .align_y(iced::Center);
        if self.root.is_some() {
            footer = footer.push(iced::widget::space().width(Fill)).push(
                button("Library folder")
                    .on_press(Message::OpenLibrary)
                    .style(button::text),
            );
        }
        body = body.push(footer);
        container(body.spacing(12).width(Fill))
            .padding(20)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let settings = column![
            text("New mastering").size(18),
            pick_list(self.reference_choices(), self.selected_reference(), Message::ChooseReference),
            text("Applied only when you start a new mastering. History selection does not change settings.").size(12),
            field(
                "Output directory",
                &self.draft.output_dir,
                Field::OutputDir
            ),
            row![
                field(
                    "Target loudness (LUFS)",
                    &self.draft.loudness,
                    Field::Loudness
                ),
                field(
                    "True-peak ceiling (dBTP)",
                    &self.draft.ceiling,
                    Field::Ceiling
                )
            ]
            .spacing(12),
            row![
                field(
                    "Mastering intensity (0–1)",
                    &self.draft.intensity,
                    Field::Intensity
                ),
                field(
                    "Stereo matching (0–1)",
                    &self.draft.stereo,
                    Field::Stereo
                )
            ]
            .spacing(12),
            checkbox(self.draft.preserve_bass)
                .label("Preserve bass")
                .on_toggle(Message::PreserveBass),
            checkbox(self.draft.pre_compression)
                .label("Pre-compression")
                .on_toggle(Message::PreCompression),
            rule::horizontal(1),
            text("Output").size(18),
            row![
                column![
                    text("Format").size(12),
                    pick_list(Format::ALL, Some(self.draft.format), Message::Format).width(Fill)
                ]
                .spacing(5),
                field(
                    "Sample rate (Hz)",
                    &self.draft.sample_rate,
                    Field::SampleRate
                ),
                field(
                    "WAV bits (32 = float)",
                    &self.draft.bit_depth,
                    Field::BitDepth
                )
            ]
            .spacing(12),
            rule::horizontal(1),
            text("Optimization").size(18),
            pick_list(Solver::ALL, Some(self.draft.solver), Message::Solver).width(Fill),
            row![
                field(
                    "Evaluation budget",
                    &self.draft.evaluations,
                    Field::Evaluations
                ),
                field("Seed", &self.draft.seed, Field::Seed)
            ]
            .spacing(12),
            field(
                "Worker threads (0 = automatic)",
                &self.draft.workers,
                Field::Workers
            ),
            rule::horizontal(1),
            text("Engine & resources").size(18),
            text("Use the backend's feature/codex build. The GUI can open without it.").size(12),
            field(
                "phase_limiter executable",
                &self.draft.backend,
                Field::Backend
            ),
            field(
                "FFmpeg executable or PATH name",
                &self.draft.ffmpeg,
                Field::Ffmpeg
            ),
            field("sound_quality2_cache", &self.draft.cache, Field::Cache),
            rule::horizontal(1),
            text(format!("Library storage: {:.1} MiB",self.storage_bytes as f64 / 1048576.)).size(15),
            text(self.root.as_ref().map(|p|p.display().to_string()).unwrap_or_default()).size(12),
            button("Open library folder").on_press(Message::OpenLibrary),
        ]
        .spacing(12);

        settings.into()
    }
}
