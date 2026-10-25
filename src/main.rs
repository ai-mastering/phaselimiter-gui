#![deny(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod analysis;
mod backend;
mod library;
mod model;
mod playback;
mod plots;
mod ui;

use iced::widget::{
    Column, button, canvas, checkbox, column, container, pick_list, progress_bar, row, rule,
    scrollable, slider, text, text_input,
};
use iced::{Element, Fill, Subscription, Task, Theme};
use library::{Library, Track};
use model::{Format, Job, Settings, Solver, Status};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

type AudioKey = (usize, Option<usize>);
fn main() -> iced::Result {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        playback::log(&format!("Rust panic: {panic}"));
        previous_hook(panic);
    }));
    iced::application(App::load, App::update, App::view)
        .title("Phaselimiter")
        .subscription(App::subscription)
        .theme(Theme::Light)
        .window_size((1240.0, 860.0))
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
    Imported(Result<Track, String>),
    Format(Format),
    Solver(Solver),
    PreserveBass(bool),
    PreCompression(bool),
    ChooseReference(ReferenceChoice),
    Compose,
    ToggleSettings,
    UseSettings(usize),
    Master,
    Start,
    Cancel,
    Remove(AudioKey),
    Select(AudioKey),
    Backend(usize, backend::Update),
    Completed(usize, Result<String, String>),
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
    StorageSize(u64),
    OpenLibrary,
    FolderOpened(Result<(), String>),
    Export,
    ExportPath(String),
    Exported(Result<(), String>),
    Close,
}
struct Active {
    id: usize,
    cancel: Arc<AtomicBool>,
    _handle: iced::task::Handle,
}
struct App {
    library: Library,
    root: Option<PathBuf>,
    // Drop the save worker before releasing the library lock.
    save_worker: Option<library::SaveWorker>,
    _library_lock: Option<std::fs::File>,
    draft: Settings,
    input: String,
    selected: Option<AudioKey>,
    active: Option<Active>,
    running: bool,
    closing: bool,
    hovering: bool,
    error: Option<String>,
    notice: Option<String>,
    show_settings: bool,
    composer_track: Option<usize>,
    show_details: bool,
    output_side: bool,
    graph: plots::Kind,
    analysis_queue: VecDeque<AudioKey>,
    analyzing: Option<AudioKey>,
    importing: usize,
    finalizing: HashSet<usize>,
    player: playback::Player,
    prepared: HashMap<PathBuf, Arc<playback::Decoded>>,
    warming: HashSet<PathBuf>,
    prepared_order: VecDeque<PathBuf>,
    loading: bool,
    loaded: bool,
    active_key: Option<AudioKey>,
    equalize: bool,
    region: Option<(f64, f64)>,
    position: f64,
    duration: f64,
    export_path: String,
    storage_bytes: u64,
}
impl Default for App {
    fn default() -> Self {
        let library = Library::default();
        let draft = library.settings.clone();
        Self {
            library,
            root: None,
            save_worker: None,
            _library_lock: None,
            draft,
            input: String::new(),
            selected: None,
            active: None,
            running: false,
            closing: false,
            hovering: false,
            error: None,
            notice: None,
            show_settings: false,
            composer_track: None,
            show_details: false,
            output_side: false,
            graph: plots::Kind::Spectrum,
            analysis_queue: VecDeque::new(),
            analyzing: None,
            importing: 0,
            player: playback::Player::default(),
            prepared: HashMap::new(),
            warming: HashSet::new(),
            prepared_order: VecDeque::new(),
            finalizing: HashSet::new(),
            loading: false,
            loaded: false,
            active_key: None,
            equalize: true,
            region: None,
            position: 0.,
            duration: 0.,
            export_path: String::new(),
            storage_bytes: 0,
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
                app.draft = library
                    .draft
                    .clone()
                    .unwrap_or_else(|| library.settings.clone());
                app.selected = library.selected;
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
        let task = app.prepare();
        let analysis = app.start_analysis();
        let storage = app.refresh_storage();
        (app, Task::batch([task, analysis, storage]))
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
        self.library.selected = self.selected;
        self.library.output_side = Some(self.output_side);
        self.library.equalize = Some(self.equalize);
        self.library.draft = Some(self.draft.clone());
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
    fn listening_key(&self) -> Option<AudioKey> {
        let (track, job) = self.selected?;
        Some((track, if self.output_side { job } else { None }))
    }
    fn listening_analysis(&self) -> Option<&analysis::Analysis> {
        if self.output_side && self.job().is_some() {
            self.job()?.analysis.as_ref()
        } else {
            self.track()?.analysis.as_ref()
        }
    }
    fn selected_gain(&self) -> f32 {
        // One fixed target for every item; never normalize against the last selected item.
        if self.equalize {
            self.listening_analysis()
                .and_then(|a| Some((a.number("loudness")?, a.number("true_peak")?)))
                .map_or(1., |(loudness, peak)| {
                    10f64.powf((-18. - loudness).min(-1. - peak).clamp(-60., 24.) / 20.) as f32
                })
        } else {
            1.
        }
    }
    fn update_gain(&self) {
        if !self.loading && self.listening_key() == self.active_key {
            self.player.gain(self.selected_gain());
        }
    }
    fn activate(&mut self, audio: Arc<playback::Decoded>) {
        match self.player.load(audio, self.selected_gain()) {
            Ok(()) => {
                self.loaded = true;
                self.active_key = self.listening_key();
                self.loading = false;
                self.player.region(self.region);
            }
            Err(error) => {
                self.loading = false;
                self.error = Some(error);
            }
        }
    }
    fn request_decode(&mut self, path: PathBuf, hash: String) -> Task<Message> {
        if !self.warming.insert(path.clone()) {
            return Task::none();
        }
        let cached = self.prepared.get(&path).cloned();
        let ffmpeg = self.draft.ffmpeg.clone();
        let message_path = path.clone();
        Task::perform(
            async move {
                if let Some(audio) = cached {
                    tokio::task::spawn_blocking(move || library::verify(&path, &hash))
                        .await
                        .map_err(|e| e.to_string())??;
                    Ok(audio)
                } else {
                    playback::decode(path, hash, ffmpeg).await
                }
            },
            move |result| Message::Prepared(message_path.clone(), result),
        )
    }
    fn warm_selection(&mut self) -> Task<Message> {
        let Some((track, _)) = self.selected else {
            return Task::none();
        };
        let mut keys = vec![(track, None)];
        keys.extend(
            self.library
                .jobs
                .iter()
                .rev()
                .filter(|j| j.track_id == track && j.status == Status::Succeeded)
                .take(7)
                .map(|j| (track, Some(j.id))),
        );
        let mut tasks = Vec::new();
        for key in keys {
            if let Some((path, hash)) = self.audio(key)
                && !self.prepared.contains_key(&path)
            {
                tasks.push(self.request_decode(path, hash));
            }
        }
        Task::batch(tasks)
    }
    fn prepare(&mut self) -> Task<Message> {
        self.loading = false;
        let Some(key) = self.listening_key() else {
            return Task::none();
        };
        let Some((path, hash)) = self.audio(key) else {
            self.notice =
                Some("Selected result is not ready. Current playback is unchanged.".into());
            return Task::none();
        };
        self.loading = true;
        if let Some(audio) = self.prepared.get(&path).cloned()
            && audio.unchanged(&path)
        {
            self.prepared_order.retain(|p| p != &path);
            self.prepared_order.push_back(path);
            self.activate(audio);
            return self.warm_selection();
        }
        let task = self.request_decode(path, hash);
        let warm = self.warm_selection();
        Task::batch([task, warm])
    }

    fn import(&mut self, path: PathBuf) -> Task<Message> {
        self.hovering = false;
        let Some(root) = self.root.clone() else {
            self.error = Some("The data directory is unavailable.".into());
            return Task::none();
        };
        let id = self.library.next_id;
        self.library.next_id += 1;
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
        if self.analyzing.is_some() || self.closing {
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
            let settings = self.draft.clone();
            return Task::perform(
                analysis::run(
                    root,
                    input,
                    hash,
                    PathBuf::from(settings.backend),
                    settings.ffmpeg,
                    PathBuf::from(settings.cache),
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
        let id = self
            .composer_track
            .or(self.selected.map(|key| key.0))
            .ok_or("Select an input first.")?;
        let track = self
            .library
            .tracks
            .iter()
            .find(|t| t.id == id)
            .ok_or("Select an input first.")?
            .clone();
        let root = self.root.as_ref().ok_or("Data directory unavailable.")?;
        let id = self.library.next_id;
        self.library.next_id += 1;
        let dir = root.join("jobs").join(id.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let config = self.draft.parse()?;
        if let Some(reference) = &config.reference {
            let expected = self
                .library
                .tracks
                .iter()
                .filter_map(|t| t.analysis.as_ref())
                .find(|a| a.path() == Some(reference.as_path()))
                .and_then(|a| a.path())
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .map(str::to_owned)
                .or_else(|| {
                    self.library
                        .jobs
                        .iter()
                        .find(|j| j.config.reference.as_ref() == Some(reference))
                        .and_then(|j| j.reference_hash.clone())
                })
                .ok_or("Choose an analyzed library audio as reference.")?;
            library::verify(reference, &expected)?;
        }
        let output = dir.join("output").with_extension(config.format.extension());
        let reference_hash = config
            .reference
            .as_ref()
            .map(|p| library::hash(p))
            .transpose()?;
        self.library.jobs.push(Job {
            id,
            track_id: track.id,
            input: track.input,
            output,
            config,
            status: Status::Waiting,
            progress: 0.,
            log: Vec::new(),
            output_hash: None,
            input_hash: Some(track.hash),
            reference_hash,
            analysis: None,
            analysis_error: None,
            limiting_error: None,
        });
        self.library.settings = self.draft.clone();
        self.running = true;
        self.show_settings = false;
        self.notice = Some("Mastering queued. Playback selection is unchanged.".into());
        Ok(())
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut save = false;
        match message {
            Message::Edit(field, value) => {
                *self.field_mut(field) = value;
                save = true;
            }
            Message::Input(value) => self.input = value,
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
            Message::Format(v) => {
                self.draft.format = v;
                save = true;
            }
            Message::Solver(v) => {
                self.draft.solver = v;
                save = true;
            }
            Message::PreserveBass(v) => {
                self.draft.preserve_bass = v;
                save = true;
            }
            Message::PreCompression(v) => {
                self.draft.pre_compression = v;
                save = true;
            }
            Message::ChooseReference(choice) => {
                self.draft.reference = choice.path;
                save = true;
            }
            Message::Compose => {
                self.composer_track = self.selected.map(|key| key.0);
                self.show_settings = true;
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
                if self.show_settings {
                    self.composer_track = self.selected.map(|key| key.0);
                }
                tasks.push(self.refresh_storage());
            }
            Message::UseSettings(id) => {
                if let Some(job) = self.library.jobs.iter().find(|j| j.id == id) {
                    self.composer_track = Some(job.track_id);
                    self.draft = settings_from_config(&job.config, &self.library.settings);
                    self.show_settings = true;
                }
            }
            Message::Master => match self.queue_master() {
                Ok(()) => save = true,
                Err(error) => self.error = Some(error),
            },
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
                self.selected = Some(key);
                self.output_side = key.1.is_some();
                self.error = None;
                self.notice = None;
                self.export_path = self
                    .job()
                    .map(|j| self.default_export(j))
                    .unwrap_or_default();
                tasks.push(self.prepare());
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
                    if key.1.is_none() {
                        if let Some(track) = self.library.tracks.iter().find(|t| t.id == key.0) {
                            let reference = track
                                .analysis
                                .as_ref()
                                .and_then(|a| a.path())
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            if self.draft.reference == reference {
                                self.draft.reference.clear();
                            }
                            if self.library.settings.reference == reference {
                                self.library.settings.reference.clear();
                            }
                        }
                        if self.composer_track == Some(key.0) {
                            self.composer_track = None;
                            self.show_settings = false;
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
            Message::Backend(id, update) => {
                if !self.active.as_ref().is_some_and(|a| a.id == id) {
                    return Task::none();
                }
                if let Some(job) = self.library.jobs.iter_mut().find(|j| j.id == id) {
                    match update {
                        backend::Update::Line(line) => {
                            if job.log.len() >= 400 {
                                job.log.remove(0);
                            }
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
                if let Some((id, job)) = self.selected {
                    for key in [(id, None), (id, job)] {
                        if !self.analysis_queue.contains(&key) && self.analyzing != Some(key) {
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
            Message::Tick => {
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
                self.warming.remove(&path);
                let wanted = self
                    .listening_key()
                    .and_then(|key| self.audio(key))
                    .is_some_and(|(p, _)| p == path);
                match result {
                    Ok(audio) => {
                        let registered = self.library.tracks.iter().any(|t| t.input == path)
                            || self.library.jobs.iter().any(|j| j.output == path);
                        if registered {
                            self.prepared_order.retain(|p| p != &path);
                            self.prepared_order.push_back(path.clone());
                            self.prepared.insert(path, audio.clone());
                            while self.prepared_order.len() > 8 {
                                if let Some(old) = self.prepared_order.pop_front() {
                                    self.prepared.remove(&old);
                                }
                            }
                            if wanted {
                                self.activate(audio);
                            } else {
                                self.player.warm(audio);
                            }
                        }
                    }
                    Err(error) => {
                        if wanted {
                            self.loading = false;
                            self.error = Some(error);
                        }
                    }
                }
            }
            Message::Graph(kind) => self.graph = kind,
            Message::Details => self.show_details = !self.show_details,
            Message::StorageSize(bytes) => self.storage_bytes = bytes,
            Message::OpenLibrary => {
                if let Some(root) = &self.root {
                    tasks.push(Task::perform(
                        library::open_folder(root.clone()),
                        Message::FolderOpened,
                    ));
                }
            }
            Message::FolderOpened(result) => {
                if let Err(e) = result {
                    self.error = Some(e);
                }
            }
            Message::ExportPath(path) => self.export_path = path,
            Message::Export => {
                if let Some(job) = self.job()
                    && let Some(hash) = job.output_hash.clone()
                {
                    let source = job.output.clone();
                    let dest = PathBuf::from(&self.export_path);
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
                Ok(()) => self.notice = Some("Exported. The stored result is unchanged.".into()),
                Err(e) => self.error = Some(e),
            },
            Message::Close => {
                self.closing = true;
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
        {
            if self.save_sync() {
                return iced::exit();
            }
            self.closing = false;
        }
        tasks.push(self.start_next());
        tasks.push(self.start_analysis());
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
        job.config
            .output_dir
            .join(format!(
                "{name}_master_{}.{}",
                job.id,
                job.config.format.extension()
            ))
            .display()
            .to_string()
    }
    fn field_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Backend => &mut self.draft.backend,
            Field::Ffmpeg => &mut self.draft.ffmpeg,
            Field::Cache => &mut self.draft.cache,
            Field::OutputDir => &mut self.draft.output_dir,
            Field::Loudness => &mut self.draft.loudness,
            Field::Intensity => &mut self.draft.intensity,
            Field::Stereo => &mut self.draft.stereo,
            Field::Ceiling => &mut self.draft.ceiling,
            Field::SampleRate => &mut self.draft.sample_rate,
            Field::BitDepth => &mut self.draft.bit_depth,
            Field::Workers => &mut self.draft.workers,
            Field::Evaluations => &mut self.draft.evaluations,
            Field::Seed => &mut self.draft.seed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceChoice {
    path: String,
    label: String,
}
impl std::fmt::Display for ReferenceChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
impl App {
    fn reference_choices(&self) -> Vec<ReferenceChoice> {
        let mut choices = vec![ReferenceChoice {
            path: String::new(),
            label: "Reference: default".into(),
        }];
        choices.extend(
            self.library
                .tracks
                .iter()
                .filter(|t| t.analysis_error.is_none())
                .filter_map(|t| {
                    let analysis = t.analysis.as_ref().filter(|a| a.available())?;
                    Some(ReferenceChoice {
                        path: analysis.path()?.display().to_string(),
                        label: format!("Reference: {} · #{}", t.name, t.id),
                    })
                }),
        );
        if !self.draft.reference.is_empty()
            && !choices.iter().any(|c| c.path == self.draft.reference)
        {
            choices.push(ReferenceChoice {
                path: self.draft.reference.clone(),
                label: "Reference: saved with this result".into(),
            });
        }
        choices
    }
    fn selected_reference(&self) -> Option<ReferenceChoice> {
        self.reference_choices()
            .into_iter()
            .find(|r| r.path == self.draft.reference)
    }
}
fn field<'a>(label: &'static str, value: &'a str, field: Field) -> Element<'a, Message> {
    column![
        text(label).size(12),
        text_input(label, value)
            .on_input(move |v| Message::Edit(field, v))
            .padding(8)
    ]
    .spacing(5)
    .width(Fill)
    .into()
}
fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn measure(a: Option<&analysis::Analysis>, key: &str, unit: &str) -> String {
    a.and_then(|a| a.number(key))
        .map_or_else(|| "—".into(), |v| format!("{v:.1}{unit}"))
}
fn settings_from_config(c: &model::Config, _base: &Settings) -> Settings {
    Settings {
        backend: c.backend.display().to_string(),
        ffmpeg: c.ffmpeg.clone(),
        cache: c.cache.display().to_string(),
        output_dir: c.output_dir.display().to_string(),
        loudness: c.loudness.to_string(),
        intensity: c.intensity.to_string(),
        stereo: c.stereo.to_string(),
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
        reference: c
            .reference
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app_with_track(dir: &std::path::Path) -> App {
        let source = dir.join("日本語 mix.wav");
        std::fs::write(&source, b"original audio").unwrap();
        let track = library::import(dir, source, 0).unwrap();
        let mut app = App {
            root: Some(dir.to_path_buf()),
            ..App::default()
        };
        app.draft.output_dir = dir.display().to_string();
        app.library.tracks.push(track);
        app.library.next_id = 1;
        app.selected = Some((0, None));
        app
    }
    #[test]
    #[ignore = "Local profiling with an existing analyzed library"]
    fn profile_selection_save_cost() {
        let source = std::env::var("PHASELIMITER_PROFILE_LIBRARY")
            .expect("set PHASELIMITER_PROFILE_LIBRARY");
        let original = Library::load(std::path::Path::new(&source)).unwrap();
        let mut state = original.clone();
        for copy in 1..20 {
            state
                .tracks
                .extend(original.tracks.iter().cloned().map(|mut t| {
                    t.id += copy * 100;
                    t
                }));
            state
                .jobs
                .extend(original.jobs.iter().cloned().map(|mut j| {
                    j.id += copy * 100;
                    j.track_id += copy * 100;
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
        let _ = app.update(Message::Select((0, None)));
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
        app.prepared_order.push_back(path.clone());
        let _ = app.prepare();
        assert!(app.loaded);
        assert!(!app.loading);
        assert!(app.warming.is_empty());
        assert!(app.prepared.contains_key(&path));
    }
    #[test]
    fn background_preparation_does_not_switch_the_selected_source() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let source = dir.path().join("other.wav");
        std::fs::write(&source, b"other audio").unwrap();
        let track = library::import(dir.path(), source, 1).unwrap();
        let path = track.input.clone();
        app.library.tracks.push(track);
        let audio = playback::test_audio(&[[0.5; 2]; 1000], Some(&path));
        app.loading = true;
        let _ = app.update(Message::Prepared(path.clone(), Ok(audio)));
        assert_eq!(app.selected, Some((0, None)));
        assert!(!app.loaded);
        assert!(app.loading);
        assert!(app.prepared.contains_key(&path));
    }
    #[test]
    fn browsing_history_does_not_retarget_an_open_mastering_form() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        let source = dir.path().join("another.wav");
        std::fs::write(&source, b"another input").unwrap();
        app.library
            .tracks
            .push(library::import(dir.path(), source, 1).unwrap());
        app.library.next_id = 2;
        let _ = app.update(Message::Compose);
        let _ = app.update(Message::Select((1, None)));
        assert_eq!(app.composer_track, Some(0));
        app.queue_master().unwrap();
        assert_eq!(app.library.jobs[0].track_id, 0);
    }
    #[test]
    fn repeated_mastering_keeps_results_and_captures_each_setting() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.draft.loudness = "-9".into();
        app.queue_master().unwrap();
        app.draft.loudness = "-11".into();
        app.queue_master().unwrap();
        assert_eq!(app.library.jobs.len(), 2);
        assert_eq!(app.library.jobs[0].config.loudness, -9.);
        assert_eq!(app.library.jobs[1].config.loudness, -11.);
        assert_eq!(app.library.jobs[0].input, app.library.jobs[1].input);
        assert_ne!(app.library.jobs[0].output, app.library.jobs[1].output);
        assert_eq!(app.library.settings.loudness, "-11");
        let before = app.draft.loudness.clone();
        let _ = app.update(Message::Select((0, Some(1))));
        assert_eq!(app.draft.loudness, before);
        assert!(app.output_side);
        let _ = app.update(Message::Side(false));
        assert_eq!(app.selected, Some((0, Some(1))));
        assert!(!app.output_side);
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
        app.draft.reference = report_path.display().to_string();
        app.queue_master().unwrap();
        let job = &app.library.jobs[0];
        assert_eq!(job.config.reference.as_ref().unwrap(), &report_path);
        library::verify(
            job.config.reference.as_ref().unwrap(),
            job.reference_hash.as_ref().unwrap(),
        )
        .unwrap();
        std::fs::remove_dir_all(app.library.tracks[0].input.parent().unwrap()).unwrap();
        let _ = app.update(Message::UseSettings(job.id));
        app.queue_master().unwrap();
        assert!(app.library.jobs[1].config.reference.is_some());
    }
    #[test]
    fn history_ui_selects_directly_without_a_comparison_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let messages = {
            let mut ui = iced_test::simulator(app.view());
            ui.find("日本語 mix.wav  · #0").unwrap();
            ui.click("Master 1 · -9 LUFS · Queued").unwrap();
            ui.into_messages()
        };
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.selected, Some((0, Some(1))));
        assert!(!app.show_settings);
    }
    #[test]
    fn finished_background_work_does_not_change_selection() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_track(dir.path());
        app.queue_master().unwrap();
        app.running = false;
        let _ = app.update(Message::Completed(1, Ok("checksum".into())));
        assert_eq!(app.selected, Some((0, None)));
        assert_eq!(app.library.jobs[0].status, Status::Succeeded);
        app.save();
        let loaded = Library::load(dir.path()).unwrap();
        assert_eq!(loaded.selected, Some((0, None)));
        assert_eq!(loaded.jobs[0].output_hash.as_deref(), Some("checksum"));
    }
}
