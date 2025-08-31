// Super simple command line midi player

mod midi;
use std::sync::Arc;
use std::time::Instant;

use thousands::Separable;

use clap::{Parser, ValueHint};

use crate::midi::player::play_parsed_events;
use crate::midi::{loader::load_midi_file, player::parse_midi_events};

mod kdmapi;
use crate::kdmapi::KDMAPI;

macro_rules! must {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        }
    };
}

#[derive(Parser, Debug)]
#[command(name = "midi_player", about = "Play a MIDI file", author, version)]
struct Args {
    /// Midi file to play
    #[arg(
        short = 'f',
        long = "file",
        value_name = "midi_file",
        value_hint = ValueHint::FilePath,
        required = true
    )]
    file: String,
}

fn main() {
    let args = Args::parse();
    let file = args.file;

    let (tracks, time_div) = must!(load_midi_file(file));
    let num_tracks = tracks.len();

    let start = Instant::now();
    let parsed = parse_midi_events(tracks, time_div, 0);
    let total_ms = parsed.total_duration.as_millis();
    let minutes = total_ms / 60_000;
    let seconds = (total_ms % 60_000) / 1_000;
    let millis = total_ms % 1_000;

    println!(
        "Parsed MIDI Summary:\n\
     - Tracks: {}\n\
     - Time division: {}\n\
     - Events: {}\n\
     - Note Count: {}\n\
     - Total Ticks: {}\n\
     - Total Duration: {:02}:{:02}.{:03}\n\
     - Parse Time: {:.2?}",
        num_tracks.separate_with_commas(),
        time_div,
        parsed.events.len().separate_with_commas(),
        parsed.note_count.separate_with_commas(),
        parsed.total_ticks.separate_with_commas(),
        minutes,
        seconds,
        millis,
        start.elapsed()
    );

    let kdmapi_ref = KDMAPI.as_ref().unwrap();
    let stream = kdmapi_ref.open_stream().unwrap();
    let stream = Arc::new(stream);

    let play_stream = Arc::clone(&stream);

    play_parsed_events(
        &parsed,
        time_div,
        move |data| {
            play_stream.send_direct_data(data);
        },
        None,
    );
}
