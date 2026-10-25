//! Serialize and sync snapshots away from the GUI; coalesce rapid selection changes.
use super::Library;
use std::{
    path::PathBuf,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};
enum Command {
    Save(Box<Library>),
    Flush(mpsc::Sender<Result<(), String>>),
    Shutdown,
}
pub struct SaveWorker {
    sender: mpsc::Sender<Command>,
    results: mpsc::Receiver<Result<(), String>>,
    thread: Option<JoinHandle<()>>,
}
impl SaveWorker {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let (results_tx, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("library-save".into())
            .spawn(move || {
                let mut pending = None;
                let mut last = Ok(());
                loop {
                    let command = if pending.is_some() {
                        receiver.recv_timeout(Duration::from_millis(100))
                    } else {
                        receiver
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    };
                    match command {
                        Ok(Command::Save(snapshot)) => pending = Some(snapshot),
                        Ok(Command::Flush(reply)) => {
                            write(&root, &mut pending, &mut last, &results_tx);
                            let _ = reply.send(last.clone());
                        }
                        Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                            write(&root, &mut pending, &mut last, &results_tx);
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            write(&root, &mut pending, &mut last, &results_tx)
                        }
                    }
                }
            })
            .map_err(|e| format!("Starting library save worker: {e}"))?;
        Ok(Self {
            sender,
            results,
            thread: Some(thread),
        })
    }
    pub fn save(&self, snapshot: Library) -> Result<(), String> {
        self.sender
            .send(Command::Save(Box::new(snapshot)))
            .map_err(|_| "Library save worker stopped.".into())
    }
    pub fn flush(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Command::Flush(tx))
            .map_err(|_| "Library save worker stopped.".to_string())?;
        rx.recv()
            .map_err(|_| "Library save worker stopped.".to_string())?
    }
    pub fn error(&self) -> Option<String> {
        self.results.try_iter().last().and_then(Result::err)
    }
}
fn write(
    root: &std::path::Path,
    pending: &mut Option<Box<Library>>,
    last: &mut Result<(), String>,
    results: &mpsc::Sender<Result<(), String>>,
) {
    if let Some(snapshot) = pending.take() {
        *last = snapshot.save(root);
        let _ = results.send(last.clone());
    }
}
impl Drop for SaveWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flush_preserves_the_latest_selection_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let saver = SaveWorker::new(dir.path().to_path_buf()).unwrap();
        let mut state = Library {
            selected: Some((1, None)),
            ..Library::default()
        };
        saver.save(state.clone()).unwrap();
        state.selected = Some((2, Some(3)));
        state.settings.loudness = "-12".into();
        saver.save(state).unwrap();
        saver.flush().unwrap();
        let saved = Library::load(dir.path()).unwrap();
        assert_eq!(saved.selected, Some((2, Some(3))));
        assert_eq!(saved.settings.loudness, "-12");
    }
    #[test]
    fn save_errors_are_returned_by_flush_without_replacing_existing_data() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("file");
        std::fs::write(&root, b"original").unwrap();
        let saver = SaveWorker::new(root.clone()).unwrap();
        saver.save(Library::default()).unwrap();
        assert!(saver.flush().is_err());
        assert_eq!(std::fs::read(root).unwrap(), b"original");
    }
}
