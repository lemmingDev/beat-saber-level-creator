//! Key detection using Krumhansl-Schmuckler algorithm.

pub struct KeyDetector {
    sample_rate: u32,
    // Krumhansl-Schmuckler key profiles
    major_profile: [f32; 12],
    minor_profile: [f32; 12],
}

impl KeyDetector {
    pub fn new(_sample_rate: u32) -> Self {
        // Krumhansl-Kessler profiles (normalized)
        Self {
            sample_rate: _sample_rate,
            major_profile: [
                6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88
            ],
            minor_profile: [
                6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17
            ],
        }
    }

    pub fn detect(&self, audio: &[f32]) -> (i8, f32, bool) {
        if audio.len() < self.sample_rate as usize {
            return (-1, 0.0, true);
        }

        // Compute chromagram (simplified - using DFT at chroma frequencies)
        let chroma = self.compute_chromagram(audio);
        
        // Correlate with key profiles
        let mut best_corr = -1.0;
        let mut best_key = 0;
        let mut best_major = true;

        for key in 0..12 {
            // Rotate profiles
            let mut major_rotated = [0.0; 12];
            let mut minor_rotated = [0.0; 12];
            for i in 0..12 {
                major_rotated[i] = self.major_profile[(i + key) % 12];
                minor_rotated[i] = self.minor_profile[(i + key) % 12];
            }

            let major_corr = self.correlation(&chroma, &major_rotated);
            let minor_corr = self.correlation(&chroma, &minor_rotated);

            if major_corr > best_corr {
                best_corr = major_corr;
                best_key = key;
                best_major = true;
            }
            if minor_corr > best_corr {
                best_corr = minor_corr;
                best_key = key;
                best_major = false;
            }
        }

        // Normalize confidence
        let confidence = (best_corr + 1.0) / 2.0;
        
        (best_key as i8, confidence.clamp(0.0, 1.0), best_major)
    }

    fn compute_chromagram(&self, audio: &[f32]) -> [f32; 12] {
        let mut chroma = [0.0f32; 12];
        let window_size = 8192;
        let hop_size = 4096;
        
        // A4 = 440 Hz, C0 = 16.35 Hz
        const C0: f32 = 16.3516;
        
        for frame_start in (0..audio.len()).step_by(hop_size) {
            if frame_start + window_size > audio.len() {
                break;
            }
            
            // Compute DFT at 12 chroma frequencies per octave, across multiple octaves
            for octave in 0..8 {
                for pitch_class in 0..12 {
                    let midi_note = 12 + octave * 12 + pitch_class; // C1 = 24
                    let freq = C0 * 2.0f32.powf(midi_note as f32 / 12.0);
                    
                    // Goertzel algorithm for single frequency
                    let k = (freq * window_size as f32 / self.sample_rate as f32).round() as usize;
                    if k < window_size / 2 {
                        let mut real = 0.0;
                        let mut imag = 0.0;
                        let omega = 2.0 * std::f32::consts::PI * k as f32 / window_size as f32;
                        let cos_omega = omega.cos();
                        
                        let mut d1 = 0.0;
                        let mut d2 = 0.0;
                        
                        for i in 0..window_size {
                            let x = audio[frame_start + i];
                            let y = x + 2.0 * cos_omega * d1 - d2;
                            d2 = d1;
                            d1 = y;
                        }
                        
                        real = 0.5 * (d1 - d2 * cos_omega);
                        imag = 0.5 * d2 * omega.sin();
                        let mag = (real * real + imag * imag).sqrt();
                        
                        chroma[pitch_class] += mag;
                    }
                }
            }
        }
        
        // Normalize
        let sum: f32 = chroma.iter().sum();
        if sum > 0.0 {
            for c in &mut chroma {
                *c /= sum;
            }
        }
        
        chroma
    }

    fn correlation(&self, a: &[f32; 12], b: &[f32; 12]) -> f32 {
        let mean_a = a.iter().sum::<f32>() / 12.0;
        let mean_b = b.iter().sum::<f32>() / 12.0;
        
        let mut num = 0.0;
        let mut den_a = 0.0;
        let mut den_b = 0.0;
        
        for i in 0..12 {
            let da = a[i] - mean_a;
            let db = b[i] - mean_b;
            num += da * db;
            den_a += da * da;
            den_b += db * db;
        }
        
        let den = (den_a * den_b).sqrt();
        if den > 0.0 { num / den } else { 0.0 }
    }
}
