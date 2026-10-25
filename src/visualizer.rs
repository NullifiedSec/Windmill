//! Optional, genuinely audio-reactive PipeWire/PulseAudio monitor visualizer.
//! Captures *only* the default output sink's monitor, never a microphone.
//! Samples are processed in RAM and are never written to disk/network.
use std::{
    error::Error,
    io::Read,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const BANDS: [f64; 16] = [
    55., 80., 110., 160., 220., 320., 450., 650., 900., 1250., 1700., 2400., 3300., 4600., 6500.,
    9000.,
];
const SAMPLE_RATE: f64 = 22050.;
const SIZE: usize = 2048;
fn analyze(pcm: &[i16]) -> [f32; 16] {
    let mut values = [0.; 16];
    if pcm.len() != SIZE {
        return values;
    }
    // Goertzel frequency bins; inexpensive and works for real audio, not a fake animation.
    for (i, hertz) in BANDS.iter().enumerate() {
        let k = (0.5 + (SIZE as f64 * hertz / SAMPLE_RATE)).floor();
        let omega = 2. * std::f64::consts::PI * k / SIZE as f64;
        let coeff = 2. * omega.cos();
        let mut a = 0.;
        let mut b = 0.;
        for (j, sample) in pcm.iter().enumerate() {
            let win = 0.5 - 0.5 * (2. * std::f64::consts::PI * j as f64 / (SIZE - 1) as f64).cos();
            let next = (*sample as f64 / 32768.) * win + coeff * a - b;
            b = a;
            a = next;
        }
        let power = (a * a + b * b - coeff * a * b).max(0.).sqrt() / (SIZE as f64) * 2.;
        values[i] = (power.sqrt() * 3.0).clamp(0., 1.) as f32;
    }
    values
}
pub struct Spectrum {
    child: Child,
    rx: Receiver<[f32; 16]>,
}
impl Spectrum {
    pub fn start() -> Result<Self> {
        let output = Command::new("pactl").arg("get-default-sink").output()?;
        if !output.status.success() {
            return Err("Cannot determine output sink".into());
        }
        let sink = String::from_utf8(output.stdout)?.trim().to_owned();
        if sink.is_empty()
            || sink.len() > 180
            || !sink
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || b"._-:".contains(&ch))
        {
            return Err("Invalid output sink identifier".into());
        }
        let mut child = Command::new("parec")
            .args([
                "--device",
                &format!("{sink}.monitor"),
                "--raw",
                "--format=s16le",
                "--channels=1",
                "--rate=22050",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let out = child.stdout.take().ok_or("parec output unavailable")?;
        let (tx, rx) = mpsc::sync_channel(2);
        thread::spawn(move || read_pcm(out, tx));
        Ok(Self { child, rx })
    }
    pub fn newest(&self) -> Option<[f32; 16]> {
        self.rx.try_iter().last()
    }
}
fn read_pcm(mut input: impl Read, tx: SyncSender<[f32; 16]>) {
    let mut buffer = [0u8; SIZE * 2];
    while input.read_exact(&mut buffer).is_ok() {
        let mut pcm = [0i16; SIZE];
        for (index, chunk) in buffer.as_chunks::<2>().0.iter().enumerate() {
            pcm[index] = i16::from_le_bytes([chunk[0], chunk[1]]);
        }
        if tx.try_send(analyze(&pcm)).is_err() {
            // Keep real time: if rendering is slower, drop stale frames.
        }
    }
}
impl Drop for Spectrum {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn genuinely_responds_to_sample_energy() {
        let quiet = [0i16; SIZE];
        let mut tone = [0i16; SIZE];
        for (j, s) in tone.iter_mut().enumerate() {
            *s = (12_000. * (2. * std::f64::consts::PI * 450. * j as f64 / SAMPLE_RATE).sin())
                as i16;
        }
        assert!(analyze(&quiet).iter().all(|a| *a == 0.));
        let bands = analyze(&tone);
        assert!(
            bands[6] > bands[0] * 2.,
            "440Hz spectrum is not frequency reactive"
        );
        assert!(bands[6] > 0.1);
    }
}
