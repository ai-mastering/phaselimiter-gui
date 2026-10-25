// A separate process group also stops FFmpeg children on Unix when a task is
// cancelled or the window closes. Child::kill_on_drop handles the direct child
// on every platform.
pub(super) struct ProcessGroup(pub Option<u32>);
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
