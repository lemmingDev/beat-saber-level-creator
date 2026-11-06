//! Spectral analysis utilities.

use realfft::RealFftPlanner;

#[derive(Debug, Clone)]
pub struct SpectralResult {
    pub spectral_flux: Vec<f32>,
    pub rms_energy: Vec<f32>,
    pub magnitude_spectra: Vec<Vec<f32>>,
}

pub struct SpectralAnalyzer {
    sample_rate: u32,
    window_size: usize,
    hop_size: usize,
    planner: RealFftPlanner<f32>,
    window: Vec<f32>,
}

impl SpectralAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        let window_size = 2048;
        let hop_size = 512;
        let window = Self::hann_window(window_size);
        
        Self {
            sample_rate,
            window_size,
            hop_size,
            planner: RealFftPlanner::new(),
            window,
        }
    }

    pub fn analyze(&mut self, audio: &[f32]) -> SpectralResult {
        let mut spectral_flux = Vec::new();
        let mut rms_energy = Vec::new();
        let mut magnitude_spectra = Vec::new();
        let mut prev_magnitude = vec![0.0; self.window_size / 2 + 1];

        let fft = self.planner.plan_fft_forward(self.window_size);
        let mut buffer = vec![0.0f32; self.window_size];
        let mut complex_spectrum = fft.make_output_vec();

        for frame_start in (0..audio.len()).step_by(self.hop_size) {
            if frame_start + self.window_size > audio.len() {
                break;
            }

            // Apply window
            for i in 0..self.window_size {
                buffer[i] = audio[frame_start + i] * self.window[i];
            }

            // RMS
            let rms: f32 = buffer.iter().map(|x| x * x).sum::<f32>().sqrt() / (self.window_size as f32).sqrt();
            rms_energy.push(rms);

            // FFT
            fft.process(&mut buffer, &mut complex_spectrum).unwrap();

            // Magnitude spectrum
            let mut magnitude = vec![0.0; self.window_size / 2 + 1];
            for (i, c) in complex_spectrum.iter().enumerate() {
                magnitude[i] = (c.re * c.re + c.im * c.im).sqrt();
            }

            // Spectral flux
            let mut flux = 0.0;
            for i in 0..magnitude.len() {
                let diff = magnitude[i] - prev_magnitude[i];
                if diff > 0.0 {
                    flux += diff;
                }
            }
            spectral_flux.push(flux);

            prev_magnitude = magnitude.clone();
            magnitude_spectra.push(magnitude);
        }

        SpectralResult {
            spectral_flux,
            rms_energy,
            magnitude_spectra,
        }
    }

    pub fn analyze_chunk(&mut self, audio: &[f32]) -> SpectralResult {
        // For streaming - just one frame
        let mut buffer = vec![0.0f32; self.window_size];
        let len = audio.len().min(self.window_size);
        
        for i in 0..len {
            buffer[i] = audio[i] * self.window[i];
        }

        let rms: f32 = buffer.iter().map(|x| x * x).sum::<f32>().sqrt() / (len as f32).sqrt();

        let fft = self.planner.plan_fft_forward(self.window_size);
        let mut complex_spectrum = fft.make_output_vec();
        fft.process(&mut buffer, &mut complex_spectrum).unwrap();

        let mut magnitude = vec![0.0; self.window_size / 2 + 1];
        for (i, c) in complex_spectrum.iter().enumerate() {
            magnitude[i] = (c.re * c.re + c.im * c.im).sqrt();
        }

        SpectralResult {
            spectral_flux: vec![0.0],
            rms_energy: vec![rms],
            magnitude_spectra: vec![magnitude],
        }
    }

    fn hann_window(size: usize) -> Vec<f32> {
        use std::f32::consts::PI;
        (0..size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (size - 1) as f32).cos()))
            .collect()
    }
}
