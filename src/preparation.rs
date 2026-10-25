//! Playback selection, background decoding and the bounded disk-backed PCM cache.
use crate::{App, AudioKey, Message, analysis, library, model::Status, playback};
use iced::Task;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

// RAM prefetch has a separate mixer limit.
const PCM_CACHE_ENTRIES: usize = 100;

#[derive(Default)]
pub(super) struct Cache {
    entries: HashMap<PathBuf, Arc<playback::Decoded>>,
    pending: HashSet<PathBuf>,
    order: VecDeque<PathBuf>,
}
impl Cache {
    pub(super) fn contains_key(&self, path: &Path) -> bool {
        self.entries.contains_key(path)
    }
    fn get(&self, path: &Path) -> Option<Arc<playback::Decoded>> {
        self.entries.get(path).cloned()
    }
    fn touch(&mut self, path: &Path) {
        self.order.retain(|p| p != path);
        self.order.push_back(path.to_path_buf());
    }
    pub(super) fn insert(&mut self, path: PathBuf, audio: Arc<playback::Decoded>) {
        self.touch(&path);
        self.entries.insert(path, audio);
        while self.order.len() > PCM_CACHE_ENTRIES {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }
    fn begin(&mut self, path: PathBuf) -> bool {
        self.pending.insert(path)
    }
    fn finish(&mut self, path: &Path) {
        self.pending.remove(path);
    }
    #[cfg(test)]
    pub(super) fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }
}

impl App {
    pub(super) fn listening_key(&self) -> Option<AudioKey> {
        let (track, job) = self.selected?;
        Some((
            track,
            if self.output_side {
                job
            } else if job.is_some() {
                self.comparison_job().map(|j| j.id)
            } else {
                None
            },
        ))
    }
    pub(super) fn listening_analysis(&self) -> Option<&analysis::Analysis> {
        let (_, job) = self.listening_key()?;
        if let Some(id) = job {
            self.library
                .jobs
                .iter()
                .find(|j| j.id == id)?
                .analysis
                .as_ref()
        } else {
            self.track()?.analysis.as_ref()
        }
    }
    pub(super) fn selected_gain(&self) -> f32 {
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
    pub(super) fn update_gain(&self) {
        if !self.loading && self.listening_key() == self.active_key {
            self.player.gain(self.selected_gain());
        }
    }
    pub(super) fn activate(&mut self, audio: Arc<playback::Decoded>) {
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
    pub(super) fn request_decode(&mut self, path: PathBuf, hash: String) -> Task<Message> {
        if !self.prepared.begin(path.clone()) {
            return Task::none();
        }
        let cached = self.prepared.get(&path);
        let ffmpeg = self.library.app_settings.ffmpeg_command();
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
    pub(super) fn warm_selection(&mut self) -> Task<Message> {
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
    pub(super) fn prepare(&mut self) -> Task<Message> {
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
        if let Some(audio) = self.prepared.get(&path)
            && audio.unchanged(&path)
        {
            self.prepared.touch(&path);
            self.activate(audio);
            return self.warm_selection();
        }
        let task = self.request_decode(path, hash);
        let warm = self.warm_selection();
        Task::batch([task, warm])
    }

    pub(super) fn preparation_finished(
        &mut self,
        path: PathBuf,
        result: Result<Arc<playback::Decoded>, String>,
    ) {
        self.prepared.finish(&path);
        let wanted = self
            .listening_key()
            .and_then(|key| self.audio(key))
            .is_some_and(|(p, _)| p == path);
        match result {
            Ok(audio) => {
                let registered = self.library.tracks.iter().any(|t| t.input == path)
                    || self.library.jobs.iter().any(|j| j.output == path);
                if registered {
                    self.prepared.insert(path, audio.clone());
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_evicts_least_recently_used_and_coalesces_pending_decodes() {
        let mut cache = Cache::default();
        let audio = playback::test_audio(&[[0.2; 2]; 10], None);
        for id in 0..PCM_CACHE_ENTRIES {
            cache.insert(PathBuf::from(id.to_string()), audio.clone());
        }
        cache.touch(Path::new("0"));
        cache.insert(PathBuf::from("new"), audio);
        assert!(cache.contains_key(Path::new("0")));
        assert!(!cache.contains_key(Path::new("1")));
        assert!(cache.contains_key(Path::new("new")));
        let path = PathBuf::from("pending");
        assert!(cache.begin(path.clone()));
        assert!(!cache.begin(path.clone()));
        cache.finish(&path);
        assert!(cache.begin(path));
    }
}
