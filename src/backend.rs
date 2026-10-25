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
    Command(String),
    Line(String),
    Progress(f32),
    LimitingError(f64),
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
    if let Some(reference) = &c.reference {
        flag(
            "mastering5_mastering_reference_file",
            reference.clone().into_os_string(),
        );
    }
    flag(
        "limiting_error_spectrogram_output",
        job.output
            .with_extension("limiting-error.png")
            .into_os_string(),
    );
    for (name, value) in [
        ("mastering", c.mastering.to_string()),
        ("low_cut_freq", c.low_cut.to_string()),
        ("high_cut_freq", c.high_cut.to_string()),
        ("lowpass_true_peak_cut_freq", c.lowpass_cut.to_string()),
        ("limiter_external_oversample", c.oversample.to_string()),
        ("limiter_internal_oversample", "1".into()),
        ("max_iter1", c.max_iter1.to_string()),
        ("max_iter2", c.max_iter2.to_string()),
        ("noise_update_mode", c.noise_mode.argument().into()),
        ("mastering_mode", "mastering5".into()),
        ("limiting_mode", "phase".into()),
        ("reference", c.loudness.to_string()),
        ("reference_mode", c.reference_mode.argument().into()),
        ("mastering_matching_level", c.intensity.to_string()),
        ("mastering_ms_matching_level", c.intensity.to_string()),
        ("mastering5_mastering_level", c.intensity.to_string()),
        ("erb_eval_func_weighting", c.preserve_bass.to_string()),
        ("pre_compression", c.pre_compression.to_string()),
        ("ceiling_mode", c.ceiling_mode.argument().into()),
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

// Display the actual argv in shell syntax; execution still bypasses the shell.
fn command_text(command: &std::process::Command) -> String {
    fn quote(value: &std::ffi::OsStr) -> String {
        let value = value.to_string_lossy();
        if cfg!(windows) {
            format!("'{}'", value.replace("'", "''"))
        } else {
            format!("'{}'", value.replace("'", "'\"'\"'"))
        }
    }
    let prefix = if cfg!(windows) { "& " } else { "" };
    format!(
        "{prefix}{}",
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(quote)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

pub fn progress(line: &str) -> Option<f32> {
    let (_, rest) = line.split_once("progression:")?;
    let value = rest.split_whitespace().next()?.parse::<f32>().ok()?;
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

// Unix process-group signaling is the only unsafe operation in this crate.
#[allow(unsafe_code)]
mod process_group;
use process_group::ProcessGroup;

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
    let input = job.input.clone();
    let input_hash = job.input_hash.clone();
    let reference = job.config.reference.clone();
    let reference_hash = job.reference_hash.clone();
    let checked = tokio::task::spawn_blocking(move || -> Result<(), String> {
        if let Some(hash) = input_hash {
            crate::library::verify(&input, &hash)?;
        }
        if let (Some(path), Some(hash)) = (reference, reference_hash) {
            crate::library::verify(&path, &hash)?;
        }
        Ok(())
    })
    .await;
    match checked {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Update::Finished(Err(error)),
        Err(error) => return Update::Finished(Err(error.to_string())),
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
    if sender
        .send(Update::Command(command_text(command.as_std())))
        .await
        .is_err()
    {
        return Update::Cancelled;
    }
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
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Update::Finished(Err("Backend output pipes unavailable.".into()));
    };
    let mut stdout = BufReader::new(stdout).lines();
    let mut stderr = BufReader::new(stderr).lines();
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
        if let Some((_, tail)) = line.split_once("limiting_error:")
            && let Some(value) = tail
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        {
            let _ = sender.send(Update::LimitingError(value)).await;
        }
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
    fn passes_processing_choices_and_one_shared_mastering_level() {
        let dir = tempfile::tempdir().unwrap();
        let settings = crate::model::Settings {
            output_dir: dir.path().display().to_string(),
            mastering: false,
            intensity: "0.3".into(),
            noise_mode: crate::model::NoiseMode::Adaptive,
            reference_mode: crate::model::ReferenceMode::Rms,
            ceiling_mode: crate::model::CeilingMode::Peak,
            oversample: "4".into(),
            ..crate::model::Settings::default()
        };
        let job = crate::model::test_job(
            dir.path().join("input.wav"),
            dir.path().join("output.wav"),
            settings.parse().unwrap(),
            0,
        );
        let args = arguments(&job);
        for (flag, expected) in [
            ("mastering", "false"),
            ("mastering_matching_level", "0.3"),
            ("mastering_ms_matching_level", "0.3"),
            ("mastering5_mastering_level", "0.3"),
            ("reference_mode", "rms"),
            ("ceiling_mode", "peak"),
            ("noise_update_mode", "adaptive"),
            ("limiter_external_oversample", "4"),
            ("limiter_internal_oversample", "1"),
            ("max_iter1", "100"),
            ("max_iter2", "400"),
        ] {
            let name = OsString::from(format!("--{flag}"));
            let pair = args.windows(2).find(|pair| pair[0] == name).unwrap();
            assert_eq!(pair[1], OsString::from(expected));
        }
    }

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
        crate::model::test_job(input, dir.join("output.wav"), settings.parse().unwrap(), 0)
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
        let captured = updates
            .iter()
            .find_map(|update| match update {
                Update::Command(command) => Some(command),
                _ => None,
            })
            .unwrap();
        assert!(captured.contains("日本語 input.wav"));
        assert!(captured.contains("'--tmp'"));
        assert!(captured.contains("phaselimiter-"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_existing_outputs_and_modified_inputs() {
        use iced::futures::StreamExt;
        let dir = tempfile::tempdir().unwrap();
        let mut job = fake_job("exit 0", dir.path());
        std::fs::write(&job.output, b"keep me").unwrap();
        let updates: Vec<_> = run(job.clone(), Arc::new(AtomicBool::new(false)))
            .collect()
            .await;
        assert!(matches!(updates.last(), Some(Update::Finished(Err(_)))));
        assert_eq!(std::fs::read(&job.output).unwrap(), b"keep me");
        std::fs::remove_file(&job.output).unwrap();
        job.input_hash = Some(crate::library::hash(&job.input).unwrap());
        std::fs::write(&job.input, b"modified").unwrap();
        let updates: Vec<_> = run(job, Arc::new(AtomicBool::new(false))).collect().await;
        assert!(
            matches!(updates.last(),Some(Update::Finished(Err(error))) if error.contains("Stored file has changed"))
        );
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
