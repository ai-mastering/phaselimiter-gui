#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod backend;
mod model;

use iced::widget::{
    Column, button, checkbox, column, container, pick_list, progress_bar, row, rule, scrollable,
    text, text_input,
};
use iced::{Element, Fill, Subscription, Task, Theme};
use model::{Format, Job, Settings, Solver, Status};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(Theme::Light)
        .window_size((1120.0, 820.0))
        .exit_on_close_request(false)
        .run()
}

#[derive(Debug, Clone, Copy)]
enum Field {
    Backend,
    Ffmpeg,
    Cache,
    OutputDir,
    Loudness,
    Intensity,
    Stereo,
    Ceiling,
    SampleRate,
    BitDepth,
    Workers,
    Evaluations,
    Seed,
}

#[derive(Debug, Clone)]
enum Message {
    Edit(Field, String),
    Input(String),
    AddInput,
    FileDropped(PathBuf),
    Hover(bool),
    Format(Format),
    Solver(Solver),
    PreserveBass(bool),
    PreCompression(bool),
    Start,
    Cancel,
    ClearFinished,
    Remove(usize),
    Select(usize),
    Backend(usize, backend::Update),
    Close,
}

struct Active {
    id: usize,
    cancel: Arc<AtomicBool>,
    _handle: iced::task::Handle,
}

#[derive(Default)]
struct App {
    settings: Settings,
    input: String,
    jobs: Vec<Job>,
    next_id: usize,
    selected: Option<usize>,
    active: Option<Active>,
    running: bool,
    closing: bool,
    hovering: bool,
    error: Option<String>,
}

impl App {
    fn title(&self) -> String {
        let queued = self
            .jobs
            .iter()
            .filter(|j| j.status == Status::Waiting)
            .count();
        let completed = self
            .jobs
            .iter()
            .filter(|j| j.status == Status::Succeeded)
            .count();
        format!("Phaselimiter | {queued} queued | {completed} completed")
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                Some(Message::FileDropped(path))
            }
            iced::Event::Window(iced::window::Event::FileHovered(_)) => Some(Message::Hover(true)),
            iced::Event::Window(iced::window::Event::FilesHoveredLeft) => {
                Some(Message::Hover(false))
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::Close),
            _ => None,
        })
    }

    fn add(&mut self, path: PathBuf) {
        self.hovering = false;
        let result = self
            .settings
            .parse()
            .and_then(|config| model::add_job(&mut self.jobs, path, config, self.next_id));
        match result {
            Ok(()) => {
                self.selected = Some(self.next_id);
                self.next_id += 1;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn start_next(&mut self) -> Task<Message> {
        if self.active.is_some() || !self.running {
            return Task::none();
        }
        let Some(job) = self.jobs.iter_mut().find(|j| j.status == Status::Waiting) else {
            self.running = false;
            return Task::none();
        };
        job.status = Status::Processing;
        self.selected = Some(job.id);
        let id = job.id;
        let cancel = Arc::new(AtomicBool::new(false));
        let (task, handle) = Task::run(backend::run(job.clone(), cancel.clone()), move |update| {
            Message::Backend(id, update)
        })
        .abortable();
        self.active = Some(Active {
            id,
            cancel,
            _handle: handle.abort_on_drop(),
        });
        task
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Edit(field, value) => *self.field_mut(field) = value,
            Message::Input(value) => self.input = value,
            Message::AddInput => {
                if !self.input.is_empty() {
                    self.add(PathBuf::from(&self.input));
                    if self.error.is_none() {
                        self.input.clear();
                    }
                }
            }
            Message::FileDropped(path) => self.add(path),
            Message::Hover(value) => self.hovering = value,
            Message::Format(value) => self.settings.format = value,
            Message::Solver(value) => self.settings.solver = value,
            Message::PreserveBass(value) => self.settings.preserve_bass = value,
            Message::PreCompression(value) => self.settings.pre_compression = value,
            Message::Start => {
                self.running = true;
                self.error = None;
                return self.start_next();
            }
            Message::Cancel => {
                self.running = false;
                if let Some(active) = &self.active {
                    active.cancel.store(true, Ordering::Relaxed);
                }
            }
            Message::ClearFinished => self
                .jobs
                .retain(|j| matches!(j.status, Status::Waiting | Status::Processing)),
            Message::Remove(id) => self
                .jobs
                .retain(|j| j.id != id || j.status == Status::Processing),
            Message::Select(id) => self.selected = Some(id),
            Message::Backend(id, update) => {
                if !self.active.as_ref().is_some_and(|active| active.id == id) {
                    return Task::none();
                }
                let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) else {
                    return Task::none();
                };
                let done = match update {
                    backend::Update::Line(line) => {
                        if job.log.len() == 200 {
                            job.log.remove(0);
                        }
                        job.log.push(line);
                        false
                    }
                    backend::Update::Progress(value) => {
                        job.progress = job.progress.max(value);
                        false
                    }
                    backend::Update::Finished(result) => {
                        match result {
                            Ok(()) => {
                                job.status = Status::Succeeded;
                                job.progress = 1.0;
                            }
                            Err(error) => {
                                job.status = Status::Failed;
                                job.log.push(error);
                            }
                        }
                        true
                    }
                    backend::Update::Cancelled => {
                        job.status = Status::Cancelled;
                        true
                    }
                };
                if done {
                    self.active = None;
                    if self.closing {
                        return iced::exit();
                    }
                    return self.start_next();
                }
            }
            Message::Close => {
                self.closing = true;
                self.running = false;
                if let Some(active) = &self.active {
                    active.cancel.store(true, Ordering::Relaxed);
                } else {
                    return iced::exit();
                }
            }
        }
        if self.running {
            self.start_next()
        } else {
            Task::none()
        }
    }

    fn field_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Backend => &mut self.settings.backend,
            Field::Ffmpeg => &mut self.settings.ffmpeg,
            Field::Cache => &mut self.settings.cache,
            Field::OutputDir => &mut self.settings.output_dir,
            Field::Loudness => &mut self.settings.loudness,
            Field::Intensity => &mut self.settings.intensity,
            Field::Stereo => &mut self.settings.stereo,
            Field::Ceiling => &mut self.settings.ceiling,
            Field::SampleRate => &mut self.settings.sample_rate,
            Field::BitDepth => &mut self.settings.bit_depth,
            Field::Workers => &mut self.settings.workers,
            Field::Evaluations => &mut self.settings.evaluations,
            Field::Seed => &mut self.settings.seed,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let header = row![
            column![
                text("PHASELIMITER").size(12),
                text("Audio mastering").size(30)
            ]
            .spacing(4),
            iced::widget::space().width(Fill),
            text("Automatic mastering · Offline").size(13)
        ]
        .align_y(iced::Center);

        let settings = column![
            text("Mastering settings").size(22),
            text("Settings are captured when files are added.").size(12),
            field(
                "Output directory",
                &self.settings.output_dir,
                Field::OutputDir
            ),
            row![
                field(
                    "Target loudness (LUFS)",
                    &self.settings.loudness,
                    Field::Loudness
                ),
                field(
                    "True-peak ceiling (dBTP)",
                    &self.settings.ceiling,
                    Field::Ceiling
                )
            ]
            .spacing(12),
            row![
                field(
                    "Mastering intensity (0–1)",
                    &self.settings.intensity,
                    Field::Intensity
                ),
                field(
                    "Stereo matching (0–1)",
                    &self.settings.stereo,
                    Field::Stereo
                )
            ]
            .spacing(12),
            checkbox(self.settings.preserve_bass)
                .label("Preserve bass")
                .on_toggle(Message::PreserveBass),
            checkbox(self.settings.pre_compression)
                .label("Pre-compression")
                .on_toggle(Message::PreCompression),
            rule::horizontal(1),
            text("Output").size(18),
            row![
                column![
                    text("Format").size(12),
                    pick_list(Format::ALL, Some(self.settings.format), Message::Format).width(Fill)
                ]
                .spacing(5),
                field(
                    "Sample rate (Hz)",
                    &self.settings.sample_rate,
                    Field::SampleRate
                ),
                field(
                    "WAV bits (32 = float)",
                    &self.settings.bit_depth,
                    Field::BitDepth
                )
            ]
            .spacing(12),
            rule::horizontal(1),
            text("Optimization").size(18),
            pick_list(Solver::ALL, Some(self.settings.solver), Message::Solver).width(Fill),
            row![
                field(
                    "Evaluation budget",
                    &self.settings.evaluations,
                    Field::Evaluations
                ),
                field("Seed", &self.settings.seed, Field::Seed)
            ]
            .spacing(12),
            field(
                "Worker threads (0 = automatic)",
                &self.settings.workers,
                Field::Workers
            ),
            rule::horizontal(1),
            text("Engine & resources").size(18),
            text("Use the backend's feature/codex build. The GUI can open without it.").size(12),
            field(
                "phase_limiter executable",
                &self.settings.backend,
                Field::Backend
            ),
            field(
                "FFmpeg executable or PATH name",
                &self.settings.ffmpeg,
                Field::Ffmpeg
            ),
            field("sound_quality2_cache", &self.settings.cache, Field::Cache),
        ]
        .spacing(12);

        let mut queue = Column::new().spacing(10);
        if self.jobs.is_empty() {
            queue = queue.push(
                container(
                    column![
                        text("Drop your audio files here").size(23),
                        text("Add multiple files, review settings, then start the queue.").size(14),
                        text("You can also paste a file path above.").size(13)
                    ]
                    .spacing(12),
                )
                .padding(30)
                .width(Fill)
                .style(container::rounded_box),
            );
        }
        for job in &self.jobs {
            let filename = job
                .input
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let status = if job.status == Status::Processing {
                format!("Processing · {:.0}%", job.progress * 100.0)
            } else {
                job.status.to_string()
            };
            let entry = column![
                row![
                    button(text(filename).size(16))
                        .style(button::text)
                        .on_press(Message::Select(job.id)),
                    iced::widget::space().width(Fill),
                    text(status).size(13),
                    button("Remove").style(button::text).on_press_maybe(
                        (job.status != Status::Processing).then_some(Message::Remove(job.id))
                    )
                ]
                .spacing(8)
                .align_y(iced::Center),
                text(job.input.display().to_string()).size(12),
                text(format!("→ {}", job.output.display())).size(12),
                progress_bar(0.0..=1.0, job.progress).girth(5),
            ]
            .spacing(8);
            queue = queue.push(
                container(entry)
                    .padding(14)
                    .width(Fill)
                    .style(container::rounded_box),
            );
        }
        let mut right = column![
            row![
                text_input("Paste an audio file path", &self.input)
                    .on_input(Message::Input)
                    .on_submit(Message::AddInput)
                    .padding(10),
                button("Add file").on_press(Message::AddInput).padding(10)
            ]
            .spacing(8),
            text(if self.hovering {
                "Release to add files"
            } else {
                "Files stay on your computer. Existing outputs are not overwritten."
            })
            .size(12),
            row![
                text(format!("Queue · {} files", self.jobs.len())).size(21),
                iced::widget::space().width(Fill),
                button("Clear finished")
                    .on_press(Message::ClearFinished)
                    .style(button::text)
            ]
            .align_y(iced::Center),
            scrollable(queue).height(Fill),
        ]
        .spacing(12);
        if let Some(error) = &self.error {
            right = right.push(
                text(error)
                    .color(iced::Color::from_rgb8(170, 40, 40))
                    .size(14),
            );
        }
        if let Some(job) = self.jobs.iter().find(|j| Some(j.id) == self.selected) {
            let log = if job.log.is_empty() {
                "Select a job to see its backend output. Processing requires the engine, cache and FFmpeg.".to_owned()
            } else {
                job.log
                    .iter()
                    .rev()
                    .take(12)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            right = right.push(text("Job log").size(15)).push(
                container(scrollable(text(log).size(12)).height(110))
                    .padding(10)
                    .width(Fill)
                    .style(container::rounded_box),
            );
        }
        let waiting = self.jobs.iter().any(|j| j.status == Status::Waiting);
        right = right.push(
            row![
                button(if self.closing {
                    "Closing…"
                } else {
                    "Start queue"
                })
                .on_press_maybe(
                    (waiting && self.active.is_none() && !self.closing).then_some(Message::Start)
                )
                .padding([12, 20]),
                button("Cancel current & pause")
                    .on_press_maybe(self.active.as_ref().map(|_| Message::Cancel))
                    .style(button::secondary)
                    .padding([12, 16]),
            ]
            .spacing(10),
        );

        container(
            column![
                header,
                rule::horizontal(1),
                row![
                    scrollable(settings).width(400).height(Fill),
                    container(right).width(Fill).height(Fill)
                ]
                .spacing(24)
            ]
            .spacing(20),
        )
        .padding(24)
        .height(Fill)
        .into()
    }
}

fn field<'a>(label: &'static str, value: &'a str, field: Field) -> Element<'a, Message> {
    column![
        text(label).size(12),
        text_input(label, value)
            .on_input(move |value| Message::Edit(field, value))
            .padding(8)
    ]
    .spacing(5)
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropped_file_is_queued_and_can_be_removed_through_the_ui() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("日本語 track.wav");
        std::fs::write(&input, b"audio").unwrap();
        let mut app = App::default();
        app.settings.output_dir = tmp.path().to_string_lossy().into_owned();
        let _ = app.update(Message::FileDropped(input.clone()));
        assert_eq!(app.jobs.len(), 1);
        assert_eq!(app.jobs[0].input, input.canonicalize().unwrap());
        assert_eq!(app.jobs[0].status, Status::Waiting);
        let messages = {
            let mut ui = iced_test::simulator(app.view());
            ui.find("日本語 track.wav")
                .expect("Dropped file is visible");
            ui.click("Remove").unwrap();
            ui.into_messages()
        };
        for message in messages {
            let _ = app.update(message);
        }
        assert!(app.jobs.is_empty());
    }

    #[test]
    fn invalid_settings_are_reported_without_queuing_a_file() {
        let mut app = App::default();
        app.settings.intensity = "NaN".into();
        let _ = app.update(Message::FileDropped(PathBuf::from("some.wav")));
        assert!(app.error.is_some());
        assert!(app.jobs.is_empty());
    }
}
