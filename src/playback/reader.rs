//! One disk worker per cached source. Only the mixer thread consumes its PCM.
use super::Decoded;
use rtrb::{Consumer, RingBuffer};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
const AHEAD: usize = 22050; // Half a second of stereo PCM.
#[derive(Clone, Copy)]
struct Frame {
    generation: u64,
    position: u64,
    samples: [f32; 2],
}
struct Control {
    generation: AtomicU64,
    position: AtomicU64,
    start: AtomicU64,
    end: AtomicU64,
    stop: AtomicBool,
}
pub(super) struct Reader {
    samples: Consumer<Frame>,
    control: Arc<Control>,
    thread: JoinHandle<()>,
    generation: u64,
    frames: u64,
    frame: Option<Frame>,
    errors: mpsc::Receiver<String>,
}
impl Reader {
    pub fn new(audio: Arc<Decoded>, position: u64, region: Option<(u64, u64)>) -> Self {
        let frames = audio.frames;
        let region = region
            .filter(|&(a, b)| a < frames && b.min(frames) > a)
            .map(|(a, b)| (a, b.min(frames)));
        let (mut producer, samples) = RingBuffer::new(AHEAD);
        let control = Arc::new(Control {
            generation: AtomicU64::new(1),
            position: AtomicU64::new(position),
            start: AtomicU64::new(region.map_or(0, |r| r.0)),
            end: AtomicU64::new(region.map_or(0, |r| r.1)),
            stop: AtomicBool::new(false),
        });
        let c = control.clone();
        let (errors_tx, errors) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("audio-file-reader".into())
            .spawn(move || {
                let result = (|| -> Result<(), String> {
                    let mut file = BufReader::with_capacity(
                        65536,
                        File::open(&audio.path).map_err(|e| e.to_string())?,
                    );
                    let mut generation = 0;
                    let mut position = 0;
                    let mut bytes = [0u8; 8192];
                    while !c.stop.load(Ordering::Acquire) {
                        let requested = c.generation.load(Ordering::Acquire);
                        if requested != generation {
                            generation = requested;
                            position = c.position.load(Ordering::Relaxed).min(audio.frames);
                            file.seek(SeekFrom::Start(position * 8))
                                .map_err(|e| e.to_string())?;
                        }
                        let end = c.end.load(Ordering::Relaxed).min(audio.frames);
                        if end > 0 && position >= end {
                            position = c.start.load(Ordering::Relaxed);
                            file.seek(SeekFrom::Start(position * 8))
                                .map_err(|e| e.to_string())?;
                        }
                        let remaining = if end > 0 {
                            end.saturating_sub(position)
                        } else {
                            audio.frames.saturating_sub(position)
                        };
                        let count = producer
                            .slots()
                            .min(bytes.len() / 8)
                            .min(remaining as usize);
                        if count == 0 {
                            thread::park_timeout(Duration::from_millis(10));
                            continue;
                        }
                        file.read_exact(&mut bytes[..count * 8])
                            .map_err(|e| format!("Reading playback PCM: {e}"))?;
                        for (index, bytes) in
                            bytes[..count * 8].as_chunks::<8>().0.iter().enumerate()
                        {
                            let samples = [
                                f32::from_le_bytes(bytes[..4].try_into().unwrap()),
                                f32::from_le_bytes(bytes[4..].try_into().unwrap()),
                            ];
                            // Capacity cannot decrease: this thread is the only producer.
                            if producer
                                .push(Frame {
                                    generation,
                                    position: position + index as u64,
                                    samples,
                                })
                                .is_err()
                            {
                                return Err("Playback reader queue overflow.".into());
                            }
                        }
                        position += count as u64;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = errors_tx.send(error);
                }
            })
            .expect("create audio file reader");
        Self {
            samples,
            control,
            thread,
            generation: 1,
            frames,
            frame: None,
            errors,
        }
    }
    pub fn seek(&mut self, position: u64) {
        self.control.position.store(position, Ordering::Relaxed);
        self.generation = self.control.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.frame = None;
        self.thread.thread().unpark();
    }
    pub fn region(&mut self, region: Option<(u64, u64)>, position: u64) {
        let region = region
            .filter(|&(a, b)| a < self.frames && b.min(self.frames) > a)
            .map(|(a, b)| (a, b.min(self.frames)));
        self.control
            .start
            .store(region.map_or(0, |r| r.0), Ordering::Relaxed);
        self.control
            .end
            .store(region.map_or(0, |r| r.1), Ordering::Relaxed);
        self.seek(position);
    }
    pub fn at(&mut self, position: u64) -> Option<[f32; 2]> {
        if let Some(frame) = self.frame.take()
            && frame.generation == self.generation
            && frame.position == position
        {
            self.frame = Some(frame);
            return Some(frame.samples);
        }
        while let Ok(frame) = self.samples.pop() {
            if frame.generation != self.generation {
                continue;
            }
            if frame.position == position {
                self.frame = Some(frame);
                if position.is_multiple_of(1024) {
                    self.thread.thread().unpark();
                }
                return Some(frame.samples);
            }
            if frame.position > position || position - frame.position >= AHEAD as u64 {
                self.seek(position);
                return None;
            }
        }
        self.thread.thread().unpark();
        None
    }
    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        self.control.stop.store(true, Ordering::Release);
        self.thread.thread().unpark();
    }
}
