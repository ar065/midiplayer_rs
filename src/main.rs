mod midi;
use std::sync::Arc;
use std::time::Instant;

use crate::midi::player::play_parsed_events;
// use crate::midi::player::play_events;
// use crate::midi::{loader::load_midi_file, player::parse_midi_events, player::play_events};
use crate::midi::{loader::load_midi_file, player::parse_midi_events};

mod kdmapi;
use crate::kdmapi::KDMAPI;

fn main() {
    println!("Hello, world!");

    let kdmapi_ref = KDMAPI.as_ref().unwrap();
    let stream = kdmapi_ref.open_stream().unwrap();
    let stream = Arc::new(stream);

    let (tracks, time_div) = load_midi_file(
        "/run/media/ar06/74EAEFC8EAEF8528/Midis/A-1/Dance Till Your PC runs out of RAM 1.3b.mid",
    )
    .unwrap();
    println!("Tracks: {}, Time division: {}", tracks.len(), time_div);

    let start = Instant::now();
    let parsed = parse_midi_events(tracks, time_div, 0);
    let total_ms = parsed.total_duration.as_millis();
    let minutes = total_ms / 60_000;
    let seconds = (total_ms % 60_000) / 1_000;
    let millis = total_ms % 1_000;

    println!(
        "Parsed MIDI Summary:\n\
     - Events: {:>6}\n\
     - Total Ticks: {:>6}\n\
     - Total Duration: {:02}:{:02}.{:03}\n\
     - Parse Time: {:.2?}",
        parsed.events.len(),
        parsed.total_ticks,
        minutes,
        seconds,
        millis,
        start.elapsed()
    );

    let play_stream = Arc::clone(&stream);
    // play_events(&events, move |data| {
    //     play_stream.send_direct_data(data);
    // });
    play_parsed_events(&parsed, time_div, move |data| {
        play_stream.send_direct_data(data);
    });
}
