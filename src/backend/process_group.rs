// Stop the whole backend tree, including FFmpeg, before Child::kill_on_drop
// reaps the direct child. Windows has no Unix-style process-group kill.
pub(super) struct ProcessGroup(pub Option<u32>);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(pid) = self.0.take() {
            use std::os::windows::process::CommandExt;
            // Keep the parent alive until taskkill has discovered its children.
            let _ = std::process::Command::new("taskkill.exe")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(unix)]
        if let Some(pid) = self.0.take() {
            // SAFETY: this is the dedicated process group created for our child.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}
