use crossbeam_channel::{Receiver, Sender};
use std::collections::VecDeque;
///
/// rwstream.rs
///
/// ChannelStream: the write method sends the received samples on the CrssBeam channel
/// for the Read trait to read them back
///
/// the Read trait implementation is used by the HTTP response to send the response wav stream
/// to the media Renderer
///
use std::io::Read;
use std::io::Result as IoResult;
use std::time::Duration;

/// Channelstream - used to transport the samples from the wave_reader to the http output wav stream
pub struct ChannelStream {
    pub s: Sender<Vec<i16>>,
    pub r: Receiver<Vec<i16>>,
    fifo: VecDeque<i16>,
    silence: Vec<i16>,
    read_timeout: Duration,
    silence_period: Duration,
    sending_silence: bool,
}

impl ChannelStream {
    pub fn new(tx: Sender<Vec<i16>>, rx: Receiver<Vec<i16>>) -> ChannelStream {
        ChannelStream {
            s: tx,
            r: rx,
            fifo: VecDeque::with_capacity(16384),
            silence: Vec::new(),
            read_timeout: Duration::new(30, 0),
            silence_period: Duration::new(1, 0),
            sending_silence: false,
        }
    }

    // create a 1-second near-silent 440 Hz tone (attenuation 32)
    pub fn create_silence(&mut self, sample_rate: u32) {
        let size = (sample_rate * 2) as usize;
        self.silence = Vec::with_capacity(size);
        self.silence.resize(size, 0i16);
        /*
        // near-silent 440 Hz tone (attenuation 32)
        use std::f64::consts::PI;
        self.silence = Vec::with_capacity((sample_rate * 2) as usize);
        let incr = (2.0 * PI) / (sample_rate as f64 / 440.0);
        let min_value: f64 = 0.0;
        let max_value: f64 = 2.0 * PI;
        let mut i = 0;
        loop {
            let mut input_value = min_value;
            while input_value <= max_value {
                let sample = (32.0 * input_value.sin()) as i16;
                self.silence.push(sample); // left channel
                self.silence.push(sample); // right channel
                input_value += incr;
                i += 1;
                if i == sample_rate {
                    return;
                }
            }
        }
        */
    }

    pub fn write(&self, samples: &[i16]) {
        self.s.send(samples.to_vec()).unwrap();
    }
}

impl Read for ChannelStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let mut time_out = if self.sending_silence {
            self.silence_period
        } else {
            self.read_timeout
        };
        let mut i = 0;
        while i < buf.len() - 2 {
            match self.fifo.pop_front() {
                Some(sample) => {
                    buf[i] = ((sample >> 8) & 0xff) as u8;
                    buf[i + 1] = (sample & 0xff) as u8;
                    i += 2;
                }
                None => match self.r.recv_timeout(time_out) {
                    Ok(chunk) => {
                        self.fifo.extend(chunk);
                        self.sending_silence = false;
                        time_out = self.read_timeout;
                    }
                    Err(_) => {
                        self.fifo.extend(self.silence.clone());
                        self.sending_silence = true;
                        time_out = self.silence_period;
                    }
                },
            }
        }
        Ok(i)
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::rwstream::*;
    use crossbeam_channel::{unbounded, Receiver, Sender};
    #[test]

    fn test_silence() {
        let (tx, rx): (Sender<Vec<i16>>, Receiver<Vec<i16>>) = unbounded();
        let mut cs = ChannelStream::new(tx, rx);
        cs.create_silence(44100);
        assert_eq!(cs.silence.len(), 44100 * 2);
        let mut i = 0;
        for sample in cs.silence {
            if i == 15 {
                eprint!("{}\r\n", sample);
                i = 0;
            } else {
                eprint!("{} ", sample);
                i += 1;
            }
        }
    }
}
