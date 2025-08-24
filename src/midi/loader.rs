use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use crate::midi::track_data::TrackData;

/// Load a MIDI file.
/// This returns a vector of TrackData and the time division.
pub fn load_midi_file<P: AsRef<Path>>(filename: P) -> io::Result<(Vec<TrackData>, u16)> {
    let file = File::open(&filename)?;
    let mut reader = BufReader::new(file);

    // Read and verify the header;
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;
    if &header != b"MThd" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Not a MIDI file",
        ));
    }

    // Header length (big-endian)
    let mut buf4 = [0u8; 4];
    reader.read_exact(&mut buf4)?;
    let header_len = u32::from_be_bytes(buf4);
    if header_len != 6 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid header length",
        ));
    }

    // Format and track count
    let mut buf2 = [0u8; 2];
    reader.read_exact(&mut buf2)?;
    let format = u16::from_be_bytes(buf2);

    reader.read_exact(&mut buf2)?;
    let num_tracks = u16::from_be_bytes(buf2) as usize;

    // Time division
    reader.read_exact(&mut buf2)?;
    let time_div = u16::from_be_bytes(buf2);
    if (time_div & 0x8000) != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SMPTE timing is not supported",
        ));
    }

    // Allocate the tracks
    let mut tracks = Vec::with_capacity(num_tracks);

    for _ in 0..num_tracks {
        // Read the track header
        reader.read_exact(&mut header)?;
        if &header != b"MTrk" {
            // Skip the unknown chunk or break?
            // We chose to break in this case.
            break;
        }

        // Read the track length
        reader.read_exact(&mut buf4)?;
        let length = u32::from_be_bytes(buf4) as usize;

        // Read the track data
        let mut data = vec![0u8; length];
        reader.read_exact(&mut data)?;

        // Initialize the track
        let mut track = TrackData::new(length);
        track.data = data;
        track.update_tick();

        tracks.push(track);
    }

    Ok((tracks, time_div))
}
