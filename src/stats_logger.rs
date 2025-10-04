use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub struct StatsLogger {
    fps: usize,
    buffer_size: usize,
    history: Vec<AtomicU32>,
    current_frame: AtomicUsize,
}

impl StatsLogger {
    pub fn new(fps: usize) -> Self {
        Self {
            fps,
            buffer_size: fps,
            history: (0..fps).map(|_| AtomicU32::new(0)).collect(),
            current_frame: AtomicUsize::new(0),
        }
    }

    pub fn increment(&self, by: u32) {
        let idx = self.current_frame.load(Ordering::Relaxed);
        self.history[idx].fetch_add(by, Ordering::Relaxed);
    }

    pub fn next_frame(&self) {
        let next = (self.current_frame.load(Ordering::Relaxed) + 1) % self.buffer_size;
        self.current_frame.store(next, Ordering::Relaxed);
        self.history[next].store(0, Ordering::Relaxed);
    }

    pub fn get_eps(&self) -> u32 {
        self.history
            .iter()
            .map(|x| x.load(Ordering::Relaxed))
            .sum()
    }
}
