use crate::model::Job;
use iced::futures::{SinkExt, Stream};
use std::ffi::OsString;
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone)]
pub enum Update {
    Line(String),
    Progress(f32),
    Finished(Result<(), String>),
    Cancelled,
}

pub fn arguments(job: &Job) -> Vec<OsString> {
    let c = &job.config;
    let mut args = Vec::new();
    let mut flag = |name: &str, value: OsString| {
        args.push(format!("--{name}").into());
        args.push(value);
    };
    flag("input", job.input.clone().into_os_string());
    flag("output", job.output.clone().into_os_string());
    flag("ffmpeg", c.ffmpeg.clone().into());
    flag("sound_quality2_cache", c.cache.clone().into_os_string());
    for (name, value) in [
        ("mastering", "true".into()),
        ("mastering_mode", "mastering5".into()),
        ("reference", c.loudness.to_string()),
        ("reference_mode", "loudness".into()),
        ("mastering_matching_level", c.intensity.to_string()),
        ("mastering_ms_matching_level", c.stereo.to_string()),
        ("mastering5_mastering_level", c.intensity.to_string()),
        ("erb_eval_func_weighting", c.preserve_bass.to_string()),
        ("pre_compression", c.pre_compression.to_string()),
        ("ceiling_mode", "true_peak".into()),
        ("ceiling", c.ceiling.to_string()),
        ("output_format", c.format.extension().into()),
        ("sample_rate", c.sample_rate.to_string()),
        ("bit_depth", c.bit_depth.to_string()),
        ("worker_count", c.workers.to_string()),
        (
            "mastering5_optimization_algorithm",
            c.solver.argument().into(),
        ),
        (
            "mastering5_optimization_max_eval_count",
            c.evaluations.to_string(),
        ),
        ("mastering5_optimization_seed", c.seed.to_string()),
    ] {
        flag(name, value.into());
    }
    args
}

pub fn progress(line: &str) -> Option<f32> {
    let (_, rest) = line.split_once("progression:")?;
    let value = rest.split_whitespace().next()?.parse::<f32>().ok()?;
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

// A separate process group also stops FFmpeg children on Unix when a task is
// cancelled or the window closes. Child::kill_on_drop handles the direct child
// on every platform.
struct ProcessGroup(Option<u32>);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0.take() {
            // SAFETY: this is the dedicated process group created for our child.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

pub fn run(job: Job, cancel: Arc<AtomicBool>) -> impl Stream<Item = Update> {
    iced::stream::channel(100, async move |mut sender| {
        let result = execute(&job, cancel, &mut sender).await;
        let _ = sender.send(result).await;
    })
}

async fn execute(
    job: &Job,
    cancel: Arc<AtomicBool>,
    sender: &mut iced::futures::channel::mpsc::Sender<Update>,
) -> Update {
    if job.output.exists() {
        return Update::Finished(Err(format!(
            "Output already exists; refusing to overwrite: {}",
            job.output.display()
        )));
    }
    let scratch = match tempfile::Builder::new().prefix("phaselimiter-").tempdir() {
        Ok(dir) => dir,
        Err(e) => return Update::Finished(Err(format!("Creating temporary directory: {e}"))),
    };
    let mut command = tokio::process::Command::new(&job.config.backend);
    command
        .args(arguments(job))
        .arg("--tmp")
        .arg(scratch.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Update::Finished(Err(format!(
                "Could not start {}: {e}",
                job.config.backend.display()
            )));
        }
    };
    let mut group = ProcessGroup(child.id());
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    let mut stderr = BufReader::new(child.stderr.take().expect("piped stderr")).lines();
    let (mut stdout_done, mut stderr_done) = (false, false);
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
    while !stdout_done || !stderr_done {
        let line = tokio::select! {
            _ = tick.tick() => {
                if cancel.load(Ordering::Relaxed) {
                    drop(group);
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Update::Cancelled;
                }
                continue;
            }
            line = stdout.next_line(), if !stdout_done => match line {
                Ok(Some(line)) => line,
                Ok(None) => { stdout_done = true; continue; }
                Err(e) => return Update::Finished(Err(format!("Reading backend stdout: {e}"))),
            },
            line = stderr.next_line(), if !stderr_done => match line {
                Ok(Some(line)) => line,
                Ok(None) => { stderr_done = true; continue; }
                Err(e) => return Update::Finished(Err(format!("Reading backend stderr: {e}"))),
            },
        };
        if let Some(value) = progress(&line) {
            let _ = sender.send(Update::Progress(value)).await;
        }
        if sender
            .send(Update::Line(line.chars().take(2000).collect()))
            .await
            .is_err()
        {
            return Update::Cancelled;
        }
    }
    let status = loop {
        tokio::select! {
            status = child.wait() => break status,
            _ = tick.tick() => if cancel.load(Ordering::Relaxed) {
                drop(group); let _ = child.kill().await; let _ = child.wait().await;
                return Update::Cancelled;
            },
        }
    };
    // The child has been reaped; do not signal a subsequently reused process ID.
    group.0 = None;
    match status {
        Ok(status) if status.success() && job.output.is_file() => Update::Finished(Ok(())),
        Ok(status) if status.success() => Update::Finished(Err(
            "Backend exited successfully but did not create the output file.".into(),
        )),
        Ok(status) => Update::Finished(Err(format!(
            "Backend exited with {status}. See the job log."
        ))),
        Err(e) => Update::Finished(Err(format!("Waiting for backend: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_backend_progress_and_rejects_non_finite_values() {
        assert_eq!(progress("stage: progression: 5e-1"), Some(0.5));
        assert_eq!(progress("progression: 2"), Some(1.0));
        assert_eq!(progress("progression: NaN"), None);
        assert_eq!(progress("ordinary log message"), None);
    }

    #[cfg(unix)]
    fn fake_job(script: &str, dir: &std::path::Path) -> Job {
        use std::os::unix::fs::PermissionsExt;
        let backend = dir.join("fake engine");
        std::fs::write(&backend, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o755)).unwrap();
        let input = dir.join("日本語 input.wav");
        std::fs::write(&input, b"audio").unwrap();
        let settings = crate::model::Settings {
            backend: backend.to_string_lossy().into_owned(),
            output_dir: dir.to_string_lossy().into_owned(),
            ..crate::model::Settings::default()
        };
        let mut jobs = Vec::new();
        crate::model::add_job(&mut jobs, input, settings.parse().unwrap(), 0).unwrap();
        jobs.remove(0)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn streams_both_pipes_and_passes_unicode_paths_without_a_shell() {
        use iced::futures::StreamExt;
        let dir = tempfile::tempdir().unwrap();
        let job = fake_job(
            "while [ \"$#\" -gt 0 ]; do\n case \"$1\" in --output) output=\"$2\";; esac\n shift\ndone\necho 'progression: 0.5'\necho 'diagnostic' >&2\nprintf result > \"$output\"",
            dir.path(),
        );
        let output = job.output.clone();
        let updates: Vec<_> = run(job, Arc::new(AtomicBool::new(false))).collect().await;
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, Update::Progress(p) if *p == 0.5))
        );
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, Update::Line(s) if s == "diagnostic"))
        );
        assert!(matches!(updates.last(), Some(Update::Finished(Ok(())))));
        assert_eq!(std::fs::read(output).unwrap(), b"result");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reports_failure_and_cancels_a_silent_process() {
        use iced::futures::StreamExt;
        let dir = tempfile::tempdir().unwrap();
        let failed: Vec<_> = run(
            fake_job("exit 7", dir.path()),
            Arc::new(AtomicBool::new(false)),
        )
        .collect()
        .await;
        assert!(matches!(failed.last(), Some(Update::Finished(Err(_)))));
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            flag.store(true, Ordering::Relaxed);
        });
        let events = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            run(fake_job("sleep 30", dir.path()), cancel).collect::<Vec<_>>(),
        )
        .await
        .unwrap();
        assert!(matches!(events.last(), Some(Update::Cancelled)));
    }
}
