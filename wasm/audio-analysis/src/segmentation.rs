//! Song segmentation using Laplacian segmentation (based on librosa/McFee & Ellis 2014)

use crate::spectral::SpectralResult;
use beatmapper_wasm_shared::{SongSegment, Onset};

pub struct SegmentDetector {
    sample_rate: u32,
    rng_state: u64,
}

impl SegmentDetector {
    pub fn new(sample_rate: u32) -> Self {
        Self { 
            sample_rate,
            rng_state: 0x123456789abcdef0, // Fixed seed for reproducibility
        }
    }

    pub fn detect(
        &mut self,
        spectral: &SpectralResult,
        _onsets: &[Onset],
        beats: &[f32],
        tempo: f32,
    ) -> Vec<beatmapper_wasm_shared::SongSegment> {
        if spectral.magnitude_spectra.len() < 10 {
            return Vec::new();
        }

        // Build beat-synchronous feature matrix
        let n_frames = spectral.magnitude_spectra.len();
        let n_bins = spectral.magnitude_spectra[0].len();
        
        // Sync to beats
        let beat_frames: Vec<usize> = beats.iter()
            .map(|b| (b * self.sample_rate as f32 / 512.0) as usize)
            .filter(|&f| f < n_frames)
            .collect();
        
        if beat_frames.len() < 4 {
            return Vec::new();
        }

        let n_beats = beat_frames.len();
        let n_bins = spectral.magnitude_spectra[0].len();
        
        // Build beat-synchronous feature matrix
        let mut sync_features = vec![vec![0.0f32; n_beats]; n_bins];
        
        for (i, &frame) in beat_frames.iter().enumerate() {
            for bin in 0..n_bins {
                sync_features[bin][i] = spectral.magnitude_spectra[frame][bin];
            }
        }

        // Compute recurrence matrix (cosine similarity between beats)
        let mut recurrence = vec![vec![0.0f32; n_beats]; n_beats];
        for i in 0..n_beats {
            for j in 0..n_beats {
                let mut dot = 0.0;
                let mut norm_i = 0.0;
                let mut norm_j = 0.0;
                for b in 0..n_bins {
                    let vi = sync_features[b][i];
                    let vj = sync_features[b][j];
                    dot += vi * vj;
                    norm_i += vi * vi;
                    norm_j += vj * vj;
                }
                if norm_i > 0.0 && norm_j > 0.0 {
                    recurrence[i][j] = dot / (norm_i.sqrt() * norm_j.sqrt());
                }
            }
        }

        // Enhance diagonals with median filter
        let mut filtered = recurrence.clone();
        let filter_size = 7;
        for i in 0..n_beats {
            for j in 0..n_beats {
                if i.abs_diff(j) <= filter_size / 2 {
                    let mut sum = 0.0;
                    let mut count = 0;
                    for di in -(filter_size as isize / 2)..=(filter_size as isize / 2) {
                        let ni = (i as isize + di) as usize;
                        if ni < n_beats {
                            sum += recurrence[ni][j];
                            count += 1;
                        }
                    }
                    filtered[i][j] = sum / count as f32;
                }
            }
        }

        // Path similarity
        let mut path_sim = vec![0.0f32; n_beats - 1];
        for i in 0..n_beats - 1 {
            let diff = (spectral.spectral_flux[i + 1] - spectral.spectral_flux[i]).abs();
            path_sim[i] = (-diff).exp();
        }
        let sigma = path_sim.iter().sum::<f32>() / path_sim.len() as f32;
        path_sim.iter_mut().for_each(|x| *x = (-*x / sigma).exp());

        // Combine affinity matrices
        let mu = 0.5;
        let mut affinity = filtered;
        for i in 0..n_beats - 1 {
            affinity[i][i + 1] += mu * path_sim[i];
            affinity[i + 1][i] += mu * path_sim[i];
        }

        // Normalized Laplacian
        let mut deg = vec![0.0f32; n_beats];
        for i in 0..n_beats {
            for j in 0..n_beats {
                deg[i] += affinity[i][j];
            }
        }
        let deg_sqrt_inv: Vec<f32> = deg.iter().map(|d| if *d > 0.0 { 1.0 / d.sqrt() } else { 0.0 }).collect();

        let mut laplacian = vec![vec![0.0f32; n_beats]; n_beats];
        for i in 0..n_beats {
            laplacian[i][i] = 1.0;
            for j in 0..n_beats {
                if i != j && affinity[i][j] > 0.0 {
                    laplacian[i][j] = -affinity[i][j] * deg_sqrt_inv[i] * deg_sqrt_inv[j];
                }
            }
        }

        // Eigenvectors via power iteration
        let k = 5.min(n_beats - 1);
        let eigenvectors = self.eigen_decomposition(&laplacian, k);

        // K-means clustering
        let seg_ids = self.kmeans(&eigenvectors, 5.min(n_beats));

        // Find boundaries
        let mut bound_beats = Vec::new();
        for i in 1..n_beats {
            if seg_ids[i] != seg_ids[i - 1] {
                bound_beats.push(i);
            }
        }
        bound_beats.insert(0, 0);

        // Convert to segments
        let mut segments = Vec::new();
        for i in 0..bound_beats.len() - 1 {
            let start_beat = bound_beats[i] as f32;
            let end_beat = bound_beats[i + 1] as f32;
            let start_time = start_beat * 60.0 / tempo;
            let end_time = end_beat * 60.0 / tempo;
            segments.push(beatmapper_wasm_shared::SongSegment {
                start_time,
                end_time,
                segment_id: i as i32,
            });
        }

        segments
    }

    fn eigen_decomposition(&mut self, matrix: &[Vec<f32>], k: usize) -> Vec<Vec<f32>> {
        let n = matrix.len();
        let mut vectors = vec![vec![0.0f32; n]; k];
        
        // Initialize random vectors
        for j in 0..k {
            for i in 0..n {
                vectors[j][i] = self.rand_f32();
            }
            // Normalize
            let norm = vectors[j].iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for i in 0..n {
                    vectors[j][i] /= norm;
                }
            }
        }

        // Power iteration
        for _ in 0..20 {
            for j in 0..k {
                let mut result = vec![0.0f32; n];
                for i in 0..n {
                    let mut sum = 0.0;
                    for k in 0..n {
                        sum += vectors[j][k] * matrix[i][k];
                    }
                    result[i] = sum;
                }
                // Normalize
                let norm = result.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for i in 0..n {
                        result[i] /= norm;
                    }
                }
                vectors[j] = result;
            }

            // Orthogonalize (Gram-Schmidt)
            for j in 1..k {
                for i in 0..j {
                    let mut dot = 0.0;
                    for k in 0..n {
                        dot += vectors[j][k] * vectors[i][k];
                    }
                    for k in 0..n {
                        vectors[j][k] -= dot * vectors[i][k];
                    }
                }
                let norm = vectors[j].iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for i in 0..n {
                        vectors[j][i] /= norm;
                    }
                }
            }
        }
        
        // Transpose to [n_beats][k]
        let mut result = vec![vec![0.0f32; k]; n];
        for j in 0..k {
            for i in 0..n {
                result[i][j] = vectors[j][i];
            }
        }
        result
    }

    fn kmeans(&mut self, data: &[Vec<f32>], k: usize) -> Vec<usize> {
        let n = data.len();
        let d = data[0].len();
        let mut centroids = vec![vec![self.rand_f32(); d]; k];
        let mut labels = vec![0; n];
        
        for _ in 0..10 {
            // Assign
            for i in 0..n {
                let mut best_dist = f32::INFINITY;
                let mut best_k = 0;
                for k in 0..k {
                    let mut dist = 0.0;
                    for d in 0..data[i].len() {
                        let diff = data[i][d] - centroids[k][d];
                        dist += diff * diff;
                    }
                    if dist < best_dist {
                        best_dist = dist;
                        best_k = k;
                    }
                }
                labels[i] = best_k;
            }
            
            // Update centroids
            let mut counts = vec![0; k];
            for k in 0..k {
                for d in 0..centroids[0].len() {
                    centroids[k][d] = 0.0;
                }
            }
            for i in 0..n {
                let k = labels[i];
                for d in 0..centroids[0].len() {
                    centroids[k][d] += data[i][d];
                }
                counts[k] += 1;
            }
            for k in 0..k {
                if counts[k] > 0 {
                    for d in 0..centroids[0].len() {
                        centroids[k][d] /= counts[k] as f32;
                    }
                }
            }
        }
        labels
    }

    fn rand_f32(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state as f32) / (u64::MAX as f32)
    }
}
