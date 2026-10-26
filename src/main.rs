#![deny(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use uuid::Uuid;
mod analysis;
mod analysis_cache;
mod backend;
mod comparison;
mod controls;
mod descriptions;
mod diagnostics;
mod labels;
mod library;
mod measurements;
mod model;
mod playback;
mod plots;
mod preparation;
mod reference;
mod theme;
mod ui;

use comparison::{ComparisonChoice, settings_differences};
use iced::widget::{
    button, canvas, checkbox, container, pick_list, progress_bar, row, rule, scrollable, slider,
    text, text_input,
};
use iced::{Fill, Subscription, Task};
use theme::Theme;
type Element<'a, Message> = iced::Element<'a, Message, Theme>;
use library::{Library, Track};
use model::{CeilingMode, Format, Job, NoiseMode, ReferenceMode, Settings, Solver, Status};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type AudioKey = (Uuid, Option<Uuid>);
fn main() -> iced::Result {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        playback::log(&format!("Rust panic: {panic}"));
        previous_hook(panic);
    }));
    iced::application(App::load, App::update, App::view)
        .title("Phaselimiter")
        .subscription(App::subscription)
        .theme(|_: &App| Theme)
        .window_size((1280.0, 800.0))
        .exit_on_close_request(false)
        .run()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Field {
    MaxIter2,
    MaxIter1,
    Oversample,
    LowpassCut,
    HighCut,
    LowCut,
    Backend,
    Ffmpeg,
    Cache,
    ExportDir,
    Loudness,
    Intensity,
    Ceiling,
    SampleRate,
    BitDepth,
    Workers,
    Evaluations,
    Seed,
    Reference,
}
#[derive(Debug, Clone)]
enum Message {
    NoiseMode(NoiseMode),
    CeilingMode(CeilingMode),
    ReferenceMode(ReferenceMode),
    Mastering(bool),
    Edit(Field, String),
    Adjust(Field, bool),
    CommitNumber(Field),
    RevealNumber(Field, Option<f32>),
    NumberFocused(Field, String, bool),
    Input(String),
    AddInput,
    BrowseAudio,
    AudioPicked(Option<Vec<PathBuf>>),
    ToggleAudioPath,
    FileDropped(PathBuf),
    Hover(bool),
    Imported(Result<Track, String>),
    Format(Format),
    Solver(Solver),
    PreserveBass(bool),
    PreCompression(bool),
    ReferenceEnabled(bool),
    ReferencePath(String),
    ReferenceSubmit,
    BrowseReference,
    ReferencePicked(u64, Option<PathBuf>),
    DraftReferenceAnalyzed(u64, Result<reference::Prepared, String>),
    ReferenceFocused(bool),
    Compose,
    ShowAnalysis,
    ShowMastering,
    ToggleLimiterDetails,
    ToggleOptimization,
    ToggleHelp(descriptions::Topic),
    ToggleAppSettings,
    BrowseSettingPath(Field),
    SettingPathPicked(Field, Option<PathBuf>),
    Compare(ComparisonChoice),
    ReferenceLoaded(PathBuf, Result<analysis::Analysis, String>),
    EditMemo(Uuid, String),
    SaveMemo,
    Master,
    Start,
    Cancel,
    Remove(AudioKey),
    Select(AudioKey),
    Backend(Uuid, backend::Update),
    Completed(Uuid, Result<String, String>),
    Analyzed(AudioKey, Result<analysis::Analysis, String>),
    Analyze,
    Side(bool),
    Play,
    Seek(f64),
    SeekFraction(f32),
    Region(f32, f32),
    ClearRegion,
    Equalize(bool),
    Tick,
    Prepared(PathBuf, Result<Arc<playback::Decoded>, String>),
    Graph(plots::Kind),
    Details,
    CopyCommand(Uuid),
    CopyLog(Uuid),
    CopyText(String),
    SelectDiagnostic(String, iced::widget::text_editor::Action),
    UseBundledTools,
    StorageSize(u64),
    OpenLibrary,
    FolderOpened(Result<(), String>),
    Export,
    Exported(Result<PathBuf, String>),
    Close,
}
struct Active {
    id: Uuid,
    cancel: Arc<AtomicBool>,
    _handle: iced::task::Handle,
}
struct App {
    library: Library,
    root: Option<PathBuf>,
    // Drop the save worker before releasing the library lock.
    save_worker: Option<library::SaveWorker>,
    _library_lock: Option<std::fs::File>,
    input: String,
    show_audio_path: bool,
    selected: Option<AudioKey>,
    active: Option<Active>,
    running: bool,
    closing: bool,
    hovering: bool,
    error: Option<String>,
    notice: Option<String>,
    show_settings: bool,
    show_app_settings: bool,
    comparison_job: Option<Uuid>,
    reference_report: Option<(PathBuf, Result<analysis::Analysis, String>)>,
    reference_pending: Option<PathBuf>,
    draft_reference: reference::State,
    force_analysis: HashSet<AudioKey>,

    show_details: bool,
    show_limiter_details: bool,
    show_optimization: bool,
    open_help: Option<descriptions::Topic>,
    output_side: bool,
    graph: plots::Kind,
    analysis_queue: VecDeque<AudioKey>,
    analyzing: Option<AudioKey>,
    importing: usize,
    finalizing: HashSet<Uuid>,
    player: playback::Player,
    prepared: preparation::Cache,
    loading: bool,
    loaded: bool,
    active_key: Option<AudioKey>,
    equalize: bool,
    region: Option<(f64, f64)>,
    position: f64,
    duration: f64,
    storage_bytes: u64,
    memo_save_at: Option<std::time::Instant>,
    diagnostics: Vec<diagnostics::Diagnostic>,
    numeric_edits: HashMap<Field, String>,
    numeric_errors: HashMap<Field, String>,
}
impl Default for App {
    fn default() -> Self {
        let library = Library::default();
        Self {
            library,
            root: None,
            save_worker: None,
            _library_lock: None,
            input: String::new(),
            show_audio_path: false,
            selected: None,
            active: None,
            running: false,
            closing: false,
            hovering: false,
            error: None,
            notice: None,
            show_settings: false,
            show_app_settings: false,
            comparison_job: None,
            reference_report: None,
            reference_pending: None,
            draft_reference: reference::State::default(),
            force_analysis: HashSet::new(),
            show_details: false,
            show_limiter_details: false,
            show_optimization: false,
            open_help: None,
            output_side: false,
            graph: plots::Kind::Spectrum,
            analysis_queue: VecDeque::new(),
            analyzing: None,
            importing: 0,
            player: playback::Player::default(),
            prepared: preparation::Cache::default(),
            finalizing: HashSet::new(),
            loading: false,
            loaded: false,
            active_key: None,
            equalize: true,
            region: None,
            position: 0.,
            duration: 0.,
            storage_bytes: 0,
            memo_save_at: None,
            diagnostics: Vec::new(),
            numeric_edits: HashMap::new(),
            numeric_errors: HashMap::new(),
        }
    }
}
impl App {
    fn load() -> (Self, Task<Message>) {
        let mut app = Self::default();
        match library::default_root().and_then(|root| {
            let lock = library::lock(&root)?;
            let library = Library::load(&root)?;
            let worker = library::SaveWorker::new(root.clone())?;
            Ok((root, library, lock, worker))
        }) {
            Ok((root, library, lock, worker)) => {
                app.save_worker = Some(worker);
                app._library_lock = Some(lock);
                app.selected = library.selected;
                app.comparison_job = library.comparison_job;
                app.output_side = library
                    .output_side
                    .unwrap_or_else(|| library.selected.is_some_and(|key| key.1.is_some()));
                app.equalize = library.equalize.unwrap_or(true);
                app.library = library;
                app.root = Some(root);
                for t in &app.library.tracks {
                    if t.analysis.is_none() && t.analysis_error.is_none() {
                        app.analysis_queue.push_back((t.id, None));
                    }
                }
                for j in &app.library.jobs {
                    if j.status == Status::Succeeded
                        && j.analysis.is_none()
                        && j.analysis_error.is_none()
                    {
                        app.analysis_queue.push_back((j.track_id, Some(j.id)));
                    }
                }
            }
            Err(error) => {
                app.error = Some(format!(
                    "{error} Existing library data has not been changed."
                ))
            }
        }
        app.invalidate_draft_reference(true);
        let task = app.prepare();
        let analysis = app.start_analysis();
        let storage = app.refresh_storage();
        let reference = app.load_reference();
        app.refresh_diagnostics();
        (app, Task::batch([task, analysis, storage, reference]))
    }
    fn library_folder(&self) -> Result<PathBuf, String> {
        self.root
            .clone()
            .map(Ok)
            .unwrap_or_else(library::default_root)
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                    Some(Message::FileDropped(path))
                }
                iced::Event::Window(iced::window::Event::FileHovered(_)) => {
                    Some(Message::Hover(true))
                }
                iced::Event::Window(iced::window::Event::FilesHoveredLeft) => {
                    Some(Message::Hover(false))
                }
                iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::Close),
                _ => None,
            }),
            iced::time::every(std::time::Duration::from_millis(100)).map(|_| Message::Tick),
        ])
    }
    fn refresh_storage(&self) -> Task<Message> {
        let Some(root) = self.root.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || library::usage(&root))
                    .await
                    .unwrap_or(0)
            },
            Message::StorageSize,
        )
    }
    fn save(&mut self) -> bool {
        self.memo_save_at = None;
        self.library.selected = self.selected;
        self.library.output_side = Some(self.output_side);
        self.library.equalize = Some(self.equalize);
        self.library.comparison_job = self.comparison_job;
        let result = if let Some(worker) = &self.save_worker {
            worker.save(self.library.clone())
        } else if let Some(root) = &self.root {
            self.library.save(root)
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.error = Some(format!("Could not save history: {error}"));
            return false;
        }
        true
    }
    fn save_sync(&mut self) -> bool {
        if !self.save() {
            return false;
        }
        if let Some(worker) = &self.save_worker
            && let Err(error) = worker.flush()
        {
            self.error = Some(format!("Could not save history: {error}"));
            return false;
        }
        true
    }
    fn track(&self) -> Option<&Track> {
        let id = self.selected?.0;
        self.library.tracks.iter().find(|t| t.id == id)
    }
    fn job(&self) -> Option<&Job> {
        let id = self.selected?.1?;
        self.library.jobs.iter().find(|j| j.id == id)
    }
    fn audio(&self, key: AudioKey) -> Option<(PathBuf, String)> {
        if let Some(id) = key.1 {
            let job = self.library.jobs.iter().find(|j| j.id == id)?;
            Some((job.output.clone(), job.output_hash.clone()?))
        } else {
            let track = self.library.tracks.iter().find(|t| t.id == key.0)?;
            Some((track.input.clone(), track.hash.clone()))
        }
    }
    fn import(&mut self, path: PathBuf) -> Task<Message> {
        self.hovering = false;
        let Some(root) = self.root.clone() else {
            self.error = Some("The data directory is unavailable.".into());
            return Task::none();
        };
        let id = Uuid::new_v4();
        self.importing += 1;
        self.save();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || library::import(&root, path, id))
                    .await
                    .map_err(|e| e.to_string())?
            },
            Message::Imported,
        )
    }
    fn start_analysis(&mut self) -> Task<Message> {
        if self.analyzing.is_some()
            || self.closing
            || self.draft_reference.active
            || (self.library.draft.use_reference && self.draft_reference.due.is_some())
        {
            return Task::none();
        }
        let Some(root) = self.root.clone() else {
            return Task::none();
        };
        while let Some(key) = self.analysis_queue.pop_front() {
            let Some((input, hash)) = self.audio(key) else {
                continue;
            };
            self.analyzing = Some(key);
            let settings = self.library.app_settings.clone();
            return Task::perform(
                analysis::run_with_options(
                    root,
                    input,
                    hash,
                    settings.backend_path(),
                    settings.ffmpeg_command(),
                    settings.cache_path(),
                    analysis::Options {
                        force: self.force_analysis.remove(&key),
                        ..analysis::Options::FULL
                    },
                ),
                move |result| Message::Analyzed(key, result),
            );
        }
        Task::none()
    }
    fn start_next(&mut self) -> Task<Message> {
        if self.active.is_some() || !self.running || self.closing {
            return Task::none();
        }
        let Some(job) = self
            .library
            .jobs
            .iter_mut()
            .find(|j| j.status == Status::Waiting)
        else {
            self.running = false;
            return Task::none();
        };
        job.status = Status::Processing;
        let id = job.id;
        let cancel = Arc::new(AtomicBool::new(false));
        let (task, handle) = Task::run(backend::run(job.clone(), cancel.clone()), move |u| {
            Message::Backend(id, u)
        })
        .abortable();
        self.active = Some(Active {
            id,
            cancel,
            _handle: handle.abort_on_drop(),
        });
        self.save();
        task
    }
    fn queue_master(&mut self) -> Result<(), String> {
        if let Some((_, reason)) = self.settings_issue() {
            return Err(reason);
        }
        let id = self
            .selected
            .map(|key| key.0)
            .ok_or("Select an input first.")?;
        let track = self
            .library
            .tracks
            .iter()
            .find(|t| t.id == id)
            .ok_or("Select an input first.")?
            .clone();
        let root = self.root.as_ref().ok_or("Data directory unavailable.")?;
        let mut config = self.library.draft.parse()?;
        if self.library.draft.mastering && self.library.draft.use_reference {
            config.reference = Some(
                self.ready_reference()
                    .map_err(|e| format!("Reference: {e}"))?
                    .to_path_buf(),
            );
            let prepared = self
                .draft_reference
                .report
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .ok_or("Reference: analysis is not ready.")?;
            library::verify(
                std::path::Path::new(&self.library.draft.reference_audio),
                &prepared.source_hash,
            )
            .map_err(|e| format!("Reference: {e}"))?;
        }
        if let Some(reference) = &config.reference {
            let expected = reference
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Reference: analysis has no content hash.")?;
            library::verify(reference, expected).map_err(|e| format!("Reference: {e}"))?;
        }
        let id = Uuid::new_v4();

        let dir = root.join("jobs").join(id.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let output = dir.join("output").with_extension(config.format.extension());
        let reference_hash = config
            .reference
            .as_ref()
            .map(|p| library::hash(p))
            .transpose()?;
        self.library.jobs.push(Job {
            id,
            created_at: Some(chrono::Utc::now()),
            memo: String::new(),
            track_id: track.id,
            input: track.input,
            output,
            config,
            tools: self.library.app_settings.resolved_tools(),
            status: Status::Waiting,
            progress: 0.,
            log: Vec::new(),
            output_hash: None,
            input_hash: Some(track.hash),
            reference_hash,
            analysis: None,
            analysis_error: None,
            limiting_error: None,
            command: None,
        });
        self.running = true;
        self.notice = Some("Mastering queued. Playback selection is unchanged.".into());
        Ok(())
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut save = false;
        match message {
            Message::Edit(field, value) => {
                if !controls::enabled(field, &self.library.draft) {
                    return Task::none();
                }
                if let Some(spec) = controls::spec(field, &self.library.draft) {
                    if spec.partial(&value) {
                        self.numeric_edits.insert(field, value);
                        self.numeric_errors.remove(&field);
                        return Task::none();
                    }
                    if !spec.valid(&value) {
                        self.numeric_errors
                            .insert(field, format!("Input rejected. {}", spec.hint()));
                        return Task::none();
                    }
                    self.numeric_edits.remove(&field);
                } else if !controls::valid_choice(field, &value, &self.library.draft) {
                    return Task::none();
                }
                self.numeric_errors.remove(&field);
                *self.field_mut(field) = value;
                save = true;
            }
            Message::Adjust(field, up) => {
                if controls::enabled(field, &self.library.draft)
                    && let Some(spec) = controls::spec(field, &self.library.draft)
                    && let Some(value) = spec.adjust(self.field_value(field), up)
                {
                    self.numeric_edits.remove(&field);
                    self.numeric_errors.remove(&field);
                    *self.field_mut(field) = value;
                    save = true;
                }
            }
            Message::CommitNumber(field) => {
                self.numeric_edits.remove(&field);
                self.numeric_errors.remove(&field);
            }
            Message::NumberFocused(field, buffer, focused) => {
                if !focused
                    && controls::enabled(field, &self.library.draft)
                    && self.numeric_edits.get(&field) == Some(&buffer)
                    && let Some(spec) = controls::spec(field, &self.library.draft)
                {
                    self.numeric_errors.insert(field, spec.hint());
                }
            }
            Message::RevealNumber(field, offset) => {
                if let Some(y) = offset {
                    tasks.push(iced::widget::operation::scroll_to(
                        "mastering-fields",
                        iced::widget::operation::AbsoluteOffset { x: 0., y },
                    ));
                }
                tasks.push(iced::widget::operation::focus(
                    if field == Field::Reference {
                        "reference-path".into()
                    } else {
                        format!("number-{field:?}")
                    },
                ));
            }
            Message::Input(value) => self.input = value,
            Message::ToggleAudioPath => self.show_audio_path = !self.show_audio_path,
            Message::BrowseAudio => tasks.push(Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose audio files")
                        .pick_files()
                        .await
                        .map(|files| {
                            files
                                .into_iter()
                                .map(|file| file.path().to_path_buf())
                                .collect()
                        })
                },
                Message::AudioPicked,
            )),
            Message::AudioPicked(paths) => {
                if let Some(paths) = paths {
                    for path in paths {
                        tasks.push(self.import(path));
                    }
                }
            }
            Message::AddInput => {
                if !self.input.is_empty() {
                    let path = PathBuf::from(std::mem::take(&mut self.input));
                    tasks.push(self.import(path));
                }
            }
            Message::FileDropped(path) => tasks.push(self.import(path)),
            Message::Hover(value) => self.hovering = value,
            Message::Imported(result) => {
                self.importing = self.importing.saturating_sub(1);
                match result {
                    Ok(track) => {
                        let key = (track.id, None);
                        let warm = self.request_decode(track.input.clone(), track.hash.clone());
                        tasks.push(warm);
                        self.library.tracks.push(track);
                        self.analysis_queue.push_back(key);
                        self.notice =
                            Some("Audio added. Select it in the history to listen.".into());
                        if self.selected.is_none() {
                            self.selected = Some(key);
                            tasks.push(self.prepare());
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
                save = true;
            }
            Message::Mastering(v) => {
                self.library.draft.mastering = v;
                self.invalidate_draft_reference(true);
                self.numeric_edits.clear();
                self.numeric_errors.clear();
                save = true;
            }
            Message::ReferenceMode(v) => {
                self.library.draft.reference_mode = v;
                self.numeric_edits.clear();
                self.numeric_errors.clear();
                save = true;
            }
            Message::CeilingMode(v) => {
                self.library.draft.ceiling_mode = v;
                self.numeric_edits.clear();
                self.numeric_errors.clear();
                save = true;
            }
            Message::NoiseMode(v) => {
                self.library.draft.noise_mode = v;
                save = true;
            }
            Message::Format(v) => {
                self.library.draft.format = v;
                self.numeric_edits.clear();
                self.numeric_errors.clear();
                let rates = model::sample_rates(v);
                let rate = self
                    .library
                    .draft
                    .sample_rate
                    .parse::<u32>()
                    .unwrap_or(44100);
                if !rates.contains(&rate) {
                    self.library.draft.sample_rate = rates
                        .iter()
                        .min_by_key(|r| r.abs_diff(rate))
                        .copied()
                        .unwrap_or(44100)
                        .to_string();
                }
                save = true;
            }
            Message::Solver(v) => {
                if !self.library.draft.mastering {
                    return Task::none();
                }
                self.library.draft.solver = v;
                save = true;
            }
            Message::PreserveBass(v) => {
                self.library.draft.preserve_bass = v;
                save = true;
            }
            Message::PreCompression(v) => {
                self.library.draft.pre_compression = v;
                save = true;
            }
            Message::ReferenceEnabled(enabled) => {
                if !self.library.draft.mastering {
                    return Task::none();
                }
                self.numeric_errors.remove(&Field::Reference);
                self.library.draft.use_reference = enabled;
                self.library.draft.reference.clear();
                self.invalidate_draft_reference(true);

                save = true;
            }
            Message::ReferencePath(path) => {
                if !self.library.draft.mastering || path.is_empty() {
                    return Task::none();
                }
                self.numeric_errors.remove(&Field::Reference);
                self.library.draft.reference_audio = path;
                self.library.draft.reference.clear();
                self.invalidate_draft_reference(false);
                save = true;
            }
            Message::ReferenceSubmit => {
                if self.draft_reference.due.is_some()
                    || (!self.draft_reference.active && self.ready_reference().is_err())
                {
                    self.draft_reference.due = Some(std::time::Instant::now());
                }
            }
            Message::ReferenceFocused(focused) => {
                if self.draft_reference.focused && !focused && self.draft_reference.due.is_some() {
                    self.draft_reference.due = Some(std::time::Instant::now());
                }
                self.draft_reference.focused = focused;
            }
            Message::BrowseReference => {
                if !self.library.draft.mastering {
                    return Task::none();
                }
                self.numeric_errors.remove(&Field::Reference);
                let generation = self.draft_reference.generation;
                tasks.push(Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose reference audio")
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |path| Message::ReferencePicked(generation, path),
                ));
            }
            Message::ReferencePicked(generation, path) => {
                if generation == self.draft_reference.generation
                    && let Some(path) = path
                {
                    self.library.draft.reference_audio = path.display().to_string();
                    self.library.draft.reference.clear();
                    self.invalidate_draft_reference(true);
                    save = true;
                }
            }
            Message::DraftReferenceAnalyzed(generation, result) => {
                self.draft_reference.active = false;
                self.draft_reference.cancel = None;
                if generation == self.draft_reference.generation {
                    self.numeric_errors.remove(&Field::Reference);
                    self.draft_reference.report = Some(result);
                }
            }
            Message::Compose => {
                self.show_settings = true;
                self.show_app_settings = false;
            }
            Message::ShowAnalysis => {
                self.show_settings = false;
                self.show_app_settings = false;
            }
            Message::ShowMastering => {
                self.show_settings = true;
                self.show_app_settings = false;
            }
            Message::BrowseSettingPath(field) => {
                let title = match field {
                    Field::Backend => "Choose phase_limiter executable",
                    Field::Ffmpeg => "Choose FFmpeg executable",
                    Field::Cache => "Choose sound_quality2_cache",
                    Field::ExportDir => "Choose default export directory",
                    _ => return Task::none(),
                };
                let current = PathBuf::from(self.field_value(field));
                tasks.push(Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
                        if current.is_dir() {
                            dialog = dialog.set_directory(&current);
                        } else if let Some(parent) = current.parent().filter(|p| p.is_dir()) {
                            dialog = dialog.set_directory(parent);
                        }
                        let picked = if field == Field::ExportDir {
                            dialog.pick_folder().await
                        } else {
                            dialog.pick_file().await
                        };
                        picked.map(|file| file.path().to_path_buf())
                    },
                    move |path| Message::SettingPathPicked(field, path),
                ));
            }
            Message::SettingPathPicked(field, path) => {
                if matches!(
                    field,
                    Field::Backend | Field::Ffmpeg | Field::Cache | Field::ExportDir
                ) && let Some(path) = path
                {
                    *self.field_mut(field) = path.to_string_lossy().into_owned();
                    save = true;
                }
            }
            Message::ToggleAppSettings => {
                self.show_app_settings = !self.show_app_settings;
                tasks.push(self.refresh_storage());
            }
            Message::Compare(choice) => {
                self.comparison_job = choice.job;
                if !self.output_side {
                    tasks.push(self.prepare());
                }
                save = true;
            }
            Message::ReferenceLoaded(path, result) => {
                if self.reference_pending.as_ref() == Some(&path) {
                    self.reference_pending = None;
                    self.reference_report = Some((path, result));
                }
            }
            Message::ToggleLimiterDetails => {
                self.show_limiter_details = !self.show_limiter_details;
            }
            Message::ToggleHelp(topic) => {
                self.open_help = if self.open_help == Some(topic) {
                    None
                } else {
                    Some(topic)
                };
            }
            Message::ToggleOptimization => {
                self.show_optimization = !self.show_optimization;
            }
            Message::Master => {
                if let Some(field) = controls::first_invalid(self) {
                    tasks.push(self.reveal_setting(field));
                } else {
                    match self.queue_master() {
                        Ok(()) => {
                            save = true;
                            self.error = None;
                            self.numeric_errors.clear();
                        }
                        Err(error) => {
                            if error.starts_with("Reference:") {
                                self.numeric_errors.insert(Field::Reference, error);
                                tasks.push(self.reveal_setting(Field::Reference));
                            } else {
                                self.error = Some(error);
                            }
                        }
                    }
                }
            }
            Message::Start => {
                self.running = true;
            }
            Message::Cancel => {
                self.running = false;
                if let Some(active) = &self.active {
                    active.cancel.store(true, Ordering::Relaxed);
                }
            }
            Message::Select(key) => {
                if self.memo_save_at.is_some() {
                    self.save();
                }
                if self.selected.map(|k| k.0) != Some(key.0)
                    || key.1.is_none()
                    || self.comparison_job == key.1
                {
                    self.comparison_job = None;
                }
                self.selected = Some(key);
                if let Some(job) = self.job() {
                    self.library.draft = settings_from_config(&job.config);
                    self.numeric_edits.clear();
                    self.numeric_errors.clear();
                    self.invalidate_draft_reference(true);
                }
                self.output_side = key.1.is_some();
                self.error = None;
                self.notice = None;
                tasks.push(self.prepare());
                tasks.push(self.load_reference());
                save = true;
            }
            Message::Remove(key) => {
                let busy = self
                    .library
                    .jobs
                    .iter()
                    .any(|j| j.track_id == key.0 && j.status == Status::Processing);
                if busy {
                    self.error =
                        Some("Cancel the active mastering before removing this audio.".into());
                } else {
                    if key.1.is_none()
                        && let Some(track) = self.library.tracks.iter().find(|t| t.id == key.0)
                    {
                        let reference = track
                            .analysis
                            .as_ref()
                            .and_then(|a| a.path())
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        if self.library.draft.reference == reference {
                            self.library.draft.reference.clear();
                        }
                    }
                    let removed_jobs: Vec<_> = self
                        .library
                        .jobs
                        .iter()
                        .filter(|j| {
                            if let Some(id) = key.1 {
                                j.id == id
                            } else {
                                j.track_id == key.0
                            }
                        })
                        .map(|j| j.id)
                        .collect();
                    if let Some(id) = key.1 {
                        self.library.jobs.retain(|j| j.id != id);
                        if self.comparison_job == Some(id) {
                            self.comparison_job = None;
                            tasks.push(self.prepare());
                        }
                    } else {
                        self.library.jobs.retain(|j| j.track_id != key.0);
                        self.library.tracks.retain(|t| t.id != key.0);
                    }
                    if self
                        .selected
                        .is_some_and(|s| s == key || (key.1.is_none() && s.0 == key.0))
                    {
                        self.selected = None;
                        self.player.pause();
                        self.loaded = false;
                        self.active_key = None;
                        self.loading = false;
                    }
                    // Save the manifest before removing the owned files.
                    if !self.save_sync() {
                        return Task::none();
                    }
                    if let Some(root) = &self.root {
                        for id in removed_jobs {
                            if key.1.is_none() {
                                let _ =
                                    std::fs::remove_dir_all(root.join("jobs").join(id.to_string()));
                            }
                        }
                        let dir = if let Some(id) = key.1 {
                            root.join("jobs").join(id.to_string())
                        } else {
                            root.join("audio").join(key.0.to_string())
                        };
                        if let Err(e) = std::fs::remove_dir_all(dir) {
                            self.error = Some(format!(
                                "History removed; could not remove stored files: {e}"
                            ));
                        }
                    }
                    save = true;
                }
            }
            Message::CopyCommand(id) => {
                if let Some(command) = self
                    .library
                    .jobs
                    .iter()
                    .find(|job| job.id == id)
                    .and_then(|job| job.command.clone())
                {
                    tasks.push(iced::clipboard::write(command));
                }
            }
            Message::CopyLog(id) => {
                if let Some(job) = self.library.jobs.iter().find(|job| job.id == id) {
                    tasks.push(iced::clipboard::write(job.log.join("\n")));
                }
            }
            Message::CopyText(value) => tasks.push(iced::clipboard::write(value)),
            Message::UseBundledTools => {
                self.library.app_settings.backend.clear();
                self.library.app_settings.ffmpeg.clear();
                self.library.app_settings.cache.clear();
                save = true;
            }
            Message::SelectDiagnostic(key, action) => {
                if !action.is_edit()
                    && let Some(diagnostic) = self.diagnostics.iter_mut().find(|d| d.text == key)
                {
                    diagnostic.content.perform(action);
                }
            }
            Message::Backend(id, update) => {
                if !self.active.as_ref().is_some_and(|a| a.id == id) {
                    return Task::none();
                }
                if let Some(job) = self.library.jobs.iter_mut().find(|j| j.id == id) {
                    match update {
                        backend::Update::Command(command) => {
                            job.command = Some(command);
                            save = true;
                        }
                        backend::Update::Line(line) => {
                            job.log.push(line);
                        }
                        backend::Update::Progress(p) => job.progress = job.progress.max(p),
                        backend::Update::LimitingError(value) => job.limiting_error = Some(value),
                        backend::Update::Cancelled => {
                            job.status = Status::Cancelled;
                            self.active = None;
                            save = true;
                        }
                        backend::Update::Finished(result) => {
                            self.active = None;
                            save = true;
                            match result {
                                Ok(()) => {
                                    self.finalizing.insert(id);
                                    let path = job.output.clone();
                                    tasks.push(Task::perform(
                                        async move {
                                            tokio::task::spawn_blocking(move || {
                                                library::hash(&path)
                                            })
                                            .await
                                            .map_err(|e| e.to_string())?
                                        },
                                        move |r| Message::Completed(id, r),
                                    ));
                                }
                                Err(e) => {
                                    job.status = Status::Failed;
                                    job.log.push(e);
                                }
                            }
                        }
                    }
                }
            }
            Message::Completed(id, result) => {
                self.finalizing.remove(&id);
                if let Some(job) = self.library.jobs.iter_mut().find(|j| j.id == id) {
                    match result {
                        Ok(hash) => {
                            job.status = Status::Succeeded;
                            job.progress = 1.;
                            job.output_hash = Some(hash);
                            self.analysis_queue.push_back((job.track_id, Some(id)));
                            self.notice =
                                Some("Mastering complete. Select the result to compare.".into());
                        }
                        Err(e) => {
                            job.status = Status::Failed;
                            job.log.push(e);
                        }
                    }
                }
                tasks.push(self.warm_selection());
                save = true;
            }
            Message::Analyzed(key, result) => {
                self.analyzing = None;
                let fields = if let Some(id) = key.1 {
                    self.library
                        .jobs
                        .iter_mut()
                        .find(|j| j.id == id)
                        .map(|j| (&mut j.analysis, &mut j.analysis_error))
                } else {
                    self.library
                        .tracks
                        .iter_mut()
                        .find(|t| t.id == key.0)
                        .map(|t| (&mut t.analysis, &mut t.analysis_error))
                };
                if let Some((analysis, error)) = fields {
                    match result {
                        Ok(a) => {
                            *analysis = Some(a);
                            *error = None;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
                self.update_gain();
                save = true;
            }
            Message::Analyze => {
                if self.library.draft.use_reference {
                    self.draft_reference.force = true;
                    self.invalidate_draft_reference(true);
                }
                self.reference_report = None;
                tasks.push(self.load_reference());
                if let Some((id, job)) = self.selected {
                    for key in [(id, None), (id, job)] {
                        if !self.analysis_queue.contains(&key) && self.analyzing != Some(key) {
                            self.force_analysis.insert(key);
                            self.analysis_queue.push_back(key);
                        }
                    }
                }
            }
            Message::Side(output) => {
                self.output_side = output;
                tasks.push(self.prepare());
                save = true;
            }
            Message::Play => {
                if self.player.status().2 {
                    self.player.pause();
                } else if self.loaded {
                    match self.player.play() {
                        Ok(()) => {}
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            Message::Seek(seconds) => self.player.seek(seconds),
            Message::SeekFraction(fraction) => self.player.seek(self.duration * fraction as f64),
            Message::Region(a, b) => {
                self.region = Some((self.duration * a as f64, self.duration * b as f64));
                self.player.region(self.region);
            }
            Message::ClearRegion => {
                self.region = None;
                self.player.region(None);
            }
            Message::Equalize(value) => {
                self.equalize = value;
                self.update_gain();
                save = true;
            }
            Message::EditMemo(id, memo) => {
                if let Some(job) = self.library.jobs.iter_mut().find(|job| job.id == id) {
                    job.memo = memo;
                    self.memo_save_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(400));
                }
            }
            Message::SaveMemo => save = true,
            Message::Tick => {
                if self
                    .memo_save_at
                    .is_some_and(|deadline| std::time::Instant::now() >= deadline)
                {
                    save = true;
                }
                for (&field, buffer) in &self.numeric_edits {
                    if controls::enabled(field, &self.library.draft)
                        && !self.numeric_errors.contains_key(&field)
                    {
                        let buffer = buffer.clone();
                        tasks.push(
                            iced::widget::operation::is_focused(format!("number-{field:?}")).map(
                                move |focused| {
                                    Message::NumberFocused(field, buffer.clone(), focused)
                                },
                            ),
                        );
                    }
                }
                if self.library.draft.use_reference {
                    tasks.push(
                        iced::widget::operation::is_focused("reference-path")
                            .map(Message::ReferenceFocused),
                    );
                }
                if let Some(worker) = &self.save_worker
                    && let Some(error) = worker.error()
                {
                    self.error = Some(format!("Could not save history: {error}"));
                }
                let (p, d, _) = self.player.status();
                self.position = p;
                self.duration = d;
                if let Some(e) = self.player.take_error() {
                    self.error = Some(e)
                }
            }
            Message::Prepared(path, result) => {
                self.preparation_finished(path, result);
            }
            Message::Graph(kind) => self.graph = kind,
            Message::Details => self.show_details = !self.show_details,
            Message::StorageSize(bytes) => self.storage_bytes = bytes,
            Message::OpenLibrary => match self.library_folder() {
                Ok(root) => tasks.push(Task::perform(
                    library::open_folder(root),
                    Message::FolderOpened,
                )),
                Err(error) => self.error = Some(error),
            },
            Message::FolderOpened(result) => {
                if let Err(e) = result {
                    self.error = Some(e);
                }
            }
            Message::Export => {
                if let Some(job) = self.job()
                    && job.status == Status::Succeeded
                    && let Some(hash) = job.output_hash.clone()
                {
                    let source = job.output.clone();
                    let dest = PathBuf::from(self.default_export(job));
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                library::export(&source, &hash, &dest)
                            })
                            .await
                            .map_err(|e| e.to_string())?
                        },
                        Message::Exported,
                    ));
                }
            }
            Message::Exported(result) => match result {
                Ok(path) => self.notice = Some(format!("Exported to {}", path.display())),
                Err(e) => self.error = Some(e),
            },
            Message::Close => {
                self.closing = true;
                if let Some(cancel) = &self.draft_reference.cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
                self.running = false;
                self.player.pause();
                self.save();
                if let Some(active) = &self.active {
                    active.cancel.store(true, Ordering::Relaxed);
                }
            }
        }
        if save {
            self.save();
        }
        if self.closing
            && self.active.is_none()
            && self.finalizing.is_empty()
            && self.importing == 0
            && !self.draft_reference.active
        {
            if self.save_sync() {
                return iced::exit();
            }
            self.closing = false;
        }
        tasks.push(self.start_next());
        tasks.push(self.start_draft_reference());
        tasks.push(self.start_analysis());
        self.refresh_diagnostics();
        Task::batch(tasks)
    }
    fn default_export(&self, job: &Job) -> String {
        let name = self
            .library
            .tracks
            .iter()
            .find(|t| t.id == job.track_id)
            .map(|t| {
                std::path::Path::new(&t.name)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| "audio".into());
        self.library
            .app_settings
            .export_directory()
            .join(format!(
                "{name}_master_{}.{}",
                job.created_at.map_or_else(
                    || "undated".into(),
                    |t| t
                        .with_timezone(&chrono::Local)
                        .format("%Y%m%d_%H%M%S")
                        .to_string()
                ),
                job.config.format.extension()
            ))
            .display()
            .to_string()
    }
    fn field_value(&self, field: Field) -> &str {
        match field {
            Field::LowCut => &self.library.draft.low_cut,
            Field::HighCut => &self.library.draft.high_cut,
            Field::LowpassCut => &self.library.draft.lowpass_cut,
            Field::Oversample => &self.library.draft.oversample,
            Field::MaxIter1 => &self.library.draft.max_iter1,
            Field::MaxIter2 => &self.library.draft.max_iter2,
            Field::Backend => &self.library.app_settings.backend,
            Field::Ffmpeg => &self.library.app_settings.ffmpeg,
            Field::Cache => &self.library.app_settings.cache,
            Field::ExportDir => &self.library.app_settings.export_dir,
            Field::Loudness => &self.library.draft.loudness,
            Field::Intensity => &self.library.draft.intensity,
            Field::Ceiling => &self.library.draft.ceiling,
            Field::SampleRate => &self.library.draft.sample_rate,
            Field::BitDepth => &self.library.draft.bit_depth,
            Field::Workers => &self.library.draft.workers,
            Field::Evaluations => &self.library.draft.evaluations,
            Field::Seed => &self.library.draft.seed,
            Field::Reference => &self.library.draft.reference,
        }
    }
    fn field_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::LowCut => &mut self.library.draft.low_cut,
            Field::HighCut => &mut self.library.draft.high_cut,
            Field::LowpassCut => &mut self.library.draft.lowpass_cut,
            Field::Oversample => &mut self.library.draft.oversample,
            Field::MaxIter1 => &mut self.library.draft.max_iter1,
            Field::MaxIter2 => &mut self.library.draft.max_iter2,
            Field::Backend => &mut self.library.app_settings.backend,
            Field::Ffmpeg => &mut self.library.app_settings.ffmpeg,
            Field::Cache => &mut self.library.app_settings.cache,
            Field::ExportDir => &mut self.library.app_settings.export_dir,
            Field::Loudness => &mut self.library.draft.loudness,
            Field::Intensity => &mut self.library.draft.intensity,
            Field::Ceiling => &mut self.library.draft.ceiling,
            Field::SampleRate => &mut self.library.draft.sample_rate,
            Field::BitDepth => &mut self.library.draft.bit_depth,
            Field::Workers => &mut self.library.draft.workers,
            Field::Evaluations => &mut self.library.draft.evaluations,
            Field::Seed => &mut self.library.draft.seed,
            Field::Reference => &mut self.library.draft.reference,
        }
    }
}

fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn settings_from_config(c: &model::Config) -> Settings {
    Settings {
        noise_mode: c.noise_mode,
        max_iter2: c.max_iter2.to_string(),
        max_iter1: c.max_iter1.to_string(),
        oversample: c.oversample.to_string(),
        lowpass_cut: c.lowpass_cut.to_string(),
        ceiling_mode: c.ceiling_mode,
        reference_mode: c.reference_mode,
        high_cut: c.high_cut.to_string(),
        low_cut: c.low_cut.to_string(),
        mastering: c.mastering,
        loudness: c.loudness.to_string(),
        intensity: c.intensity.to_string(),
        ceiling: c.ceiling.to_string(),
        sample_rate: c.sample_rate.to_string(),
        bit_depth: c.bit_depth.to_string(),
        workers: c.workers.to_string(),
        evaluations: c.evaluations.to_string(),
        seed: c.seed.to_string(),
        preserve_bass: c.preserve_bass,
        pre_compression: c.pre_compression,
        format: c.format,
        solver: c.solver,
        use_reference: c.reference_audio.is_some(),
        reference_audio: c
            .reference_audio
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        reference: c
            .reference
            .as_ref()
            .filter(|_| c.reference_audio.is_none())
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_folder_remains_available_without_enabling_writes_after_load_failure() {
        let mut app = App {
            error: Some("Cannot read saved library".into()),
            ..App::default()
        };
        assert_eq!(
            app.library_folder().unwrap(),
            library::default_root().unwrap()
        );
        let _ = app.update(Message::OpenLibrary);
        assert!(app.root.is_none());
        assert!(app.save_worker.is_none());
        assert!(app._library_lock.is_none());
        assert_eq!(app.error.as_deref(), Some("Cannot read saved library"));
    }

    #[test]
    fn library_folder_uses_the_loaded_library_location() {
        let dir = tempfile::tempdir().unwrap();
        let app = App {
            root: Some(dir.path().to_path_buf()),
            ..App::default()
        };
        assert_eq!(app.library_folder().unwrap(), dir.path());
    }

    #[test]
    fn audio_picker_cancel_preserves_state_and_multiple_files_start_imports() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App {
            root: Some(dir.path().to_path_buf()),
            input: "pending path.wav".into(),
            ..App::default()
        };
        let _ = app.update(Message::AudioPicked(None));
        assert_eq!(app.importing, 0);
        assert_eq!(app.input, "pending path.wav");
        let _ = app.update(Message::AudioPicked(Some(vec![
            dir.path().join("first.wav"),
            dir.path().join("second.wav"),
        ])));
        assert_eq!(app.importing, 2);
        assert_eq!(app.input, "pending path.wav");
    }

    #[test]
    fn audio_path_input_is_optional() {
        let mut app = App::default();
        assert!(!app.show_audio_path);
        let _ = app.update(Message::ToggleAudioPath);
        assert!(app.show_audio_path);
        app.input = "pending.wav".into();
        let _ = app.update(Message::ToggleAudioPath);
        assert!(!app.show_audio_path);
        assert_eq!(app.input, "pending.wav");
    }

    fn app_with_track(dir: &std::path::Path) -> App {
        let source = dir.join("日本語 mix.wav");
        std::fs::write(&source, b"original audio").unwrap();
        let track = library::import(dir, source, Uuid::from_u128(0)).unwrap();
        let mut app = App {
            root: Some(dir.to_path_buf()),
            ..App::default()
        };
        app.library.app_settings.export_dir = dir.display().to_string();
        app.library.tracks.push(track);
        app.selected = Some((Uuid::from_u128(0), None));
        app
    }
    #[test]
    fn disabled_mastering_ignores_pending_external_reference_and_preserves_uuid_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.library.draft.use_reference = true;
        app.library.draft.reference_audio = "missing reference.wav".into();
        app.library.draft.reference = "missing report.json".into();
        app.invalidate_draft_reference(true);
        assert!(!app.reference_ready());
        let _ = app.update(Message::Mastering(false));
        assert!(app.reference_ready());
        assert!(app.draft_reference.due.is_none());
        assert!(!app.draft_reference.active);
        app.queue_master().unwrap();
        let job = &app.library.jobs[0];
        assert_eq!(job.id.get_version(), Some(uuid::Version::Random));
        assert!(job.config.reference.is_none());
        assert!(job.config.reference_audio.is_none());
        let before = app.library.draft.reference_audio.clone();
        let _ = app.update(Message::ReferencePath("ignored while disabled.wav".into()));
        assert_eq!(app.library.draft.reference_audio, before);
    }
    #[test]
    fn validation_opens_the_collapsed_group_containing_the_invalid_setting() {
        let mut app = App::default();
        app.library.draft.max_iter1 = "invalid".into();
        let _ = app.update(Message::Master);
        assert!(app.show_limiter_details);
        assert!(!app.show_optimization);
        assert!(app.show_settings);
        assert!(app.library.jobs.is_empty());
        let _ = app.update(Message::Edit(Field::MaxIter1, "100".into()));
        let _ = app.update(Message::ToggleLimiterDetails);
        app.library.draft.seed = "invalid".into();
        let _ = app.update(Message::Master);
        assert!(!app.show_limiter_details);
        assert!(app.show_optimization);
        assert!(app.library.jobs.is_empty());
    }
    #[test]
    fn invalid_and_unfinished_settings_disable_run_and_explain_how_to_recover() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.show_settings = true;
        for (field, value) in [(Field::MaxIter1, "bad"), (Field::SampleRate, "123")] {
            *app.field_mut(field) = value.into();
            let (_, reason) = app.settings_issue().unwrap();
            let mut ui = iced_test::Simulator::with_size(
                Default::default(),
                iced::Size::new(850., 650.),
                app.view(),
            );
            assert!(
                ui.find(format!("Cannot run mastering: {reason}"))
                    .unwrap()
                    .visible_bounds()
                    .is_some()
            );
            assert!(ui.find("Run mastering").unwrap().visible_bounds().is_some());
            ui.click("Run mastering").unwrap();
            assert!(!ui.into_messages().any(|m| matches!(m, Message::Master)));
            assert!(app.queue_master().is_err());
            let _ = app.update(Message::Edit(
                field,
                if field == Field::MaxIter1 {
                    "100"
                } else {
                    "44100"
                }
                .into(),
            ));
            assert!(app.settings_issue().is_none());
        }
        let _ = app.update(Message::Edit(Field::Loudness, "-".into()));
        assert!(!app.numeric_errors.contains_key(&Field::Loudness));
        assert!(app.settings_issue().unwrap().1.contains("finish entering"));
        assert!(app.queue_master().is_err());
        let _ = app.update(Message::NumberFocused(Field::Loudness, "-".into(), false));
        assert!(app.numeric_errors.contains_key(&Field::Loudness));
        let _ = app.update(Message::Master);
        assert_eq!(app.numeric_edits[&Field::Loudness], "-");
        assert!(app.library.jobs.is_empty());
        let _ = app.update(Message::CommitNumber(Field::Loudness));
        assert!(app.settings_issue().is_none());
        let _ = app.update(Message::Edit(Field::Loudness, "NaN".into()));
        assert!(app.settings_issue().is_none()); // Rejected input did not replace the valid value.
        let _ = app.update(Message::Edit(Field::Loudness, "-12".into()));
        let _ = app.update(Message::NumberFocused(Field::Loudness, "-".into(), false));
        assert!(!app.numeric_errors.contains_key(&Field::Loudness)); // Ignore stale focus results.
        app.queue_master().unwrap();
        assert_eq!(app.library.jobs.len(), 1);
        assert_eq!(app.library.jobs[0].config.loudness, -12.);
    }

    #[test]
    fn reference_blockers_are_visible_by_run_without_an_extra_navigation_button() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.show_settings = true;
        app.library.draft.use_reference = true;
        for error in [None, Some("The reference file could not be read.")] {
            app.draft_reference.report = error.map(|e| Err(e.into()));
            app.library.draft.reference_audio =
                if error.is_some() { "missing.wav" } else { "" }.into();
            let (field, reason) = app.settings_issue().unwrap();
            assert_eq!(field, Some(Field::Reference));
            let mut ui = iced_test::Simulator::with_size(
                Default::default(),
                iced::Size::new(850., 650.),
                app.view(),
            );
            assert!(
                ui.find(format!("Cannot run mastering: {reason}"))
                    .unwrap()
                    .visible_bounds()
                    .is_some()
            );
            ui.click("Run mastering").unwrap();
            assert!(ui.find("Show setting").is_err());
            let messages: Vec<_> = ui.into_messages().collect();
            assert!(!messages.iter().any(|m| matches!(m, Message::Master)));
        }
        let _ = app.update(Message::Mastering(false));
        app.library.draft.reference_audio.clear();
        assert!(app.settings_issue().is_none());
        app.queue_master().unwrap();
    }

    #[test]
    fn every_required_numeric_control_rejects_empty_input() {
        let mut app = App::default();
        app.library.draft.ceiling_mode = CeilingMode::LowpassTruePeak;
        for field in [
            Field::Loudness,
            Field::Ceiling,
            Field::LowpassCut,
            Field::LowCut,
            Field::HighCut,
            Field::MaxIter1,
            Field::MaxIter2,
            Field::Workers,
            Field::Evaluations,
            Field::Seed,
        ] {
            let before = app.field_value(field).to_owned();
            let _ = app.update(Message::Edit(field, String::new()));
            assert_eq!(app.field_value(field), before, "{field:?}");
            assert!(!app.numeric_edits.contains_key(&field), "{field:?}");
            assert!(app.settings_issue().is_none(), "{field:?}");
        }
        // Deleting the sign of an unfinished decimal must not leave an empty buffer either.
        let _ = app.update(Message::Edit(Field::Loudness, "-".into()));
        let _ = app.update(Message::Edit(Field::Loudness, String::new()));
        assert_eq!(app.numeric_edits[&Field::Loudness], "-");
        let _ = app.update(Message::CommitNumber(Field::Loudness));
        let _ = app.update(Message::Edit(Field::Workers, "32".into()));
        assert_eq!(app.library.draft.workers, "32");
        let _ = app.update(Message::Adjust(Field::Workers, true));
        assert_eq!(app.library.draft.workers, "32");
        let _ = app.update(Message::Edit(Field::Workers, "33".into()));
        assert_eq!(app.library.draft.workers, "32");
        app.library.draft.workers = "33".into(); // Invalid saved values are also blocked.
        assert_eq!(app.settings_issue().unwrap().0, Some(Field::Workers));
        assert!(app.queue_master().is_err());
        let _ = app.update(Message::Edit(Field::Workers, "0".into()));
        assert!(app.settings_issue().is_none());
    }

    #[test]
    fn numeric_edits_keep_valid_committed_settings_and_ignore_disabled_fields() {
        let mut app = App::default();
        for bad in ["banana", "-61", "NaN"] {
            let _ = app.update(Message::Edit(Field::Loudness, bad.into()));
            assert_eq!(app.library.draft.loudness, "-9");
        }
        let _ = app.update(Message::Edit(Field::Loudness, "-".into()));
        assert_eq!(app.library.draft.loudness, "-9");
        assert_eq!(app.numeric_edits[&Field::Loudness], "-");
        let _ = app.update(Message::Edit(Field::Loudness, "-12.5".into()));
        assert_eq!(app.library.draft.loudness, "-12.5");
        assert!(app.numeric_edits.is_empty());
        assert!(app.numeric_errors.is_empty());
        let _ = app.update(Message::Adjust(Field::Loudness, true));
        assert_eq!(app.library.draft.loudness, "-12.40");
        let _ = app.update(Message::Mastering(false));
        let before = app.library.draft.intensity.clone();
        let _ = app.update(Message::Edit(Field::Intensity, "0.25".into()));
        assert_eq!(app.library.draft.intensity, before);
        let _ = app.update(Message::Edit(Field::Workers, "-1".into()));
        assert_eq!(app.library.draft.workers, "0");
    }
    #[test]
    fn format_switch_chooses_a_supported_rate_and_preserves_inactive_values() {
        let mut app = App::default();
        app.library.draft.sample_rate = "192000".into();
        app.library.draft.bit_depth = "24".into();
        let _ = app.update(Message::Format(Format::Mp3));
        assert_eq!(app.library.draft.sample_rate, "48000");
        let _ = app.update(Message::Edit(Field::BitDepth, "32".into()));
        assert_eq!(app.library.draft.bit_depth, "24");
        let _ = app.update(Message::Format(Format::Wav));
        assert_eq!(app.library.draft.bit_depth, "24");
    }
    #[test]
    fn new_processing_settings_survive_history_recall() {
        let settings = Settings {
            mastering: false,
            noise_mode: NoiseMode::Adaptive,
            reference_mode: ReferenceMode::Rms,
            ceiling_mode: CeilingMode::LowpassTruePeak,
            oversample: "4".into(),
            max_iter1: "60".into(),
            max_iter2: "240".into(),
            ..Settings::default()
        };
        let config = settings.parse().unwrap();
        let restored = settings_from_config(&config).parse().unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap(),
            serde_json::to_value(restored).unwrap()
        );
    }

    #[test]
    #[ignore = "Local profiling with an existing analyzed library"]
    fn profile_selection_save_cost() {
        let source = std::env::var("PHASELIMITER_PROFILE_LIBRARY")
            .expect("set PHASELIMITER_PROFILE_LIBRARY");
        let original = Library::load(std::path::Path::new(&source)).unwrap();
        let mut state = original.clone();
        for _ in 1..20 {
            let ids: std::collections::HashMap<_, _> = original
                .tracks
                .iter()
                .map(|t| (t.id, Uuid::new_v4()))
                .collect();
            state
                .tracks
                .extend(original.tracks.iter().cloned().map(|mut t| {
                    t.id = ids[&t.id];
                    t
                }));
            state
                .jobs
                .extend(original.jobs.iter().cloned().map(|mut j| {
                    j.id = Uuid::new_v4();
                    j.track_id = ids[&j.track_id];
                    j
                }));
        }
        let dir = tempfile::tempdir().unwrap();
        let start = std::time::Instant::now();
        state.save(dir.path()).unwrap();
        let synchronous = start.elapsed();
        let worker = library::SaveWorker::new(dir.path().to_path_buf()).unwrap();
        let start = std::time::Instant::now();
        worker.save(state.clone()).unwrap();
        let dispatch = start.elapsed();
        worker.flush().unwrap();
        eprintln!(
            "PROFILE: {} inputs, {} results; synchronous save {:.2} ms; snapshot + background dispatch {:.3} ms",
            state.tracks.len(),
            state.jobs.len(),
            synchronous.as_secs_f64() * 1000.,
            dispatch.as_secs_f64() * 1000.
        );
    }
    #[test]
    fn history_selection_defers_storage_errors_until_the_background_result() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, b"keep").unwrap();
        app.save_worker = Some(library::SaveWorker::new(blocked).unwrap());
        let path = app.library.tracks[0].input.clone();
        app.prepared.insert(
            path.clone(),
            playback::test_audio(&[[0.2; 2]; 1000], Some(&path)),
        );
        let _ = app.update(Message::Select((Uuid::from_u128(0), None)));
        assert!(app.loaded);
        assert!(!app.loading);
        assert!(app.error.is_none());
        assert!(app.save_worker.as_ref().unwrap().flush().is_err());
        let _ = app.update(Message::Tick);
        assert!(
            app.error
                .as_ref()
                .is_some_and(|e| e.contains("Could not save history"))
        );
    }
    #[test]
    fn ready_cache_selection_does_not_enter_preparing_or_discard_other_audio() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let path = app.library.tracks[0].input.clone();
        let audio = playback::test_audio(&[[0.2; 2]; 1000], Some(&path));
        app.prepared.insert(path.clone(), audio);
        let _ = app.prepare();
        assert!(app.loaded);
        assert!(!app.loading);
        assert!(app.prepared.is_idle());
        assert!(app.prepared.contains_key(&path));
    }
    #[test]
    fn background_preparation_does_not_switch_the_selected_source() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let source = dir.path().join("other.wav");
        std::fs::write(&source, b"other audio").unwrap();
        let track = library::import(dir.path(), source, Uuid::from_u128(1)).unwrap();
        let path = track.input.clone();
        app.library.tracks.push(track);
        let audio = playback::test_audio(&[[0.5; 2]; 1000], Some(&path));
        app.loading = true;
        let _ = app.update(Message::Prepared(path.clone(), Ok(audio)));
        assert_eq!(app.selected, Some((Uuid::from_u128(0), None)));
        assert!(!app.loaded);
        assert!(app.loading);
        assert!(app.prepared.contains_key(&path));
    }
    #[test]
    fn selecting_an_original_retargets_the_mastering_form() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let source = dir.path().join("another.wav");
        std::fs::write(&source, b"another input").unwrap();
        app.library
            .tracks
            .push(library::import(dir.path(), source, Uuid::from_u128(1)).unwrap());
        let _ = app.update(Message::Compose);
        let _ = app.update(Message::Select((Uuid::from_u128(1), None)));
        let _ = app.update(Message::ShowAnalysis);
        let _ = app.update(Message::ShowMastering);
        assert_eq!(app.selected.map(|key| key.0), Some(Uuid::from_u128(1)));
        app.queue_master().unwrap();
        assert_eq!(app.library.jobs[0].track_id, Uuid::from_u128(1));
    }
    #[test]
    fn setting_path_selection_is_saved_and_cancellation_preserves_value() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        for field in [
            Field::Backend,
            Field::Ffmpeg,
            Field::Cache,
            Field::ExportDir,
        ] {
            let path = dir.path().join("日本語 selected path");
            let _ = app.update(Message::SettingPathPicked(field, Some(path.clone())));
            assert_eq!(app.field_value(field), path.to_string_lossy());
            let _ = app.update(Message::SettingPathPicked(field, None));
            assert_eq!(app.field_value(field), path.to_string_lossy());
        }
        let saved = Library::load(dir.path()).unwrap();
        assert_eq!(saved.app_settings.backend, app.library.app_settings.backend);
        assert_eq!(saved.app_settings.ffmpeg, app.library.app_settings.ffmpeg);
        assert_eq!(saved.app_settings.cache, app.library.app_settings.cache);
        assert_eq!(
            saved.app_settings.export_dir,
            app.library.app_settings.export_dir
        );
    }

    #[test]
    fn history_recall_preserves_tool_overrides_and_clearing_them_does_not_change_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let recorded = app.library.jobs[0].tools.backend.clone();
        let _ = app.update(Message::Select((
            app.library.jobs[0].track_id,
            Some(app.library.jobs[0].id),
        )));
        assert!(app.library.app_settings.backend.is_empty());
        assert!(app.library.app_settings.ffmpeg.is_empty());
        assert!(app.library.app_settings.cache.is_empty());
        app.library.app_settings.backend = "explicit engine".into();
        app.library.app_settings.ffmpeg = "explicit ffmpeg".into();
        app.library.app_settings.cache = "explicit cache".into();
        let _ = app.update(Message::Select((
            app.library.jobs[0].track_id,
            Some(app.library.jobs[0].id),
        )));
        assert_eq!(app.library.app_settings.backend, "explicit engine");
        assert_eq!(app.library.app_settings.ffmpeg, "explicit ffmpeg");
        assert_eq!(app.library.app_settings.cache, "explicit cache");
        let loudness = app.library.draft.loudness.clone();
        let _ = app.update(Message::UseBundledTools);
        assert!(app.library.app_settings.backend.is_empty());
        assert!(app.library.app_settings.ffmpeg.is_empty());
        assert!(app.library.app_settings.cache.is_empty());
        assert_eq!(app.library.draft.loudness, loudness);
        assert_eq!(app.library.jobs[0].tools.backend, recorded);
        assert!(
            Library::load(dir.path())
                .unwrap()
                .app_settings
                .backend
                .is_empty()
        );
    }

    #[test]
    fn export_preferences_are_global_and_never_block_or_get_recalled_with_mastering() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let job = &app.library.jobs[0];
        let timestamp = job
            .created_at
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y%m%d_%H%M%S")
            .to_string();
        let filename = app.default_export(job);
        assert!(filename.ends_with(&format!(
            "_master_{timestamp}.{}",
            job.config.format.extension()
        )));
        let destination = dir.path().join("missing export directory");
        app.library.app_settings.export_dir = destination.display().to_string();
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[0].id),
        )));
        assert!(
            std::path::Path::new(&app.default_export(&app.library.jobs[0]))
                .starts_with(&destination)
        );
        let _ = app.update(Message::Select((
            app.library.jobs[0].track_id,
            Some(app.library.jobs[0].id),
        )));
        assert_eq!(app.library.app_settings.export_directory(), destination);
        app.queue_master().unwrap();
        app.running = false;
        assert_eq!(app.library.jobs.len(), 2);
        let new_destination = dir.path().join("another export directory");
        let _ = app.update(Message::Edit(
            Field::ExportDir,
            new_destination.display().to_string(),
        ));
        assert!(
            std::path::Path::new(&app.default_export(&app.library.jobs[0]))
                .starts_with(&new_destination)
        );
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[1].id),
        )));
        assert!(
            std::path::Path::new(&app.default_export(&app.library.jobs[0]))
                .starts_with(new_destination)
        );
        let saved = serde_json::to_value(&app.library).unwrap();
        assert!(saved["draft"].get("output_dir").is_none());
        assert!(saved["jobs"][0]["config"].get("output_dir").is_none());
    }

    #[test]
    fn changes_are_visible_immediately_above_run_on_small_screens() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[0].id),
        )));
        let _ = app.update(Message::Compose);
        app.library.draft.loudness = "-12".into();
        app.library.draft.ceiling = "-1".into();
        app.library.draft.intensity = "0.2".into();
        app.library.draft.sample_rate = "48000".into();
        app.library.draft.evaluations = "100".into();
        app.library.draft.seed = "42".into();
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(850., 650.),
            app.view(),
        );
        let changes = ui.find("Changes").unwrap();
        assert!(changes.visible_bounds().is_some());
        let run = ui.find("Run mastering").unwrap();
        assert!(run.visible_bounds().is_some());
        assert!(run.bounds().y > changes.bounds().y);
        assert!(run.bounds().y - changes.bounds().y < 220.);
        assert!(ui.find("Default export directory").is_err());
        assert!(
            ui.find(format!(
                "Load settings from {}",
                app.library.job_label(&app.library.jobs[0])
            ))
            .is_err()
        );
    }

    #[test]
    fn selecting_history_recalls_settings_and_clears_unfinished_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.library.draft.loudness = "-12".into();
        app.library.draft.intensity = "0.2".into();
        app.queue_master().unwrap();
        let job = app.library.jobs[0].clone();
        let _ = app.update(Message::Edit(Field::Loudness, "-".into()));
        let _ = app.update(Message::Select((job.track_id, Some(job.id))));
        assert!(app.numeric_edits.is_empty());
        assert!(app.numeric_errors.is_empty());
        assert_eq!(
            serde_json::to_value(app.library.draft.parse().unwrap()).unwrap(),
            serde_json::to_value(&job.config).unwrap()
        );
        app.library.draft.loudness = "-9".into();
        app.queue_master().unwrap();
        assert_eq!(app.library.jobs[0].config.loudness, -12.);
        assert_eq!(app.library.jobs[1].config.loudness, -9.);
        assert_eq!(app.library.jobs[1].track_id, job.track_id);
    }

    #[test]
    fn repeated_mastering_keeps_results_and_captures_each_setting() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.library.draft.loudness = "-9".into();
        app.queue_master().unwrap();
        app.library.draft.loudness = "-11".into();
        app.queue_master().unwrap();
        assert_eq!(app.library.jobs.len(), 2);
        assert_eq!(app.library.jobs[0].config.loudness, -9.);
        assert_eq!(app.library.jobs[1].config.loudness, -11.);
        assert_eq!(app.library.jobs[0].input, app.library.jobs[1].input);
        assert_ne!(app.library.jobs[0].output, app.library.jobs[1].output);
        assert_eq!(app.library.draft.loudness, "-11");
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[0].id),
        )));
        assert_eq!(app.library.draft.loudness, "-9");
        assert!(app.output_side);
        let _ = app.update(Message::Side(false));
        assert_eq!(
            app.selected,
            Some((Uuid::from_u128(0), Some(app.library.jobs[0].id)))
        );
        assert!(!app.output_side);
    }
    #[test]
    fn comparing_results_switches_gain_and_audio_without_changing_the_draft() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.queue_master().unwrap();
        app.running = false;
        for (i, job) in app.library.jobs.iter_mut().enumerate() {
            job.status = Status::Succeeded;
            job.analysis = Some(
                analysis::Analysis::store(
                    dir.path(),
                    &serde_json::to_vec(&serde_json::json!({
                        "loudness": if i == 0 { -12. } else { -9. }, "true_peak": -1.
                    }))
                    .unwrap(),
                )
                .unwrap(),
            );
        }
        app.library.draft.loudness = "-10".into();
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[1].id),
        )));
        app.library.draft.loudness = "-10".into();
        let _ = app.update(Message::Compare(
            app.comparison_choices()
                .into_iter()
                .find(|c| c.job == Some(app.library.jobs[0].id))
                .unwrap(),
        ));
        let _ = app.update(Message::Side(false));
        assert_eq!(
            app.listening_key(),
            Some((Uuid::from_u128(0), Some(app.library.jobs[0].id)))
        );
        assert!((app.selected_gain() - 10f32.powf(-6. / 20.)).abs() < 0.0001);
        let _ = app.update(Message::Side(true));
        assert_eq!(
            app.listening_key(),
            Some((Uuid::from_u128(0), Some(app.library.jobs[1].id)))
        );
        assert_eq!(app.library.draft.loudness, "-10");
        assert_eq!(
            Library::load(dir.path()).unwrap().comparison_job,
            Some(app.library.jobs[0].id)
        );
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[0].id),
        )));
        assert_eq!(app.comparison_job, None);
        assert_eq!(app.comparison_label(), "Original");
    }

    #[test]
    fn queueing_keeps_the_settings_workspace_open_for_the_next_iteration() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let _ = app.update(Message::Compose);
        app.queue_master().unwrap();
        assert!(app.show_settings);
        assert_eq!(app.selected.map(|key| key.0), Some(Uuid::nil()));
        app.library.draft.loudness = "-11".into();
        app.queue_master().unwrap();
        assert!(app.show_settings);
        assert_eq!(app.library.jobs[1].config.loudness, -11.);
        assert_eq!(app.selected, Some((Uuid::from_u128(0), None)));
    }

    #[test]
    fn complete_backend_log_survives_history_reload_beyond_the_preview_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        let _ = app.start_next();
        let lines: Vec<_> = (0..550)
            .map(|i| format!("解析ログ {i}: {}", "x".repeat(2500)))
            .collect();
        for line in &lines {
            let _ = app.update(Message::Backend(
                app.library.jobs[0].id,
                backend::Update::Line(line.clone()),
            ));
        }
        let _ = app.update(Message::Backend(
            app.library.jobs[0].id,
            backend::Update::Cancelled,
        ));
        assert_eq!(app.library.jobs[0].log, lines);
        let restored = Library::load(dir.path()).unwrap();
        assert_eq!(restored.jobs[0].log, lines);
    }

    #[test]
    fn narrow_settings_keep_transport_and_run_action_visible() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let _ = app.update(Message::Compose);
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(850., 650.),
            app.view(),
        );
        ui.find("Play").unwrap();
        ui.find("Mastering settings").unwrap();
        ui.click("Run mastering").unwrap();
        assert!(ui.into_messages().any(|m| matches!(m, Message::Master)));
    }

    #[test]
    fn wide_workspace_exposes_analysis_and_new_settings_together() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_track(dir.path());
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(1280., 800.),
            app.view(),
        );
        ui.find("Mastering settings").unwrap();
        ui.find("Professionality").unwrap();
        ui.find("Professionality2").unwrap();
        ui.find("Dissonance").unwrap();
        assert!(ui.find("Ear Damage").is_err());
        ui.click("Run mastering").unwrap();
        assert!(ui.into_messages().any(|m| matches!(m, Message::Master)));
    }

    #[test]
    fn external_reference_settings_round_trip_and_history_deletion_preserves_draft() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let source = dir.path().join("professional target.wav");
        std::fs::write(&source, b"reference audio").unwrap();
        app.library.draft.use_reference = true;
        app.library.draft.reference_audio = source.display().to_string();
        app.draft_reference.report = Some(Ok(reference::Prepared {
            analysis: analysis::Analysis::store(dir.path(), br#"{"bands":[],"covariance":[]}"#)
                .unwrap(),
            source_hash: library::hash(&source).unwrap(),
        }));
        app.queue_master().unwrap();
        let job = &app.library.jobs[0];
        assert_eq!(job.config.reference_audio.as_ref(), Some(&source));
        assert_ne!(job.config.reference.as_ref(), Some(&source));
        let restored = settings_from_config(&job.config);
        assert!(restored.use_reference);
        assert_eq!(restored.reference_audio, source.display().to_string());
        assert!(restored.reference.is_empty());
        app.library.draft = restored.clone();
        app.library.jobs.clear();
        assert_eq!(
            app.library.draft.reference_audio,
            source.display().to_string()
        );
        app.library.save(dir.path()).unwrap();
        let loaded = Library::load(dir.path()).unwrap();
        assert_eq!(loaded.draft.reference_audio, source.display().to_string());
        assert!(loaded.draft.use_reference);
        std::fs::write(&source, b"changed reference audio").unwrap();
        assert!(app.queue_master().is_err());
    }

    #[test]
    fn reference_checkbox_and_pending_state_disable_mastering() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.library.draft.use_reference = true;
        let mut ui = iced_test::Simulator::with_size(
            Default::default(),
            iced::Size::new(1280., 800.),
            app.view(),
        );
        ui.find("Use reference audio").unwrap();
        ui.find("Browse…").unwrap();
        ui.click("Run mastering").unwrap();
        assert!(!ui.into_messages().any(|m| matches!(m, Message::Master)));
    }

    #[test]
    fn reference_loading_ignores_previous_selection_and_verifies_saved_reports() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let report = analysis::Analysis::store(dir.path(), br#"{"loudness":-12}"#).unwrap();
        let path = report.path().unwrap().to_path_buf();
        app.library.tracks[0].analysis = Some(report);
        app.library.draft.reference = path.display().to_string();
        app.queue_master().unwrap();
        app.running = false;
        let _ = app.update(Message::Select((
            Uuid::from_u128(0),
            Some(app.library.jobs[0].id),
        )));
        assert_eq!(app.reference_pending.as_ref(), Some(&path));
        let _ = app.update(Message::ReferenceLoaded(
            path.clone(),
            analysis::Analysis::read_report(path.clone()),
        ));
        assert_eq!(
            app.reference_analysis().unwrap().number("loudness"),
            Some(-12.)
        );
        let _ = app.update(Message::Select((Uuid::from_u128(0), None)));
        let _ = app.update(Message::ReferenceLoaded(
            path.clone(),
            analysis::Analysis::read_report(path.clone()),
        ));
        assert!(app.reference_analysis().is_none());
        std::fs::write(&path, br#"{"loudness":-1}"#).unwrap();
        assert!(analysis::Analysis::read_report(path).is_err());
    }
    #[test]
    fn reference_is_shared_and_survives_source_removal() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let report = serde_json::json!({"bands":[],"covariance":[]});
        let track = &mut app.library.tracks[0];
        track.analysis = Some(
            analysis::Analysis::store(dir.path(), &serde_json::to_vec(&report).unwrap()).unwrap(),
        );
        let report_path = track
            .analysis
            .as_ref()
            .unwrap()
            .path()
            .unwrap()
            .to_path_buf();
        app.library.draft.reference = report_path.display().to_string();
        app.queue_master().unwrap();
        let job = &app.library.jobs[0];
        assert_eq!(job.config.reference.as_ref().unwrap(), &report_path);
        library::verify(
            job.config.reference.as_ref().unwrap(),
            job.reference_hash.as_ref().unwrap(),
        )
        .unwrap();
        std::fs::remove_dir_all(app.library.tracks[0].input.parent().unwrap()).unwrap();
        let _ = app.update(Message::Select((job.track_id, Some(job.id))));
        app.queue_master().unwrap();
        assert!(app.library.jobs[1].config.reference.is_some());
    }
    #[test]
    fn history_ui_selects_directly_without_a_comparison_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        app.library.draft.loudness = "-12".into();
        app.queue_master().unwrap();
        app.running = false;
        app.library.jobs[0].memo = "older".into();
        app.library.jobs[1].memo = "newer".into();
        let messages = {
            let mut ui = iced_test::simulator(app.view());
            ui.find("日本語 mix.wav").unwrap();
            let older = ui
                .find(app.library.job_label(&app.library.jobs[0]))
                .unwrap();
            let newer = ui
                .find(app.library.job_label(&app.library.jobs[1]))
                .unwrap();
            assert!(newer.bounds().y < older.bounds().y);
            ui.click(app.library.job_label(&app.library.jobs[0]))
                .unwrap();
            ui.into_messages()
        };
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(
            app.selected,
            Some((Uuid::from_u128(0), Some(app.library.jobs[0].id)))
        );
        assert!(!app.show_settings);
    }
    #[test]
    fn finished_background_work_does_not_change_selection() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let _ = app.update(Message::Completed(
            app.library.jobs[0].id,
            Ok("checksum".into()),
        ));
        assert_eq!(app.selected, Some((Uuid::from_u128(0), None)));
        assert_eq!(app.library.jobs[0].status, Status::Succeeded);
        app.save();
        let loaded = Library::load(dir.path()).unwrap();
        assert_eq!(loaded.selected, Some((Uuid::from_u128(0), None)));
        assert_eq!(loaded.jobs[0].output_hash.as_deref(), Some("checksum"));
    }
    #[test]
    fn memo_edits_autosave_latest_text_without_affecting_audio_or_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let id = app.library.jobs[0].id;
        let created = app.library.jobs[0].created_at;
        let output = app.library.jobs[0].output.clone();
        let config = serde_json::to_value(&app.library.jobs[0].config).unwrap();
        let _ = app.update(Message::Select((Uuid::nil(), Some(id))));
        let export = app.default_export(&app.library.jobs[0]);
        for memo in ["低域", "低域控えめ", "今のところこれが良い"] {
            let _ = app.update(Message::EditMemo(id, memo.into()));
        }
        assert_eq!(Library::load(dir.path()).unwrap().jobs[0].memo, "");
        app.memo_save_at = Some(std::time::Instant::now());
        let _ = app.update(Message::Tick);
        let saved = Library::load(dir.path()).unwrap();
        assert_eq!(saved.jobs[0].memo, "今のところこれが良い");
        assert_eq!(saved.jobs[0].id, id);
        assert_eq!(saved.jobs[0].created_at, created);
        assert_eq!(saved.jobs[0].output, output);
        assert_eq!(serde_json::to_value(&saved.jobs[0].config).unwrap(), config);
        assert_eq!(saved.jobs[0].status, Status::Waiting);
        assert_eq!(app.default_export(&app.library.jobs[0]), export);
        assert_eq!(app.selected, Some((Uuid::nil(), Some(id))));
        assert_eq!(saved.jobs.len(), 1);
        assert_eq!(id.get_version(), Some(uuid::Version::Random));
        let _ = app.update(Message::EditMemo(id, String::new()));
        let _ = app.update(Message::Select((Uuid::nil(), None)));
        assert_eq!(Library::load(dir.path()).unwrap().jobs[0].memo, "");
    }

    #[test]
    fn normal_close_flushes_a_memo_before_the_debounce_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        app.save_worker = Some(library::SaveWorker::new(dir.path().to_path_buf()).unwrap());
        let id = app.library.jobs[0].id;
        let _ = app.update(Message::EditMemo(id, "残しておく".into()));
        let _ = app.update(Message::Close);
        assert_eq!(
            Library::load(dir.path()).unwrap().jobs[0].memo,
            "残しておく"
        );
    }
}
