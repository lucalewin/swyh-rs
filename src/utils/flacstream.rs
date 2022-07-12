use crossbeam_channel::{unbounded, Receiver, Sender};
use flac_bound::{FlacEncoder, WriteWrapper};
use std::{
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        Arc,
    },
    time::Duration,
};

// the flacwriter receives the data from the encoder
// and writes them to the flac output channel
#[derive(Clone)]
pub struct FlacWriter {
    flac_out: Sender<Vec<u8>>,
}

impl FlacWriter {
    pub fn new(flac_out: Sender<Vec<u8>>) -> FlacWriter {
        FlacWriter { flac_out }
    }
}

impl Write for FlacWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        //eprintln!("FlacWriter writing {0} bytes", buf.len());
        match self.flac_out.send(buf.to_vec()) {
            Ok(()) => Ok(buf.len()),
            Err(_e) => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "FlacWriter channel SendError",
            )),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// a FlacChannel is set up by the channelstream
// the ChannelStream writes the captured f32 samples
// to the samples_in channel for encoding
#[derive(Clone)]
pub struct FlacChannel {
    samples_in: Receiver<Vec<f32>>,
    pub flac_in: Receiver<Vec<u8>>,
    active: Arc<AtomicBool>,
    writer: FlacWriter,
    sample_rate: u32,
    bits_per_sample: u32,
    channels: u32,
}

impl FlacChannel {
    pub fn new(
        samples_in: Receiver<Vec<f32>>,
        sample_rate: u32,
        bits_per_sample: u32,
        channels: u32,
    ) -> FlacChannel {
        let (flac_out, flac_in): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = unbounded();
        FlacChannel {
            samples_in,
            flac_in,
            active: Arc::new(AtomicBool::new(false)),
            writer: FlacWriter::new(flac_out),
            sample_rate,
            bits_per_sample,
            channels,
        }
    }

    pub fn run(&self) {
        // copy instance data for thread
        let samples_in = self.samples_in.clone();
        let mut writer = self.writer.clone();
        let ch = self.channels;
        let bps = self.bits_per_sample;
        let sr = self.sample_rate;
        let l_active = self.active.clone();
        // fire up thread
        self.active.store(true, Relaxed);
        //eprintln!("Starting FLAC encoder thread");
        let _thr = std::thread::Builder::new()
            .name("flac_encoder".into())
            .stack_size(4 * 1024 * 1024)
            .spawn(move || {
                // we're running
                // setup the encoder
                let mut outw = WriteWrapper(&mut writer);
                let mut enc = FlacEncoder::new()
                    .unwrap()
                    .channels(ch)
                    .bits_per_sample(bps)
                    .sample_rate(sr)
                    .compression_level(0)
                    .init_write(&mut outw)
                    .unwrap();
                // read captured samples and encode
                let t = Duration::from_millis(100);
                let shift = if bps == 24 { 8u8 } else { 16u8 };
                while l_active.load(Relaxed) {
                    if let Ok(f32_samples) = samples_in.recv_timeout(t) {
                        //eprintln!("Flac encoder received {0} samples", f32_samples.len());
                        let samples = f32_samples
                            .iter()
                            .map(|s| to_i32_sample(*s) >> shift)
                            .collect::<Vec<i32>>();
                        enc.process_interleaved(samples.as_slice(), (samples.len() / 2) as u32)
                            .unwrap();
                    }
                }
                let _ = enc.finish();
                //eprintln!("FLAC encoder thread stopped");
            })
            .unwrap();
    }

    pub fn stop(&self) {
        self.active.store(false, Relaxed);
    }
}

fn to_i32_sample(mut f32_sample: f32) -> i32 {
    if f32_sample > 1.0 {
        f32_sample = 1.0;
    } else if f32_sample < -1.0 {
        f32_sample = -1.0;
    }
    if f32_sample >= 0.0 {
        ((f32_sample as f64 * i32::MAX as f64) + 0.5) as i32
    } else {
        ((-f32_sample as f64 * i32::MIN as f64) - 0.5) as i32
    }
}