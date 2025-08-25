mod midi;
use std::sync::Arc;
use std::time::Instant;

use thousands::Separable;

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

    let (tracks, time_div) =
        load_midi_file("/run/media/ar06/74EAEFC8EAEF8528/Midis/midis2/tau2.5.9.mid").unwrap();
    println!("Tracks: {}, Time division: {}", tracks.len(), time_div);

    let start = Instant::now();
    let parsed = parse_midi_events(tracks, time_div, 0);
    let total_ms = parsed.total_duration.as_millis();
    let minutes = total_ms / 60_000;
    let seconds = (total_ms % 60_000) / 1_000;
    let millis = total_ms % 1_000;

    println!(
        "Parsed MIDI Summary:\n\
     - Events: {}\n\
     - Note Count: {}\n\
     - Total Ticks: {}\n\
     - Total Duration: {:02}:{:02}.{:03}\n\
     - Parse Time: {:.2?}",
        parsed.events.len().separate_with_commas(),
        parsed.note_count.separate_with_commas(),
        parsed.total_ticks.separate_with_commas(),
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
