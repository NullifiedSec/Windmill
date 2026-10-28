//! CAVA raw 8-bit spectrum reader. All FFT/smoothing is done by CAVA.
//! Never record microphone or write raw PCM to disk.
use std::{
    error::Error,
    io::Read,
    io::Write,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};
type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub const BARS: usize = 16;
pub struct Spectrum {
    child: Child,
    _config: tempfile::NamedTempFile,
    rx: Receiver<[f32; BARS]>,
}
impl Spectrum {
    pub fn start() -> Result<Self> {
        let output = Command::new("pactl").arg("get-default-sink").output()?;
        if !output.status.success() {
            return Err("No default output sink".into());
        }
        let sink = String::from_utf8(output.stdout)?.trim().to_string();
        if sink.is_empty()
            || sink.len() > 180
            || !sink
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-:".contains(&c))
        {
            return Err("Invalid output sink".into());
        }
        let mut config = tempfile::NamedTempFile::new()?;
        // PULSE monitor captures the sink (not a mic); CAVA FFT is fast and smooth.
        writeln!(
            config,
            "[general]\nframerate = 40\nbars = {BARS}\n\
            [input]\nmethod = pulse\nsource = {sink}.monitor\n\
            [output]\nmethod = raw\nraw_target = /dev/stdout\n\
            data_format = binary\nbit_format = 8\nchannels = mono\n"
        )?;
        config.flush()?;
        let mut child = Command::new("cava")
            .args(["-p"])
            .arg(config.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("CAVA not installed (sudo pacman -S cava): {e}"))?;
        let out = child.stdout.take().ok_or("No CAVA output")?;
        let (tx, rx) = mpsc::sync_channel(2);
        thread::spawn(move || capture(out, tx));
        Ok(Self {
            child,
            _config: config,
            rx,
        })
    }
    pub fn newest(&self) -> Option<[f32; BARS]> {
        self.rx.try_iter().last()
    }
}
fn capture(mut input: impl Read, tx: SyncSender<[f32; BARS]>) {
    let mut bytes = [0u8; BARS];
    while input.read_exact(&mut bytes).is_ok() {
        let mut values = [0.; BARS];
        for (dst, src) in values.iter_mut().zip(bytes.iter()) {
            *dst = *src as f32 / 255.;
        }
        // Never block CAVA if the UI is drawing slowly; drop stale frames.
        let _ = tx.try_send(values);
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
    fn decodes_raw_cava_height_frames() {
        let data = vec![0u8, 255, 128, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let (tx, rx) = mpsc::sync_channel(2);
        capture(data.as_slice(), tx);
        let v = rx.recv().unwrap();
        assert_eq!(v[0], 0.);
        assert_eq!(v[1], 1.);
        assert!((v[2] - 0.502).abs() < 0.01);
    }
}
