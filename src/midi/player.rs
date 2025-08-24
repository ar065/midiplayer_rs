use crate::midi::track_data::TrackData;
use crate::midi::utils::{delay_execution_100ns, get_time_100ns};

#[derive(Debug, Clone)]
pub struct MidiEvent {
    pub tick: u32,                 // Absolute tick
    pub message: u32,              // Status + meta type (if any)
    pub long_msg: Option<Vec<u8>>, // only for meta/sysex events
}

/// Parse tracks into a flattened list of events with the absolute tick positions.
/// Tracks are freed as they are consumed.
pub fn parse_midi_events(mut tracks: Vec<TrackData>) -> Vec<MidiEvent> {
    let mut events = Vec::new();
    let mut tick: u64 = 0;

    while !tracks.is_empty() {
        // Find the next tick accross all tracks
        let mut next_tick = None;
        for track in tracks.iter() {
            if track.length > 0 {
                let dt = track.tick.saturating_sub(tick);
                if next_tick.is_none() || dt < next_tick.unwrap() {
                    next_tick = Some(dt)
                }
            }
        }

        let delta_tick = match next_tick {
            Some(v) => v,
            None => break,
        };

        tick += delta_tick;

        tracks.retain_mut(|track| {
            while track.length > 0 && track.tick <= tick {
                track.update_command();
                track.update_message();

                let msg_type = (track.message & 0xFF) as u8;

                if msg_type < 0xF0 {
                    // regular MIDI
                    events.push(MidiEvent {
                        tick: tick as u32,
                        message: track.message,
                        long_msg: None,
                    });
                } else {
                    // meta/sysex event: clone long_msg
                    events.push(MidiEvent {
                        tick: tick as u32,
                        message: track.message,
                        long_msg: Some(track.long_msg.clone()),
                    });
                }

                track.update_tick();
            }
            track.length > 0
        });
    }

    events.sort_by_key(|e| e.tick); // Just in case; maybe not needed?
    events
}

/// Handle meta events (tempo, end-of-track, etc.)
pub fn process_meta_event(ev: &MidiEvent, multiplier: &mut f64, bpm: &mut u64, time_div: u16) {
    let meta_type = ((ev.message >> 8) & 0xFF) as u8;
    match meta_type {
        // Tempo change (FF 51)
        0x51 => {
            if let Some(ref data) = ev.long_msg {
                if data.len() >= 3 {
                    let t = ((data[0] as u64) << 16) | ((data[1] as u64) << 8) | (data[2] as u64);
                    *bpm = t;
                    let mut m = (t as f64 * 10.0) / (time_div as f64);
                    if m < 1.0 {
                        m = 1.0;
                    }
                    *multiplier = m;
                }
            }
        }
        // End of track (FF 2F)
        0x2F => {
            // Nothing special needed here (could break playback loop early if desired)
        }
        _ => {}
    }
}

/// Calculate how long to sleep for the next event, with drift correction.
/// - `delta_tick`: ticks since the last event
/// - `multiplier`: 100ns units per tick
///
/// Returns how long to sleep in 100ns units.
fn calc_sleep_time(
    delta_tick: u32,
    multiplier: f64,
    old: &mut i64,
    delta: &mut i64,
    max_drift: i64,
    last_time: &mut i64,
) -> i64 {
    let expected_100ns = (delta_tick as f64 * multiplier) as i64;

    let now = get_time_100ns();
    let elapsed = (now - *last_time) as i64;
    *last_time = now;

    let work_time = elapsed - *old;
    *old = expected_100ns;

    *delta = delta.wrapping_add(work_time);

    if *delta > 0 {
        let sleep_time = *old - *delta;
        if sleep_time <= 0 {
            *delta = (*delta).min(max_drift);
            0
        } else {
            sleep_time
        }
    } else {
        *old
    }
}

pub fn play_events(
    events: &[MidiEvent],
    time_div: u16,
    send_direct_data: impl Fn(u32) + Send + 'static,
) {
    let mut bpm: u64 = 500_000; // µs per quarter note
    let mut multiplier: f64 = (bpm as f64 * 10.0) / (time_div as f64);

    let max_drift: i64 = 100_000;
    let mut old: i64 = 0;
    let mut delta: i64 = 0;
    let mut last_time = get_time_100ns();

    let mut prev_tick: u32 = 0;

    for ev in events {
        let delta_tick = ev.tick - prev_tick;
        prev_tick = ev.tick;

        let sleep_time = calc_sleep_time(
            delta_tick,
            multiplier,
            &mut old,
            &mut delta,
            max_drift,
            &mut last_time,
        );

        if sleep_time > 0 {
            delay_execution_100ns(sleep_time);
        }

        // handle message
        let msg_type = (ev.message & 0xFF) as u8;
        if msg_type == 0xFF {
            process_meta_event(ev, &mut multiplier, &mut bpm, time_div);
        } else {
            send_direct_data(ev.message);
        }
    }
}
