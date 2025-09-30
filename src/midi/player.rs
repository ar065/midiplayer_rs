use crate::midi::track_data::TrackData;
use crate::midi::utils::{delay_execution_100ns, get_time_100ns, pack_event, unpack_event};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use thousands::Separable;

#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub events: Vec<u32>,
    pub deltas: Vec<(u32, u32)>,
    pub total_ticks: u64,
    pub total_duration: Duration,
    pub note_count: u64,
}

pub fn parse_midi_events(
    mut tracks: Vec<TrackData>,
    time_div: u16,
    min_velocity: u8,
) -> ParsedMidi {
    let mut events: Vec<u32> = Vec::with_capacity(1024);
    let mut deltas: Vec<(u32, u32)> = Vec::with_capacity(128);

    let mut tick: u64 = 0;
    let mut bpm_us_per_qn: u64 = 500_000;
    let mut multiplier: f64 = 0.0;

    let mut total_ticks: u64 = 0;
    let mut note_count: u64 = 0;
    let mut total_us_acc: u128 = 0;

    let total_tracks = tracks.len();
    let mut finished_tracks: u128 = 0;

    loop {
        let mut any_active = false;
        let group_start = events.len() as u32;

        for track in tracks.iter_mut() {
            if track.length == 0 {
                continue;
            }
            any_active = true;

            if track.tick <= tick {
                while track.length > 0 && track.tick <= tick {
                    track.update_command();
                    track.update_message();

                    let message = track.message;
                    let status = (message & 0xFF) as u8;

                    if status < 0xF0 {
                        if (0x90..=0x9F).contains(&status) {
                            let velocity = ((message >> 16) & 0xFF) as u8;
                            if velocity > 0 {
                                note_count += 1;
                            }

                            // Make sure add event with 0 velocity
                            // to make sure running status is correct.
                            if velocity >= min_velocity {
                                events.push(pack_event(message, false));
                            }
                        } else {
                            events.push(pack_event(message, false));
                        }
                    } else if status == 0xFF {
                        track.process_meta_event(&mut multiplier, &mut bpm_us_per_qn, time_div);
                        events.push(pack_event(bpm_us_per_qn as u32, true));
                    }

                    track.update_tick();
                }
            }

            if track.length == 0 {
                finished_tracks += 1;
                print!(
                    "\r\x1b[KFinished track {}/{} -> {} total events.",
                    finished_tracks,
                    total_tracks,
                    events.len().separate_with_commas()
                );
                io::stdout().flush().unwrap();
            }
        }

        if !any_active {
            break;
        }

        tracks.retain(|t| t.length > 0);

        let delta_tick = tracks
            .iter()
            .filter_map(|t| {
                if t.length > 0 {
                    Some(t.tick - tick)
                } else {
                    None
                }
            })
            .min()
            .unwrap_or(0);

        if events.len() > group_start as usize && delta_tick > 0 {
            let last_idx = events.len() as u32 - 1;
            deltas.push((last_idx, delta_tick.min(u32::MAX as u64) as u32));
        }

        if delta_tick > 0 {
            total_us_acc += (delta_tick as u128) * (bpm_us_per_qn as u128) / (time_div as u128);
        }

        tick = tick.wrapping_add(delta_tick);
        total_ticks = tick;
    }

    let total_nanos = total_us_acc.saturating_mul(1000);
    let total_duration = if total_nanos > (u64::MAX as u128) {
        Duration::from_nanos(u64::MAX)
    } else {
        Duration::from_nanos(total_nanos as u64)
    };

    events.shrink_to_fit();
    deltas.shrink_to_fit();

    ParsedMidi {
        events,
        deltas,
        total_ticks,
        total_duration,
        note_count,
    }
}

pub fn play_parsed_events(
    parsed: &ParsedMidi,
    time_div: u16,
    mut send_direct_data: impl FnMut(u32) + Send + 'static,
    delay_fn: Option<Box<dyn FnMut(i64) + Send + 'static>>,
) {
    if parsed.events.is_empty() {
        return;
    }

    let default_delay = |ns: i64| delay_execution_100ns(ns);
    let mut delay_fn = match delay_fn {
        Some(f) => f,
        None => Box::new(move |ns| default_delay(ns)),
    };

    let mut bpm_us_per_qn: u64;
    let mut tick: u64 = 0;
    let mut multiplier: f64 = 0.0;
    let max_drift: i64 = 100_000;
    let mut old: i64 = 0;
    let mut delta: i64 = 0;
    let mut last_time = get_time_100ns();

    let mut i = 0;
    let n = parsed.events.len();
    let mut delta_idx = 0;
    let n_deltas = parsed.deltas.len();

    while i < n {
        loop {
            let packed = unsafe { *parsed.events.get_unchecked(i) };
            let (data, is_tempo) = unpack_event(packed);

            if is_tempo {
                bpm_us_per_qn = data as u64;
                multiplier = (bpm_us_per_qn as f64) / (time_div as f64) * 10.0;
            } else {
                send_direct_data(data);
            }

            if delta_idx < n_deltas {
                let (idx, delta_ticks) = unsafe { *parsed.deltas.get_unchecked(delta_idx) };
                if idx == i as u32 {
                    let delta_tick = delta_ticks as u64;
                    tick = tick.wrapping_add(delta_tick);

                    let now = get_time_100ns();
                    let elapsed = (now - last_time) as i64;
                    last_time = now;

                    let work_time = elapsed - old;
                    old = (delta_tick as f64 * multiplier) as i64;
                    delta = delta.wrapping_add(work_time);

                    let sleep_time = if delta > 0 { old - delta } else { old };

                    if sleep_time <= 0 {
                        delta = delta.min(max_drift);
                    } else {
                        delay_fn(sleep_time);
                    }

                    delta_idx += 1;
                    i += 1;
                    break;
                }
            }

            i += 1;
            if i >= n {
                break;
            }
        }

        if i >= n {
            break;
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct UnpackedEvent {
    idx: u32,
    data: u32,
    is_tempo: bool,
}

pub fn play_parsed_events_batched(
    parsed: &ParsedMidi,
    time_div: u16,
    mut send_direct_data: impl FnMut(u32) + Send + 'static,
    delay_fn: Option<Box<dyn FnMut(i64) + Send + 'static>>,
) {
    if parsed.events.is_empty() {
        return;
    }

    let default_delay = |ns: i64| delay_execution_100ns(ns);
    let mut delay_fn = match delay_fn {
        Some(f) => f,
        None => Box::new(move |ns| default_delay(ns)),
    };

    let batch_size = 65536;
    let lookahead_batches = 2048;

    let (batch_tx, batch_rx): (Sender<Vec<UnpackedEvent>>, Receiver<Vec<UnpackedEvent>>) =
        bounded(lookahead_batches);

    let (pool_tx, pool_rx): (Sender<Vec<UnpackedEvent>>, Receiver<Vec<UnpackedEvent>>) =
        bounded(lookahead_batches);

    // pre-fill the pool with empty Vecs (reusable buffers)
    for _ in 0..lookahead_batches {
        // allocate with capacity to avoid resizes
        pool_tx
            .send(Vec::with_capacity(batch_size))
            .expect("pool prefill should succeed");
    }

    thread::scope(|scope| {
        let parser_tx = batch_tx.clone();
        let parser_pool_rx = pool_rx.clone();

        scope.spawn(move || {
            let mut iter = parsed.events.iter().enumerate();
            loop {
                let mut buf = match parser_pool_rx.recv() {
                    Ok(v) => v,
                    Err(_) => return,
                };

                buf.clear();
                for (idx, &packed) in (&mut iter).by_ref().take(batch_size) {
                    let (data, is_tempo) = unpack_event(packed);
                    buf.push(UnpackedEvent {
                        idx: idx as u32,
                        data,
                        is_tempo,
                    });
                }

                if buf.is_empty() {
                    // No events left.
                    let _ = parser_tx.send(buf);
                    break;
                }

                if parser_tx.send(buf).is_err() {
                    break;
                }
            }
        });

        let mut bpm_us_per_qn: u64;
        let mut tick: u64 = 0;
        let mut multiplier: f64 = 0.0;
        let max_drift: i64 = 100_000;
        let mut old: i64 = 0;
        let mut delta: i64 = 0;
        let mut last_time = get_time_100ns();

        let mut delta_idx = 0;
        let n_deltas = parsed.deltas.len();

        // Receive entire batches (blocks when empty). Processing is a tight per-batch loop.
        while let Ok(mut batch) = batch_rx.recv() {
            if batch.is_empty() {
                break;
            }

            for ev in &batch {
                if ev.is_tempo {
                    bpm_us_per_qn = ev.data as u64;
                    multiplier = (bpm_us_per_qn as f64) / (time_div as f64) * 10.0;
                } else {
                    send_direct_data(ev.data);
                }

                while delta_idx < n_deltas {
                    let (didx, delta_ticks) = unsafe { *parsed.deltas.get_unchecked(delta_idx) };
                    if didx != ev.idx {
                        break;
                    }

                    let delta_tick = delta_ticks as u64;
                    tick = tick.wrapping_add(delta_tick);

                    let now = get_time_100ns();
                    let elapsed = (now - last_time) as i64;
                    last_time = now;

                    let work_time = elapsed - old;
                    old = (delta_tick as f64 * multiplier) as i64;
                    delta = delta.wrapping_add(work_time);

                    let sleep_time = if delta > 0 { old - delta } else { old };

                    if sleep_time <= 0 {
                        delta = delta.min(max_drift);
                    } else {
                        delay_fn(sleep_time);
                    }

                    delta_idx += 1;
                }
            }
            batch.clear();
            let _ = pool_tx.try_send(batch);
        }

        drop(batch_rx);
    })
}
