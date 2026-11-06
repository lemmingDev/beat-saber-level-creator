//! Onset detection using spectral flux and phase deviation.

use beatmapper_wasm_shared::{Onset, OnsetType};
use realfft::RealFftPlanner;
use crate::spectral::SpectralResult;
use std::f32::consts::PI;

pub struct OnsetDetector {
    sample_rate: u32,
    window_size: usize,
    hop_size: usize,
    planner: RealFftPlanner<f32>,
    prev_magnitude: Vec<f32>,
    prev_phase: Vec<f32>,
    spectral_flux_history: Vec<f32>,
}

impl OnsetDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            window_size: 2048,
            hop_size: 512,
            planner: RealFftPlanner::new(),
            prev_magnitude: vec![],
            prev_phase: vec![],
            spectral_flux_history: vec![],
        }
    }

    pub fn analyze(&mut self, audio: &[f32]) -> SpectralResult {
        let mut spectral_flux = Vec::new();
        let mut rms_energy = Vec::new();
        let mut magnitude_spectra = Vec::new();

        let window = self.hann_window();
        let fft = self.planner.plan_fft_forward(self.window_size);
        let mut buffer = vec![0.0f32; self.window_size];
        let mut complex_spectrum = fft.make_output_vec();

        for frame_start in (0..audio.len()).step_by(self.hop_size) {
            if frame_start + self.window_size > audio.len() {
                break;
            }

            // Apply window and copy to buffer
            for i in 0..self.window_size {
                buffer[i] = audio[frame_start + i] * window[i];
            }

            // Compute RMS
            let rms: f32 = buffer.iter().map(|x| x * x).sum::<f32>().sqrt() / (self.window_size as f32).sqrt();
            rms_energy.push(rms);

            // FFT
            fft.process(&mut buffer, &mut complex_spectrum).unwrap();

            // Compute magnitude and phase
            let mut magnitude = vec![0.0; self.window_size / 2 + 1];
            let mut phase = vec![0.0; self.window_size / 2 + 1];
            
            for (i, c) in complex_spectrum.iter().enumerate() {
                magnitude[i] = (c.re * c.re + c.im * c.im).sqrt();
                phase[i] = c.im.atan2(c.re);
            }

            // Spectral flux (magnitude difference)
            let mut flux = 0.0;
            if !self.prev_magnitude.is_empty() {
                for i in 0..magnitude.len() {
                    let diff = magnitude[i] - self.prev_magnitude[i];
                    if diff > 0.0 {
                        flux += diff;
                    }
                }
            }
            spectral_flux.push(flux);

            // Phase deviation (for better transient detection)
            let mut phase_dev = 0.0;
            if !self.prev_phase.is_empty() {
                for i in 0..phase.len() {
                    let expected = self.prev_phase[i] + 2.0 * PI * (i as f32) * (self.hop_size as f32) / (self.window_size as f32);
                    let mut diff = phase[i] - expected;
                    // Wrap to [-pi, pi]
                    while diff > PI { diff -= 2.0 * PI; }
                    while diff < -PI { diff += 2.0 * PI; }
                    phase_dev += diff.abs();
                }
            }
            
            // Combine flux and phase deviation
            if !self.prev_magnitude.is_empty() {
                spectral_flux.last_mut().map(|f| *f += 0.5 * phase_dev / (phase.len() as f32));
            }

            self.prev_magnitude = magnitude.clone();
            self.prev_phase = phase;
            magnitude_spectra.push(magnitude);
        }

        self.spectral_flux_history = spectral_flux.clone();

        SpectralResult {
            spectral_flux,
            rms_energy,
            magnitude_spectra,
        }
    }

    pub fn analyze_chunk(&mut self, audio: &[f32]) -> SpectralResult {
        self.analyze(audio)
    }

    pub fn detect(&mut self, spectral: &SpectralResult) -> Vec<Onset> {
        let flux = &spectral.spectral_flux;
        let rms = &spectral.rms_energy;
        let mut onsets = Vec::new();

        if flux.len() < 3 {
            return onsets;
        }

        // Adaptive threshold based on local statistics
        let window = 43; // ~1 second at 512 hop, 44.1kHz
        let mut adaptive_threshold = vec![0.0; flux.len()];

        for i in 0..flux.len() {
            let start = i.saturating_sub(window / 2);
            let end = (i + window / 2).min(flux.len());
            let local_flux: Vec<f32> = flux[start..end].to_vec();
            let local_rms: Vec<f32> = rms[start..end].to_vec();
            
            let mean_flux = local_flux.iter().sum::<f32>() / local_flux.len() as f32;
            let std_flux = (local_flux.iter().map(|x| (x - mean_flux).powi(2)).sum::<f32>() / local_flux.len() as f32).sqrt();
            let mean_rms = local_rms.iter().sum::<f32>() / local_rms.len() as f32;
            
            adaptive_threshold[i] = mean_flux + 1.5 * std_flux + 0.1 * mean_rms;
        }

        // Peak picking with adaptive threshold
        for i in 1..flux.len() - 1 {
            if flux[i] > flux[i - 1] && flux[i] > flux[i + 1] && flux[i] > adaptive_threshold[i] {
                let time = i as f32 * self.hop_size as f32 / self.sample_rate as f32;
                let strength = (flux[i] / adaptive_threshold[i]).min(1.0);
                
                // Classify onset type based on spectral characteristics
                let onset_type = if i < spectral.magnitude_spectra.len() {
                    self.classify_onset(&spectral.magnitude_spectra[i], spectral.magnitude_spectra.get(i.saturating_sub(1)))
                } else {
                    OnsetType::Broadband
                };

                onsets.push(Onset { time, strength, onset_type });
            }
        }

        // Merge nearby onsets (within 30ms)
        let mut merged: Vec<Onset> = Vec::new();
        for onset in onsets {
            if let Some(last) = merged.last_mut() {
                if onset.time - last.time < 0.03 {
                    if onset.strength > last.strength {
                        *last = onset;
                    }
                } else {
                    merged.push(onset);
                }
            } else {
                merged.push(onset);
            }
        }

        merged
    }

    pub fn detect_chunk(&mut self, spectral: &SpectralResult) -> Vec<Onset> {
        self.detect(spectral)
    }

    fn classify_onset(&self, current: &[f32], previous: Option<&Vec<f32>>) -> OnsetType {
        let prev = match previous {
            Some(p) => p,
            None => return OnsetType::Broadband,
        };

        // Compute spectral centroid change
        let centroid_current = self.spectral_centroid(current);
        let centroid_prev = self.spectral_centroid(prev);
        let centroid_diff = centroid_current - centroid_prev;

        // Compute high-frequency energy ratio
        let hf_ratio = self.high_frequency_ratio(current);

        if hf_ratio > 0.6 && centroid_diff > 500.0 {
            OnsetType::Percussive
        } else if centroid_diff.abs() < 200.0 && hf_ratio < 0.3 {
            OnsetType::Harmonic
        } else {
            OnsetType::Broadband
        }
    }

    fn spectral_centroid(&self, magnitude: &[f32]) -> f32 {
        let mut weighted_sum = 0.0;
        let mut sum = 0.0;
        for (i, &mag) in magnitude.iter().enumerate() {
            let freq = i as f32 * self.sample_rate as f32 / (magnitude.len() * 2) as f32;
            weighted_sum += freq * mag;
            sum += mag;
        }
        if sum > 0.0 { weighted_sum / sum } else { 0.0 }
    }

    fn high_frequency_ratio(&self, magnitude: &[f32]) -> f32 {
        let mid = magnitude.len() / 2;
        let hf_sum: f32 = magnitude[mid..].iter().sum();
        let total: f32 = magnitude.iter().sum();
        if total > 0.0 { hf_sum / total } else { 0.0 }
    }

    fn hann_window(&self) -> Vec<f32> {
        (0..self.window_size)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (self.window_size - 1) as f32).cos()))
            .collect()
    }
}
