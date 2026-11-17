//! Procesado de señal sin dependencias de audio: ecualizador y espectro.
use std::f32::consts::PI;

pub const EQ_BANDS: usize = 10;
pub const EQ_FREQS: [f32; EQ_BANDS] =
    [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
pub const EQ_LABELS: [&str; EQ_BANDS] =
    ["32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
pub const EQ_RANGE_DB: f32 = 12.0;
pub const SPECTRUM_BANDS: usize = 48;

pub const EQ_PRESETS: [(&str, [f32; EQ_BANDS]); 7] = [
    ("Plano", [0.0; EQ_BANDS]),
    ("Rock", [4.0, 3.0, 2.0, -1.0, -2.0, -1.0, 2.0, 3.0, 4.0, 4.0]),
    ("Pop", [-1.0, 1.0, 3.0, 4.0, 3.0, 0.0, -1.0, -1.0, 1.0, 2.0]),
    ("Jazz", [3.0, 2.0, 1.0, 2.0, -2.0, -2.0, 0.0, 1.0, 2.0, 3.0]),
    ("Clásica", [4.0, 3.0, 2.0, 1.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0]),
    ("Bajos", [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Voces", [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0]),
];

pub fn db_to_linear(db: f32) -> f32 { 10f32.powf(db / 20.0) }

/// Filtro biquad en forma transpuesta II.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32, z1: f32, z2: f32,
}

impl Biquad {
    pub const IDENTITY: Biquad =
        Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 };

    /// Filtro «peaking» de la receta RBJ. Conserva el estado interno si ya existía.
    pub fn peaking(rate: f32, freq: f32, gain_db: f32, q: f32) -> Biquad {
        if gain_db.abs() < 0.05 || freq >= rate * 0.45 || rate <= 0.0 {
            return Biquad::IDENTITY;
        }
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha / a;
        Biquad {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn with_state_of(mut self, old: &Biquad) -> Biquad {
        self.z1 = old.z1;
        self.z2 = old.z2;
        self
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Energía por banda (0..1) en escala logarítmica mediante Goertzel con ventana Hann.
pub fn spectrum(samples: &[f32], rate: u32, bands: usize) -> Vec<f32> {
    if bands == 0 { return Vec::new(); }
    if samples.len() < 256 || rate == 0 { return vec![0.0; bands]; }
    let n = samples.len().min(1024);
    let slice = &samples[samples.len() - n..];
    let windowed: Vec<f32> = slice.iter().enumerate()
        .map(|(i, s)| s * (0.5 - 0.5 * (2.0 * PI * i as f32 / (n as f32 - 1.0)).cos()))
        .collect();
    let nyquist = rate as f32 / 2.0;
    let low = 40.0f32;
    let high = (nyquist * 0.9).min(16000.0).max(low * 2.0);
    let mut out = Vec::with_capacity(bands);
    for b in 0..bands {
        let t = (b as f32 + 0.5) / bands as f32;
        let freq = low * (high / low).powf(t);
        let k = freq * n as f32 / rate as f32;
        let w = 2.0 * PI * k / n as f32;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for x in &windowed {
            let s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0);
        let amplitude = power.sqrt() / (n as f32 * 0.25);
        let db = 20.0 * (amplitude + 1e-9).log10();
        // Más energía a graves: compensa la inclinación espectral típica de la música.
        let tilt = 12.0 * t;
        out.push(((db + 62.0 + tilt) / 60.0).clamp(0.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_band_is_identity() {
        let mut f = Biquad::peaking(48000.0, 1000.0, 0.0, 1.4);
        assert!((f.process(0.5) - 0.5).abs() < 1e-6);
    }
    #[test]
    fn boost_raises_matching_tone() {
        let rate = 48000.0;
        let mut f = Biquad::peaking(rate, 1000.0, 12.0, 1.4);
        let (mut peak_in, mut peak_out) = (0.0f32, 0.0f32);
        for i in 0..48000 {
            let x = (2.0 * PI * 1000.0 * i as f32 / rate).sin() * 0.1;
            let y = f.process(x);
            if i > 24000 { peak_in = peak_in.max(x.abs()); peak_out = peak_out.max(y.abs()); }
        }
        assert!(peak_out > peak_in * 3.0, "{peak_out} vs {peak_in}");
    }
    #[test]
    fn spectrum_finds_tone() {
        let rate = 44100u32;
        let samples: Vec<f32> = (0..2048)
            .map(|i| (2.0 * PI * 1000.0 * i as f32 / rate as f32).sin() * 0.5).collect();
        let bands = spectrum(&samples, rate, 32);
        let (peak, _) = bands.iter().enumerate()
            .fold((0, 0.0f32), |a, (i, v)| if *v > a.1 { (i, *v) } else { a });
        assert!((10..24).contains(&peak), "pico en {peak}");
        let silent = spectrum(&vec![0.0; 2048], rate, 32);
        assert!(silent.iter().all(|v| *v < 0.3));
    }
    #[test]
    fn db_conversion() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-3);
    }
}
