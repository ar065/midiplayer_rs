use crate::midi::track_data::TrackData;
use crate::midi::utils::{delay_execution_100ns, get_time_100ns};
use crossbeam_channel::{Receiver, Sender, bounded};
use rayon::prelude::*;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use thousands::Separable;

#[derive(Debug, Copy, Clone)]
pub struct Event {
    pub data: u32,
    pub track: u16,
    pub is_tempo: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub events: Vec<Event>,
    pub deltas: Vec<(u32, u32)>,
    pub total_ticks: u64,
    pub total_duration: Duration,
    pub note_count: u64,
}

#[derive(Debug, Clone)]
struct TrackEvents {
    events: Vec<(u64, Event)>, // (tick, event)
    tempo_changes: Vec<(u64, u64)>, // (tick, us_per_qn)
    note_count: u64,
    max_tick: u64,
}

fn parse_single_track(mut track: TrackData, track_idx: u16, time_div: u16) -> TrackEvents {
    let mut events = Vec::with_capacity(4096);
    let mut tempo_changes = Vec::with_capacity(16);
    let mut note_count = 0u64;
    let mut max_tick = 0u64;
    let mut bpm_us_per_qn = 500_000u64;

    while track.length > 0 {
        let current_tick = track.tick;
        max_tick = max_tick.max(current_tick);

        track.update_command();
        track.update_message();

        let message = track.message;
        let status = (message & 0xFF) as u8;

        if status < 0xF0 {
            // Regular MIDI message
            if (0x90..=0x9F).contains(&status) {
                let velocity = ((message >> 16) & 0xFF) as u8;
                if velocity > 0 {
                    note_count += 1;
                }
            }

            events.push((
                current_tick,
                Event {
                    data: message,
                    track: track_idx,
                    is_tempo: false,
                },
            ));
        } else if status == 0xFF {
            // Meta event - check for tempo
            let mut multiplier = 0.0f64;
            let old_bpm = bpm_us_per_qn;
            track.process_meta_event(&mut multiplier, &mut bpm_us_per_qn, time_div);
            
            if bpm_us_per_qn != old_bpm {
                tempo_changes.push((current_tick, bpm_us_per_qn));
            }
        }

        track.update_tick();
    }

    events.shrink_to_fit();
    tempo_changes.shrink_to_fit();

    TrackEvents {
        events,
        tempo_changes,
        note_count,
        max_tick,
    }
}

pub fn parse_midi_events(tracks: Vec<TrackData>, time_div: u16) -> ParsedMidi {
    let total_tracks = tracks.len();
    
    if tracks.is_empty() {
        return ParsedMidi {
            events: Vec::new(),
            deltas: Vec::new(),
            total_ticks: 0,
            total_duration: Duration::ZERO,
            note_count: 0,
        };
    }

    println!("\r\x1b[KParsing {} tracks in parallel...", total_tracks);
    io::stdout().flush().unwrap();

    let finished_counter = AtomicU64::new(0);

    let track_results: Vec<TrackEvents> = tracks
        .into_par_iter()
        .enumerate()
        .map(|(idx, track)| {
            let result = parse_single_track(track, idx as u16, time_div);
            
            let finished = finished_counter.fetch_add(1, Ordering::Relaxed) + 1;
            print!(
                "\r\x1b[KFinished track {}/{} -> {} events parsed",
                finished,
                total_tracks,
                result.events.len().separate_with_commas()
            );
            io::stdout().flush().unwrap();
            
            result
        })
        .collect();

    println!("\r\x1b[KMerging events from {} tracks...", total_tracks);
    io::stdout().flush().unwrap();

    // Calculate total events needed
    let total_event_count: usize = track_results.iter().map(|t| t.events.len()).sum();
    let total_tempo_changes: usize = track_results.iter().map(|t| t.tempo_changes.len()).sum();
    
    let mut all_events = Vec::with_capacity(total_event_count + total_tempo_changes);
    let note_count: u64 = track_results.iter().map(|t| t.note_count).sum();
    let total_ticks = track_results.iter().map(|t| t.max_tick).max().unwrap_or(0);

    // Merge tempo changes
    for track_result in &track_results {
        for &(tick, us_per_qn) in &track_result.tempo_changes {
            all_events.push((
                tick,
                Event {
                    data: us_per_qn as u32,
                    track: 0, // tempo events don't need track info
                    is_tempo: true,
                },
            ));
        }
    }

    // Merge regular events
    for track_result in track_results {
        all_events.extend(track_result.events);
    }

    // Sort all events by tick (stable sort to preserve track order for same tick)
    all_events.par_sort_by_key(|&(tick, _)| tick);

    println!("\r\x1b[KBuilding delta table...");
    io::stdout().flush().unwrap();

    // Build final events and deltas
    let mut events = Vec::with_capacity(all_events.len());
    let mut deltas = Vec::with_capacity(all_events.len() / 10);
    
    let mut prev_tick = 0u64;
    let mut bpm_us_per_qn = 500_000u64;
    let mut total_us_acc = 0u128;
    
    for (i, (tick, event)) in all_events.iter().enumerate() {
        if *tick > prev_tick {
            let delta_tick = tick - prev_tick;
            
            if i > 0 {
                deltas.push(((i - 1) as u32, delta_tick.min(u32::MAX as u64) as u32));
            }
            
            total_us_acc += (delta_tick as u128) * (bpm_us_per_qn as u128) / (time_div as u128);
            prev_tick = *tick;
        }
        
        if event.is_tempo {
            bpm_us_per_qn = event.data as u64;
        }
        
        events.push(*event);
    }

    let total_nanos = total_us_acc.saturating_mul(1000);
    let total_duration = if total_nanos > (u64::MAX as u128) {
        Duration::from_nanos(u64::MAX)
    } else {
        Duration::from_nanos(total_nanos as u64)
    };

    events.shrink_to_fit();
    deltas.shrink_to_fit();

    println!(
        "\r\x1b[KParsing complete: {} total events, {} notes",
        events.len().separate_with_commas(),
        note_count.separate_with_commas()
    );
    io::stdout().flush().unwrap();

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
    mut send_direct_data: impl FnMut(u32, u16) + Send + 'static,
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
            let data = packed.data;
            let is_tempo = packed.is_tempo;

            if is_tempo {
                bpm_us_per_qn = data as u64;
                multiplier = (bpm_us_per_qn as f64) / (time_div as f64) * 10.0;
            } else {
                send_direct_data(data, packed.track);
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
pub struct UnpackedEvent {
    pub idx: u32,
    pub data: u32,
    pub track: u16,
    pub is_tempo: bool,
}

pub fn play_parsed_events_batched(
    parsed: &ParsedMidi,
    time_div: u16,
    mut send_direct_data: impl FnMut(u32, u16) + Send + 'static,
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

    // Pre-fill the pool with empty Vecs (reusable buffers)
    for _ in 0..lookahead_batches {
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
                    let data = packed.data;
                    let is_tempo = packed.is_tempo;

                    buf.push(UnpackedEvent {
                        idx: idx as u32,
                        track: packed.track,
                        data,
                        is_tempo,
                    });
                }

                if buf.is_empty() {
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
                    send_direct_data(ev.data, ev.track);
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