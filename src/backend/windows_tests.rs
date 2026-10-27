use super::*;
use iced::futures::StreamExt;
use std::{path::PathBuf, process::Command, time::Duration};

fn running(pid: u32) -> bool {
    let output = Command::new("tasklist.exe")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.split(',').nth(1) == Some(format!("\"{pid}\"").as_str()))
}

// Also clean up on assertion failure, so a regression never leaks a CI process.
struct Cleanup(PathBuf);
impl Cleanup {
    fn pid(&self, kind: &str) -> u32 {
        std::fs::read_to_string(self.0.with_extension(kind))
            .unwrap()
            .parse()
            .unwrap()
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        for kind in ["parent.pid", "ffmpeg.pid"] {
            if let Ok(pid) = std::fs::read_to_string(self.0.with_extension(kind)) {
                let _ = Command::new("taskkill.exe")
                    .args(["/PID", pid.trim(), "/T", "/F"])
                    .output();
            }
        }
    }
}

#[tokio::test]
async fn cancelling_backend_terminates_real_ffmpeg_child() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("ffmpeg_parent.rs");
    let executable = dir.path().join("ffmpeg_parent.exe");
    std::fs::write(&source, include_str!("fixtures/ffmpeg_parent.rs")).unwrap();
    let compiled = Command::new("rustc")
        .arg(&source)
        .args(["--edition=2024", "-o"])
        .arg(&executable)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let ffmpeg = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("ffmpeg.exe"))
        .find(|path| path.is_file())
        .expect("Install FFmpeg on PATH to run the Windows cancellation test");
    let mut job = crate::model::test_job(
        dir.path().join("input.wav"),
        dir.path().join("output.wav"),
        crate::model::Settings::default().parse().unwrap(),
        0,
    );
    job.tools.backend = executable;
    job.tools.ffmpeg = ffmpeg.to_string_lossy().into_owned();
    let cleanup = Cleanup(job.output.clone());
    let cancel = Arc::new(AtomicBool::new(false));
    let events = run(job, cancel.clone());
    iced::futures::pin_mut!(events);
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match events
                .next()
                .await
                .expect("backend ended before FFmpeg was ready")
            {
                Update::Line(line) if line == "TEST_FFMPEG_READY" => break,
                Update::Finished(result) => panic!("backend failed: {result:?}"),
                _ => {}
            }
        }
    })
    .await
    .expect("FFmpeg startup timed out");
    let parent = cleanup.pid("parent.pid");
    let child = cleanup.pid("ffmpeg.pid");
    assert!(running(parent));
    assert!(running(child));
    cancel.store(true, Ordering::Relaxed);
    let remaining = tokio::time::timeout(Duration::from_secs(15), events.collect::<Vec<_>>())
        .await
        .expect("cancellation timed out");
    assert!(matches!(remaining.last(), Some(Update::Cancelled)));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while running(parent) || running(child) {
        assert!(
            std::time::Instant::now() < deadline,
            "backend or FFmpeg survived cancellation"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
