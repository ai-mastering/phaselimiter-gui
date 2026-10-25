//! The mixer is off the audio callback. It produces a short, lock-free output queue.
use super::{Decoded, reader::Reader};
use rtrb::{Consumer, Producer, RingBuffer};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub(super) const FADE: usize = 441; // 10 ms, in the shared 44.1 kHz sample clock.
const OUTPUT: usize = 16384; // Capacity for unusually large device callbacks; fill only two periods.
const CACHE: usize = 8;
#[derive(Debug, Clone, Copy)]
pub(super) struct Frame {
    pub samples: [f32; 2],
    pub position: u64,
    pub epoch: u64,
    pub ended: bool,
}
#[derive(Default)]
pub(super) struct Shared {
    pub playing: AtomicBool,
    pub position: AtomicU64,
    pub duration: AtomicU64,
    epoch: AtomicU64,
    looping: AtomicBool,
    block_frames: AtomicU64,
    pub underruns: AtomicU64,
    error: Mutex<Option<String>>,
}
pub(super) enum Command {
    Load(Arc<Decoded>, f32),
    Warm(Arc<Decoded>),
    Gain(f32),
    Seek(u64),
    Region(Option<(u64, u64)>),
    Shutdown,
}
struct Source {
    audio: Arc<Decoded>,
    reader: Reader,
    gain: f32,
    target_gain: f32,
    weight: f32,
    from: f32,
    used: u64,
}
struct Mixer {
    sources: Vec<Source>,
    active: Option<PathBuf>,
    pending: Option<PathBuf>,
    fade: usize,
    position: u64,
    epoch: u64,
    region: Option<(u64, u64)>,
    shared: Arc<Shared>,
    clock: u64,
}
impl Mixer {
    fn new(shared: Arc<Shared>) -> Self {
        Self {
            sources: Vec::new(),
            active: None,
            pending: None,
            fade: FADE,
            position: 0,
            epoch: 0,
            region: None,
            shared,
            clock: 0,
        }
    }
    fn source(&mut self, audio: Arc<Decoded>) -> usize {
        self.clock += 1;
        if let Some(i) = self.sources.iter().position(|s| s.audio.path == audio.path) {
            self.sources[i].used = self.clock;
            return i;
        }
        if self.sources.len() >= CACHE
            && let Some(i) = self
                .sources
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.weight == 0.
                        && self.pending.as_ref() != Some(&s.audio.path)
                        && self.active.as_ref() != Some(&s.audio.path)
                })
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i)
        {
            self.sources.remove(i);
        }
        let reader = Reader::new(audio.clone(), self.position, self.region);
        self.sources.push(Source {
            audio,
            reader,
            gain: 1.,
            target_gain: 1.,
            weight: 0.,
            from: 0.,
            used: self.clock,
        });
        self.sources.len() - 1
    }
    fn invalidate(&mut self, position: u64) {
        self.position = position;
        self.epoch = self.shared.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.shared.position.store(position, Ordering::Release);
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Load(audio, gain) => {
                if !self.shared.playing.load(Ordering::Acquire) {
                    self.invalidate(self.shared.position.load(Ordering::Acquire));
                }
                let i = self.source(audio);
                if self.sources[i].weight == 0. {
                    self.sources[i].gain = gain;
                }
                self.sources[i].target_gain = gain;
                self.pending = Some(self.sources[i].audio.path.clone());
            }
            Command::Warm(audio) => {
                self.source(audio);
            }
            Command::Gain(gain) => {
                let target = self.pending.as_ref().or(self.active.as_ref());
                if let Some(s) = self
                    .sources
                    .iter_mut()
                    .find(|s| Some(&s.audio.path) == target)
                {
                    s.target_gain = gain;
                }
            }
            Command::Seek(position) => {
                let position = position.min(self.shared.duration.load(Ordering::Acquire));
                let position = if let Some((start, end)) = self.region {
                    if position < start || position >= end {
                        start
                    } else {
                        position
                    }
                } else {
                    position
                };
                self.invalidate(position);
                for source in &mut self.sources {
                    source.reader.seek(position);
                }
            }
            Command::Region(region) => {
                let duration = self
                    .pending
                    .as_ref()
                    .or(self.active.as_ref())
                    .and_then(|p| self.sources.iter().find(|s| &s.audio.path == p))
                    .map_or(u64::MAX, |s| s.audio.frames);
                let region = region
                    .filter(|&(a, b)| a < duration && b.min(duration) > a)
                    .map(|(a, b)| (a, b.min(duration)));
                if self.region == region {
                    return;
                }
                self.region = region;
                if let Some((start, end)) = region
                    && (self.position < start || self.position >= end)
                {
                    self.invalidate(start);
                }
                self.shared
                    .looping
                    .store(region.is_some(), Ordering::Release);
                for source in &mut self.sources {
                    source.reader.region(region, self.position);
                }
            }
            Command::Shutdown => {}
        }
    }
    fn activate(&mut self) -> bool {
        let Some(path) = self.pending.as_ref() else {
            return true;
        };
        let Some(i) = self.sources.iter().position(|s| &s.audio.path == path) else {
            return false;
        };
        let frames = self.sources[i].audio.frames;
        if self.position < frames && self.sources[i].reader.at(self.position).is_none() {
            return false;
        }
        let path = self.sources[i].audio.path.clone();
        if self.active.as_ref() != Some(&path) {
            for source in &mut self.sources {
                source.from = source.weight;
            }
            if self.active.is_none() || !self.shared.playing.load(Ordering::Acquire) {
                for source in &mut self.sources {
                    source.weight = if source.audio.path == path { 1. } else { 0. };
                    source.from = source.weight;
                }
                self.fade = FADE;
            } else {
                self.fade = 0;
            }
            self.active = Some(path);
        }
        self.pending = None;
        self.shared.duration.store(frames, Ordering::Release);
        if self.position >= frames {
            self.invalidate(frames);
            self.shared.playing.store(false, Ordering::Release);
        }
        true
    }
    fn render(&mut self) -> Option<Frame> {
        self.activate(); // Keep the old source playing until the target has samples.
        let path = self.active.as_ref()?;
        let duration = self
            .sources
            .iter()
            .find(|s| &s.audio.path == path)?
            .audio
            .frames;
        if let Some((start, end)) = self.region
            && self.position >= end.min(duration)
        {
            self.position = start.min(duration);
        }
        if self.position >= duration {
            return None;
        }
        // Compute candidate weights first; an underrun must not advance the fade.
        let step = (self.fade + 1).min(FADE);
        let t = step as f32 / FADE as f32;
        let t = t * t * (3. - 2. * t);
        let mut samples = [0.; 2];
        for source in &mut self.sources {
            let target = if Some(&source.audio.path) == self.active.as_ref() {
                1.
            } else {
                0.
            };
            let weight = if self.fade >= FADE {
                source.weight
            } else {
                source.from + (target - source.from) * t
            };
            if weight.abs() < 1e-8 {
                continue;
            }
            let pcm = if self.position >= source.audio.frames {
                [0.; 2]
            } else {
                source.reader.at(self.position)?
            };
            let gain = source.gain
                + (source.target_gain - source.gain).clamp(-1. / FADE as f32, 1. / FADE as f32);
            for c in 0..2 {
                samples[c] += pcm[c] * gain * weight;
            }
        }
        for source in &mut self.sources {
            let target = if Some(&source.audio.path) == self.active.as_ref() {
                1.
            } else {
                0.
            };
            if self.fade < FADE {
                source.weight = source.from + (target - source.from) * t;
            }
            source.gain +=
                (source.target_gain - source.gain).clamp(-1. / FADE as f32, 1. / FADE as f32);
        }
        self.fade = step;
        if self.fade == FADE {
            while self.sources.len() > CACHE {
                let victim = self
                    .sources
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.weight == 0. && self.pending.as_ref() != Some(&s.audio.path))
                    .min_by_key(|(_, s)| s.used)
                    .map(|(i, _)| i);
                if let Some(i) = victim {
                    self.sources.remove(i);
                } else {
                    break;
                }
            }
        }
        self.position += 1;
        Some(Frame {
            samples,
            position: self.position,
            epoch: self.epoch,
            ended: self.position >= duration && self.region.is_none(),
        })
    }
    fn error(&self) -> Option<String> {
        self.sources.iter().find_map(|s| s.reader.error())
    }
}
pub(super) struct Engine {
    pub shared: Arc<Shared>,
    pub output: Option<Consumer<Frame>>,
    sender: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}
impl Default for Engine {
    fn default() -> Self {
        let shared = Arc::new(Shared::default());
        let s = shared.clone();
        let (producer, consumer) = RingBuffer::new(OUTPUT);
        let (sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("audio-mixer".into())
            .spawn(move || run(s, producer, receiver))
            .expect("create audio mixer");
        Self {
            shared,
            output: Some(consumer),
            sender,
            thread: Some(thread),
        }
    }
}
impl Engine {
    pub fn send(&self, command: Command) {
        let _ = self.sender.send(command);
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().take()
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn run(shared: Arc<Shared>, mut output: Producer<Frame>, commands: mpsc::Receiver<Command>) {
    let mut mixer = Mixer::new(shared.clone());
    loop {
        for command in commands.try_iter() {
            if matches!(command, Command::Shutdown) {
                return;
            }
            mixer.command(command);
        }
        if let Some(error) = mixer.error() {
            *shared.error.lock().unwrap() = Some(error);
        }
        mixer.activate();
        if shared.playing.load(Ordering::Acquire) {
            let target =
                (shared.block_frames.load(Ordering::Acquire) as usize * 2).clamp(512, OUTPUT);
            let queued = OUTPUT - output.slots();
            for _ in 0..target.saturating_sub(queued).min(1024) {
                if !shared.playing.load(Ordering::Acquire) {
                    break;
                }
                if let Some(frame) = mixer.render() {
                    let _ = output.push(frame);
                } else {
                    break;
                }
            }
        }
        thread::park_timeout(Duration::from_millis(1));
    }
}
// The device callback only copies from a lock-free ring. No file operations,
// waiting mutexes, allocations, or source destruction occur here.
pub(super) fn consume(output: &mut [f32], queue: &mut Consumer<Frame>, shared: &Shared) {
    output.fill(0.);
    shared
        .block_frames
        .store((output.len() / 2) as u64, Ordering::Release);
    if !shared.playing.load(Ordering::Acquire) {
        return;
    }
    for frame in output.as_chunks_mut::<2>().0 {
        let next = loop {
            match queue.pop() {
                Ok(next) if next.epoch == shared.epoch.load(Ordering::Acquire) => break next,
                Ok(_) => continue,
                Err(_) => {
                    shared.underruns.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
        };
        *frame = next.samples;
        shared.position.store(next.position, Ordering::Release);
        if next.ended
            && next.position >= shared.duration.load(Ordering::Acquire)
            && !shared.looping.load(Ordering::Acquire)
        {
            shared.playing.store(false, Ordering::Release);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::test_audio;
    fn wait_for(mut condition: impl FnMut() -> bool) {
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(
                std::time::Instant::now() < end,
                "PCM reader did not become ready"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn next(m: &mut Mixer) -> Frame {
        let mut result = None;
        wait_for(|| {
            result = m.render();
            result.is_some()
        });
        result.unwrap()
    }
    fn start(audio: Arc<Decoded>) -> Mixer {
        let mut m = Mixer::new(Arc::new(Shared::default()));
        m.command(Command::Load(audio, 1.));
        wait_for(|| m.activate() && m.active.is_some());
        m.shared.playing.store(true, Ordering::Release);
        m
    }
    fn warm(m: &mut Mixer, audio: Arc<Decoded>) {
        m.command(Command::Warm(audio));
        wait_for(|| {
            m.sources
                .iter_mut()
                .all(|s| s.reader.at(m.position).is_some())
        });
    }
    #[test]
    fn identical_sources_stay_sample_aligned_through_a_crossfade() {
        let samples: Vec<_> = (0..3000)
            .map(|i| {
                let x = (i % 37) as f32 / 50.;
                [x, -x]
            })
            .collect();
        let a = test_audio(&samples, None);
        let b = test_audio(&samples, None);
        let mut m = start(a);
        warm(&mut m, b.clone());
        for _ in 0..137 {
            next(&mut m);
        }
        m.command(Command::Load(b, 1.));
        for _ in 0..FADE + 30 {
            let frame = next(&mut m);
            let expected = samples[frame.position as usize - 1];
            for (actual, expected) in frame.samples.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-6);
            }
        }
    }
    #[test]
    fn fade_and_rapid_reversal_have_no_gain_jump() {
        let a = test_audio(&vec![[0., 0.]; 3000], None);
        let b = test_audio(&vec![[1., 1.]; 3000], None);
        let mut m = start(a.clone());
        warm(&mut m, b.clone());
        m.command(Command::Load(b, 1.));
        let mut previous = 0.;
        for _ in 0..100 {
            let value = next(&mut m).samples[0];
            assert!((value - previous).abs() < 0.004);
            previous = value;
        }
        assert!(previous > 0.);
        m.command(Command::Load(a, 1.));
        for _ in 0..FADE {
            let value = next(&mut m).samples[0];
            assert!((value - previous).abs() < 0.004);
            previous = value;
        }
        assert!(previous.abs() < 1e-6);
    }
    #[test]
    fn an_unready_target_does_not_stop_the_current_source() {
        let a = test_audio(&vec![[0.3, 0.4]; 1000], None);
        let b = test_audio(&vec![[1., 1.]; 1000], None);
        std::fs::remove_file(&b.path).unwrap();
        let mut m = start(a);
        m.command(Command::Load(b, 1.));
        for position in 1..100 {
            let frame = next(&mut m);
            assert_eq!(frame.position, position);
            assert_eq!(frame.samples, [0.3, 0.4]);
        }
        assert!(m.shared.playing.load(Ordering::Acquire));
    }
    #[test]
    fn seek_and_loop_use_exact_pcm_frames() {
        let samples: Vec<_> = (0..10).map(|i| [i as f32, i as f32]).collect();
        let mut m = start(test_audio(&samples, None));
        m.command(Command::Region(Some((2, 5))));
        m.command(Command::Seek(2));
        for expected in [2., 3., 4., 2., 3., 4., 2., 3.] {
            assert_eq!(next(&mut m).samples, [expected; 2]);
        }
    }
    #[test]
    fn callback_discards_stale_audio_and_stops_at_eof() {
        let (mut producer, mut consumer) = RingBuffer::new(8);
        let shared = Shared::default();
        shared.playing.store(true, Ordering::Release);
        shared.epoch.store(1, Ordering::Release);
        producer
            .push(Frame {
                samples: [9.; 2],
                position: 99,
                epoch: 0,
                ended: false,
            })
            .unwrap();
        producer
            .push(Frame {
                samples: [0.2, 0.4],
                position: 1,
                epoch: 1,
                ended: true,
            })
            .unwrap();
        let mut output = [7.; 6];
        consume(&mut output, &mut consumer, &shared);
        assert_eq!(output, [0.2, 0.4, 0., 0., 0., 0.]);
        assert_eq!(shared.position.load(Ordering::Acquire), 1);
        assert!(!shared.playing.load(Ordering::Acquire));
    }
    #[test]
    fn rapid_switching_releases_retired_sources_after_the_fade() {
        let mut m = start(test_audio(&vec![[0.1; 2]; 5000], None));
        for _ in 0..12 {
            let audio = test_audio(&vec![[0.1; 2]; 5000], None);
            warm(&mut m, audio.clone());
            m.command(Command::Load(audio, 1.));
            next(&mut m);
        }
        for _ in 0..FADE {
            next(&mut m);
        }
        assert!(m.sources.len() <= CACHE);
    }
    #[test]
    fn output_queue_adapts_to_large_device_callbacks() {
        let mut engine = Engine::default();
        let mut queue = engine.output.take().unwrap();
        engine.shared.block_frames.store(2048, Ordering::Release);
        engine.send(Command::Load(
            test_audio(&vec![[0.25, -0.25]; 30000], None),
            1.,
        ));
        engine.shared.playing.store(true, Ordering::Release);
        let mut output = vec![0.; 4096];
        for _ in 0..5 {
            wait_for(|| queue.slots() >= 4096);
            consume(&mut output, &mut queue, &engine.shared);
            for frame in output.as_chunks::<2>().0 {
                assert_eq!(*frame, [0.25, -0.25]);
            }
        }
        assert_eq!(engine.shared.underruns.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn shorter_target_stops_at_its_end_instead_of_waiting_forever() {
        let mut m = start(test_audio(&vec![[0.5; 2]; 1000], None));
        for _ in 0..20 {
            next(&mut m);
        }
        m.command(Command::Load(test_audio(&[[0.1; 2]; 2], None), 1.));
        m.activate();
        assert!(!m.shared.playing.load(Ordering::Acquire));
        assert_eq!(m.shared.position.load(Ordering::Acquire), 2);
    }
}
