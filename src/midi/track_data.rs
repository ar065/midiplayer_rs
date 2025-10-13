pub struct TrackData {
    pub data: Vec<u8>,
    pub long_msg: Vec<u8>,
    pub tick: u64,
    pub offset: usize,
    pub length: usize,
    pub message: u32,
    pub temp: u32,
    pub last_status: Option<u8>,
}

impl TrackData {
    /// Create a new TrackData with a given capacity.
    pub fn new(capacity: usize) -> Self {
        TrackData {
            data: Vec::with_capacity(capacity),
            long_msg: Vec::with_capacity(256),
            tick: 0,
            offset: 0,
            length: capacity,
            message: 0,
            temp: 0,
            last_status: None,
        }
    }

    /// Decode a variable-length MIDI value from the data stream.
    pub fn decode_variable_length(&mut self) -> u32 {
        let mut result = 0u32;
        while self.offset < self.length {
            let byte = self.data[self.offset];
            self.offset += 1;
            result = (result << 7) | u32::from(byte & 0x7F);
            if (byte & 0x80) == 0 {
                break;
            }
        }
        result
    }

    /// Advance the tick by the next delta-time value.
    pub fn update_tick(&mut self) {
        self.tick = self
            .tick
            .wrapping_add(u64::from(self.decode_variable_length()));
    }

    /// Read the next status byte if present and update `message`.
    /// Also handles running status
    pub fn update_command(&mut self) {
        if self.offset >= self.length {
            return;
        }

        let byte = self.data[self.offset];
        if byte >= 0x80 {
            // new status byte
            self.offset += 1;
            self.message = u32::from(byte);
            self.last_status = Some(byte);
        } else {
            // running status: reuse last_status if available
            if let Some(status) = self.last_status {
                self.message = u32::from(status);
            } else {
                // invalid MIDI: data byte without any running status
                self.message = 0; // or bail
            }
        }
    }

    /// Read message params or long meta/sysex events.
    pub fn update_message(&mut self) {
        if self.offset >= self.length {
            return;
        }

        self.temp = 0; // make sure to reset so running status is ok

        let msg_type = (self.message & 0xFF) as u8;
        match msg_type {
            0x00..=0xBF | 0xE0..=0xEF => {
                if self.offset + 2 <= self.length {
                    self.temp = u32::from(self.data[self.offset]) << 8;
                    self.temp |= u32::from(self.data[self.offset + 1]) << 16;
                    self.offset += 2;
                }
            }
            0xC0..=0xDF => {
                // 1-byte messages (program change & channel pressure)
                if self.offset < self.length {
                    self.temp = u32::from(self.data[self.offset]) << 8;
                    self.offset += 1;
                }
            }
            0xF0 | 0xFF => {
                // Sysex or Meta events
                if msg_type == 0xFF {
                    // Meta event: first data byte is the meta type
                    if self.offset < self.length {
                        self.temp = u32::from(self.data[self.offset]) << 8;
                        self.offset += 1;
                    }
                } else {
                    self.temp = 0;
                }

                let len = self.decode_variable_length() as usize;
                if self.long_msg.capacity() < len {
                    self.long_msg.reserve(len - self.long_msg.capacity());
                }

                // Copy the data
                let end = (self.offset + len).min(self.length);
                self.long_msg.clear();
                self.long_msg
                    .extend_from_slice(&self.data[self.offset..end]);
                self.offset = end;
            }
            _ => {}
        }

        // Combine the message and temp
        self.message |= self.temp;
    }

    /// Process a meta event, updating the multiplier and bpm if it's a temp change,
    /// or marking the end of the track.
    pub fn process_meta_event(&mut self, multiplier: &mut f64, bpm: &mut u64, time_div: u16) {
        let meta_type = ((self.message >> 8) & 0xFF) as u8;
        match meta_type {
            // Temp change
            0x51 if self.long_msg.len() >= 3 => {
                // microseconds per quarter note
                let t = ((self.long_msg[0] as u64) << 16)
                    | ((self.long_msg[1] as u64) << 8)
                    | (self.long_msg[2] as u64);
                *bpm = t;

                // 1 microsecond = 10 * 100ns, so (t * 10)/time_div = 100ns units per tick
                let mut m = (t as f64 * 10.0) / (time_div as f64);
                if m < 1.0 {
                    m = 1.0;
                }
                *multiplier = m;
            }

            // End of track
            0x2F => {
                self.data.clear();
                self.length = 0;
            }
            _ => {}
        }
    }
}
