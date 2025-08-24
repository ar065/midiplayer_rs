mod midi;
use crate::midi::{loader::load_midi_file, player::parse_midi_events, player::play_events};

mod kdmapi;
use crate::kdmapi::KDMAPI;

fn main() {
    println!("Hello, world!");

    let kdmapi_ref = KDMAPI.as_ref().unwrap();
    let stream = kdmapi_ref.open_stream().unwrap();

    let (tracks, time_div) =
        load_midi_file("/home/ar06/Documents/midi/Hypernova audio.mid").unwrap();
    println!("Tracks: {}, Time division: {}", tracks.len(), time_div);

    let events = parse_midi_events(tracks);
    println!("Events: {}", events.len());
}
