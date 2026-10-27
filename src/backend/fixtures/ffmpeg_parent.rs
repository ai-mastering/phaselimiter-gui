//! Standalone test backend, compiled by windows_tests with the installed rustc.
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let value = |flag: &str| args.windows(2).find(|pair| pair[0] == flag).unwrap()[1].clone();
    let output = PathBuf::from(value("--output"));
    std::fs::write(
        output.with_extension("parent.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    let mut child = Command::new(value("--ffmpeg"))
        .args([
            "-nostdin",
            "-v",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-t",
            "60",
            "-progress",
            "pipe:1",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    std::fs::write(output.with_extension("ffmpeg.pid"), child.id().to_string()).unwrap();
    // Announce readiness only once real FFmpeg is processing audio.
    let mut progress = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    while progress.read_line(&mut line).unwrap() != 0 {
        if line.starts_with("progress=") {
            println!("TEST_FFMPEG_READY");
            break;
        }
        line.clear();
    }
    // Keep the progress pipe open while the test cancels this process tree.
    assert!(child.wait().unwrap().success());
}
