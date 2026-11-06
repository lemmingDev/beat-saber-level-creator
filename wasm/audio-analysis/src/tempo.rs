//! Tempo detection using multiple algorithms: autocorrelation, YIN difference function,
//! comb filter, and onset interval histogram with octave correction.

use crate::spectral::SpectralResult;
use beatmapper_wasm_shared::Onset;
use realfft::RealFftPlanner;
use std::f32::consts::PI;

pub struct TempoDetector {
    sample_rate: u32,
    hop_size: usize,
    planner: RealFftPlanner<f32>,
    current_tempo: f32,
}

impl TempoDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            hop_size: 512,
            planner: RealFftPlanner::new(),
            current_tempo: 120.0,
        }
    }

    pub fn detect(
        &mut self,
        onsets: &[Onset],
        spectral: &SpectralResult,
        duration: f32,
    ) -> (f32, f32, Vec<f32>, Vec<f32>) {
        if onsets.len() < 4 {
            return (120.0, 0.0, vec![], vec![]);
        }

        // Build onset strength signal at hop resolution
        let num_frames = spectral.spectral_flux.len();
        let mut onset_signal = vec![0.0f32; num_frames];

        for onset in onsets {
            let frame = (onset.time * self.sample_rate as f32 / self.hop_size as f32) as usize;
            if frame < num_frames {
                onset_signal[frame] = onset_signal[frame].max(onset.strength);
            }
        }

        // Run multiple tempo detection algorithms
        let auto_candidates = self.autocorrelation_candidates(&onset_signal);

        // Apply median filter to smooth tempo candidates
        let filtered_candidates = self.median_filter_candidates(&auto_candidates, 3);

        // Merge original and filtered candidates
        let mut merged = auto_candidates.clone();
        merged.extend(filtered_candidates);
        merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        merged.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0);

        // Just use autocorrelation with octave correction
        let (tempo, confidence) = self.select_best_tempo_auto(&merged, &onset_signal);

        self.current_tempo = tempo;

        // Beat tracking
        let beats = self.track_beats(&onset_signal, tempo);
        let downbeats = self.find_downbeats(&beats, &onset_signal, tempo);

        (tempo, confidence, beats, downbeats)
    }

    // ===== Algorithm 1: Autocorrelation =====
    fn autocorrelation_candidates(&self, signal: &[f32]) -> Vec<(f32, f32)> {
        let min_bpm = 60.0;
        let max_bpm = 200.0;

        let min_period = (60.0 / max_bpm * self.sample_rate as f32 / self.hop_size as f32) as usize;
        let max_period = (60.0 / min_bpm * self.sample_rate as f32 / self.hop_size as f32) as usize;
        let max_period = max_period.min(signal.len() / 2);

        if min_period >= max_period {
            return vec![(120.0, 0.0)];
        }

        let signal_energy: f32 = signal.iter().map(|x| x * x).sum();
        if signal_energy == 0.0 {
            return vec![(120.0, 0.0)];
        }

        let mut candidates = Vec::new();

        for period in min_period..=max_period {
            let mut corr = 0.0;
            let mut energy_a = 0.0f32;
            let mut energy_b = 0.0f32;
            let count = signal.len().saturating_sub(period);

            for i in 0..count {
                corr += signal[i] * signal[i + period];
                energy_a += signal[i] * signal[i];
                energy_b += signal[i + period] * signal[i + period];
            }

            if count > 0 {
                let denom = (energy_a * energy_b).sqrt();
                if denom > 0.0 {
                    corr /= denom;
                }
                let tempo = 60.0 * self.sample_rate as f32 / (self.hop_size * period) as f32;
                if tempo >= min_bpm && tempo <= max_bpm {
                    candidates.push((tempo, corr));
                }
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        candidates.truncate(8);
        candidates
    }

    // ===== Algorithm 2: YIN Difference Function =====
    // More robust than autocorrelation for tempo detection
    // d(τ) = Σ (x(t) - x(t+τ))²
    // Normalized: d'(τ) = d(τ) / (1/τ * Σ d(j)) for j=1..τ
    fn yin_candidates(&self, signal: &[f32]) -> Vec<(f32, f32)> {
        let min_bpm = 60.0;
        let max_bpm = 200.0;

        let min_period = (60.0 / max_bpm * self.sample_rate as f32 / self.hop_size as f32) as usize;
        let max_period = (60.0 / min_bpm * self.sample_rate as f32 / self.hop_size as f32) as usize;
        let max_period = max_period.min(signal.len() / 2);

        if min_period >= max_period {
            return vec![(120.0, 0.0)];
        }

        // Compute difference function d(τ)
        let mut diff = vec![0.0f32; max_period + 1];
        for tau in 1..=max_period {
            let mut sum = 0.0;
            for i in 0..signal.len().saturating_sub(tau) {
                let delta = signal[i] - signal[i + tau];
                sum += delta * delta;
            }
            diff[tau] = sum;
        }

        // Cumulative mean normalized difference (CMND)
        // d'(τ) = d(τ) / (1/τ * Σ d(j)) for j=1..τ
        let mut cmnd = vec![1.0f32; max_period + 1];
        let mut running_sum = 0.0;
        for tau in 1..=max_period {
            running_sum += diff[tau];
            if running_sum > 0.0 {
                cmnd[tau] = diff[tau] * tau as f32 / running_sum;
            }
        }

        // Find local minima below threshold (0.15)
        let threshold = 0.15;
        let mut candidates = Vec::new();

        for tau in min_period..=max_period {
            if tau == 0 || tau >= max_period {
                continue;
            }
            // Local minimum
            if cmnd[tau] < cmnd[tau - 1] && cmnd[tau] <= cmnd[tau + 1] {
                if cmnd[tau] < threshold {
                    let tempo = 60.0 * self.sample_rate as f32 / (self.hop_size * tau) as f32;
                    if tempo >= min_bpm && tempo <= max_bpm {
                        // Confidence = 1 - cmnd (lower cmnd = higher confidence)
                        let conf = 1.0 - cmnd[tau];
                        candidates.push((tempo, conf));
                    }
                }
            }
        }

        // If no candidates below threshold, take global minimum
        if candidates.is_empty() {
            let mut best_tau = min_period;
            let mut best_cmnd = f32::MAX;
            for tau in min_period..=max_period {
                if cmnd[tau] < best_cmnd {
                    best_cmnd = cmnd[tau];
                    best_tau = tau;
                }
            }
            let tempo = 60.0 * self.sample_rate as f32 / (self.hop_size * best_tau) as f32;
            if tempo >= min_bpm && tempo <= max_bpm {
                candidates.push((tempo, 1.0 - best_cmnd));
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        candidates.truncate(8);
        candidates
    }

    // ===== Algorithm 3: Comb Filter =====
    // Tests multiple tempo hypotheses by convolving onset signal with comb filters
    fn comb_filter_candidates(&self, signal: &[f32]) -> Vec<(f32, f32)> {
        let min_bpm = 60.0;
        let max_bpm = 200.0;

        // Test tempos in 0.5 BPM increments
        let mut candidates = Vec::new();
        let mut tempo = min_bpm;
        while tempo <= max_bpm {
            let period = (60.0 / tempo * self.sample_rate as f32 / self.hop_size as f32) as usize;
            if period > 0 && period < signal.len() {
                // Comb filter: sum of signal at beat positions
                // Use 3 taps (current, +period, +2*period) for robustness
                let mut score = 0.0;
                let mut count = 0;
                for i in (0..signal.len()).step_by(period.max(1)) {
                    score += signal[i];
                    if i + period < signal.len() {
                        score += signal[i + period] * 0.5;
                    }
                    count += 1;
                }
                if count > 0 {
                    score /= count as f32;
                    candidates.push((tempo, score));
                }
            }
            tempo += 0.5;
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        candidates.truncate(8);
        candidates
    }

    // ===== Algorithm 4: Onset Interval Histogram =====
    fn interval_histogram_candidates(&self, onsets: &[Onset]) -> Vec<(f32, f32)> {
        let min_interval = 0.3; // 200 BPM
        let max_interval = 1.0; // 60 BPM
        let bin_size = 0.002; // 2ms bins for precision
        let num_bins = ((max_interval - min_interval) / bin_size) as usize;

        let mut histogram = vec![0.0f32; num_bins];

        for i in 0..onsets.len() {
            for j in i + 1..onsets.len().min(i + 12) {
                let interval = onsets[j].time - onsets[i].time;
                if interval >= min_interval && interval <= max_interval {
                    let bin = ((interval - min_interval) / bin_size) as usize;
                    if bin < num_bins {
                        let weight = onsets[i].strength * onsets[j].strength;
                        histogram[bin] += weight;
                    }
                }
            }
        }

        // Gaussian smoothing (sigma = 3 bins)
        let sigma = 3.0;
        let kernel_size = 7;
        let mut smoothed = vec![0.0f32; num_bins];
        for i in 0..num_bins {
            let mut sum = 0.0;
            let mut weight_sum = 0.0;
            for k in 0..kernel_size {
                let offset = k as i32 - (kernel_size as i32 / 2);
                let idx = i as i32 + offset;
                if idx >= 0 && idx < num_bins as i32 {
                    let w = (-((offset as f32).powi(2)) / (2.0 * sigma * sigma)).exp();
                    sum += histogram[idx as usize] * w;
                    weight_sum += w;
                }
            }
            if weight_sum > 0.0 {
                smoothed[i] = sum / weight_sum;
            }
        }

        // Find peaks
        let mut candidates = Vec::new();
        for bin in 1..num_bins - 1 {
            if smoothed[bin] > smoothed[bin - 1] && smoothed[bin] >= smoothed[bin + 1] {
                let interval = min_interval + bin as f32 * bin_size;
                let tempo = 60.0 / interval;
                if tempo >= 60.0 && tempo <= 200.0 {
                    let max_hist = smoothed.iter().cloned().fold(0.0f32, f32::max);
                    let conf = if max_hist > 0.0 { smoothed[bin] / max_hist } else { 0.0 };
                    candidates.push((tempo, conf));
                }
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        candidates.truncate(8);
        candidates
    }

    // ===== Combine all algorithms with voting =====
    /// Median filter for tempo candidates - smooths out noise
    fn median_filter_candidates(&self, candidates: &[(f32, f32)], window: usize) -> Vec<(f32, f32)> {
        if candidates.is_empty() {
            return vec![];
        }

        // Sort by tempo
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let mut filtered = Vec::new();
        for i in 0..sorted.len() {
            let start = i.saturating_sub(window / 2);
            let end = (i + window / 2 + 1).min(sorted.len());
            let window_tempos: Vec<f32> = sorted[start..end].iter().map(|c| c.0).collect();
            let window_confs: Vec<f32> = sorted[start..end].iter().map(|c| c.1).collect();

            // Median tempo
            let mut sorted_tempos = window_tempos.clone();
            sorted_tempos.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let median_tempo = if sorted_tempos.len() % 2 == 0 {
                (sorted_tempos[sorted_tempos.len() / 2 - 1] + sorted_tempos[sorted_tempos.len() / 2]) / 2.0
            } else {
                sorted_tempos[sorted_tempos.len() / 2]
            };

            // Average confidence
            let avg_conf = window_confs.iter().sum::<f32>() / window_confs.len() as f32;

            filtered.push((median_tempo, avg_conf));
        }

        filtered
    }

    fn select_best_tempo_auto(&self, auto: &[(f32, f32)], onset_signal: &[f32]) -> (f32, f32) {
        if auto.is_empty() {
            return (120.0, 0.0);
        }

        // Pick the best autocorrelation candidate
        let best_tempo = auto[0].0;
        let best_conf = auto[0].1;

        // Octave correction: if best tempo is in 90-110 range, check 2x
        let mut final_tempo = best_tempo;
        let mut final_conf = best_conf;

        if best_tempo >= 90.0 && best_tempo <= 110.0 {
            let doubled = best_tempo * 2.0;
            if doubled <= 210.0 {
                let doubled_period = (60.0 / doubled * self.sample_rate as f32 / self.hop_size as f32) as usize;
                let mut corr_doubled = 0.0;
                let mut energy_a = 0.0f32;
                let mut energy_b = 0.0f32;
                let mut count = 0;

                for i in 0..onset_signal.len().saturating_sub(doubled_period) {
                    corr_doubled += onset_signal[i] * onset_signal[i + doubled_period];
                    energy_a += onset_signal[i] * onset_signal[i];
                    energy_b += onset_signal[i + doubled_period] * onset_signal[i + doubled_period];
                    count += 1;
                }

                if count > 0 && energy_a > 0.0 && energy_b > 0.0 {
                    corr_doubled /= (energy_a * energy_b).sqrt();
                    if corr_doubled > 0.15 {
                        final_tempo = doubled;
                        final_conf = corr_doubled * 0.9;
                    }
                }
            }
        }

        // If best tempo < 90, double it
        if final_tempo < 90.0 {
            final_tempo *= 2.0;
            final_conf *= 0.8;
        }

        // Snapping: if within 0.3 of an integer, snap
        let nearest_int = final_tempo.round();
        if (final_tempo - nearest_int).abs() < 0.3 {
            final_tempo = nearest_int;
        }

        (final_tempo, final_conf)
    }

    fn track_beats(&self, signal: &[f32], tempo: f32) -> Vec<f32> {
        let period_frames = (60.0 / tempo * self.sample_rate as f32 / self.hop_size as f32) as usize;
        let mut beats = Vec::new();

        if period_frames == 0 || signal.is_empty() {
            return beats;
        }

        // Find strongest onset as anchor
        let (anchor_idx, _) = signal.iter().enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap_or((0, &0.0));

        // Forward tracking
        let mut next_beat = anchor_idx;
        while next_beat < signal.len() {
            let search_start = next_beat.saturating_sub(period_frames / 4);
            let search_end = (next_beat + period_frames / 4).min(signal.len());

            if let Some((idx, _)) = signal[search_start..search_end].iter().enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap()) {
                let abs_idx = search_start + idx;
                let time = abs_idx as f32 * self.hop_size as f32 / self.sample_rate as f32;
                beats.push(time);
                next_beat = abs_idx + period_frames;
            } else {
                next_beat += period_frames;
            }
        }

        // Backward tracking
        let mut prev_beat = anchor_idx;
        let mut prev_beats = Vec::new();
        while prev_beat > period_frames {
            let search_start = prev_beat.saturating_sub(period_frames / 4);
            let search_end = (prev_beat + period_frames / 4).min(signal.len());

            if let Some((idx, _)) = signal[search_start..search_end].iter().enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap()) {
                let abs_idx = search_start + idx;
                let time = abs_idx as f32 * self.hop_size as f32 / self.sample_rate as f32;
                prev_beats.push(time);
                if abs_idx < period_frames { break; }
                prev_beat = abs_idx.saturating_sub(period_frames);
            } else {
                if prev_beat < period_frames { break; }
                prev_beat -= period_frames;
            }
        }

        prev_beats.reverse();
        beats.splice(0..0, prev_beats);

        beats
    }

    fn find_downbeats(&self, beats: &[f32], _signal: &[f32], _tempo: f32) -> Vec<f32> {
        if beats.len() < 4 {
            return Vec::new();
        }

        let mut downbeats = Vec::new();
        for (i, &beat) in beats.iter().enumerate() {
            if i % 4 == 0 {
                downbeats.push(beat);
            }
        }
        downbeats
    }

    pub fn current_tempo(&self) -> f32 {
        self.current_tempo
    }
}
