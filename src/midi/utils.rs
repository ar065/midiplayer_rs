use std::sync::LazyLock;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

static START: LazyLock<Instant> = LazyLock::new(|| Instant::now());

/// Get the time in 100ns units
pub fn get_time_100ns() -> i64 {
    let duration = START.elapsed();

    let seconds = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos() as i64;

    (seconds * 10_000_000) + (nanos / 100)
}

/// Delay the thread execution using 100ns units
pub fn delay_execution_100ns(delay_in_100ns: i64) {
    if delay_in_100ns <= 0 {
        return;
    }

    let secs = delay_in_100ns / 10_000_000;
    let nanos = (delay_in_100ns % 10_000_000) * 100;

    let duration = Duration::new(secs as u64, nanos as u32);
    sleep(duration);
}

// // Funny stuff that allows us to keep the memory usage so low
// const KIND_BIT: u32 = 1 << 31;
// const DATA_MASK: u32 = 0x7FFFFFFF;

// #[inline(always)]
// pub fn pack_event(data: u32, is_tempo: bool) -> u32 {
//     if is_tempo {
//         data | KIND_BIT
//     } else {
//         data & DATA_MASK
//     }
// }

// #[inline(always)]
// pub fn unpack_event(packed: u32) -> (u32, bool) {
//     ((packed & DATA_MASK), (packed & KIND_BIT) != 0)
// }
