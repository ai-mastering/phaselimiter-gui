//! The only boundary to the in-process C audio library.
//!
//! Device ownership is private: it cannot be cloned or reconfigured after
//! starting. Always initialize a context before allocating a device; the
//! miniaudio wrapper's implicit-context failure path reads uninitialized memory.
//! C/driver faults cannot be caught by Rust's Result or catch_unwind. DSP and
//! decoding already run in child processes; device I/O necessarily stays here.
use super::{log, mixer};
use miniaudio::{Backend, Context, Device, DeviceConfig, DeviceType, Format};
use rtrb::Consumer;
use std::sync::Arc;

pub(super) struct PlaybackDevice(Device);

fn open_device(config: &DeviceConfig, backends: &[Backend]) -> Result<Device, String> {
    log("Initializing audio context");
    // An empty list selects the platform's default backend order, still with
    // explicit initialization before Device::new on Windows and Linux too.
    let context =
        Context::new(backends, None).map_err(|e| format!("Audio backend initialization: {e}"))?;
    log("Initializing playback device");
    Device::new(Some(context), config).map_err(|e| format!("Audio device: {e}"))
}

impl PlaybackDevice {
    pub fn open(
        queue: &mut Option<Consumer<mixer::Frame>>,
        shared: Arc<mixer::Shared>,
    ) -> Result<Self, String> {
        if queue.is_none() {
            return Err("Playback queue unavailable.".into());
        }
        let mut config = DeviceConfig::new(DeviceType::Playback);
        config.playback_mut().set_format(Format::F32);
        config.playback_mut().set_channels(2);
        config.set_sample_rate(super::RATE as u32);
        #[cfg(target_os = "macos")]
        let backends = &[Backend::CoreAudio][..];
        #[cfg(not(target_os = "macos"))]
        let backends = &[][..];
        let mut device = open_device(&config, backends)?;
        let mut queue = queue.take().ok_or("Playback queue unavailable.")?;
        // The device is exclusively owned and not started, satisfying the
        // wrapper's callback-installation preconditions. Never log in callback.
        device.set_data_callback(move |_, output, _| {
            if output.format() == Format::F32 {
                mixer::consume(output.as_samples_mut::<f32>(), &mut queue, &shared);
            } else {
                output.as_bytes_mut().fill(0);
            }
        });
        Ok(Self(device))
    }

    pub fn start(&self) -> Result<(), String> {
        if !self.0.is_started() {
            log("Starting playback device");
            self.0.start().map_err(|e| format!("Starting audio: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unavailable_backend_returns_error_without_allocating_device() {
        let config = DeviceConfig::new(DeviceType::Playback);
        let result = open_device(&config, &[Backend::CoreAudio]);
        assert!(matches!(result, Err(error) if error.starts_with("Audio backend initialization:")));
    }
}
