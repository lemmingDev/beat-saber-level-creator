//! Audio analysis for Beat Saber mapping: tempo, beats, onsets, key detection, segmentation.

use beatmapper_wasm_shared::{
    AudioAnalysisResult, Onset, OnsetType, SongSegment, init_panic_hook,
};
use wasm_bindgen::prelude::*;
use web_sys::console;
use serde_json;

mod onset;
mod tempo;
mod spectral;
mod key;
mod segmentation;

use onset::OnsetDetector;
use tempo::TempoDetector;
use spectral::SpectralAnalyzer;
use key::KeyDetector;
use segmentation::SegmentDetector;

#[wasm_bindgen]
pub struct AudioAnalyzer {
    sample_rate: u32,
    onset_detector: OnsetDetector,
    tempo_detector: TempoDetector,
    spectral_analyzer: SpectralAnalyzer,
    key_detector: KeyDetector,
    segment_detector: SegmentDetector,
}

#[wasm_bindgen]
impl AudioAnalyzer {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32) -> AudioAnalyzer {
        init_panic_hook();
        
        AudioAnalyzer {
            sample_rate,
            onset_detector: OnsetDetector::new(sample_rate),
            tempo_detector: TempoDetector::new(sample_rate),
            spectral_analyzer: SpectralAnalyzer::new(sample_rate),
            key_detector: KeyDetector::new(sample_rate),
            segment_detector: SegmentDetector::new(sample_rate),
        }
    }

    pub fn analyze(&mut self, audio: &[f32]) -> Result<String, JsValue> {
        console::log_1(&format!("Analyzing {} samples at {} Hz", audio.len(), self.sample_rate).into());

        let spectral_result = self.spectral_analyzer.analyze(audio);
        let onsets = self.onset_detector.detect(&spectral_result);
        
        let (tempo, tempo_confidence, beats, downbeats) = 
            self.tempo_detector.detect(&onsets, &spectral_result, audio.len() as f32 / self.sample_rate as f32);
        
        let (key, key_confidence, is_major) = self.key_detector.detect(audio);
        
        let segments = self.segment_detector.detect(&spectral_result, &onsets, &beats, tempo);

        let result = AudioAnalysisResult {
            sample_rate: self.sample_rate,
            duration: audio.len() as f32 / self.sample_rate as f32,
            tempo,
            tempo_confidence,
            beats: beats.to_vec(),
            downbeats: downbeats.to_vec(),
            onsets,
            spectral_flux: spectral_result.spectral_flux,
            rms_energy: spectral_result.rms_energy,
            key,
            key_confidence,
            is_major,
            segments,
        };

        serde_json::to_string(&result)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn analyze_chunk(&mut self, audio: &[f32]) -> Result<String, JsValue> {
        let spectral_result = self.spectral_analyzer.analyze_chunk(audio);
        let onsets = self.onset_detector.detect_chunk(&spectral_result);
        
        #[derive(serde::Serialize)]
        struct ChunkResult {
            onsets: Vec<Onset>,
            spectral_flux: Vec<f32>,
            rms_energy: Vec<f32>,
        }
        
        serde_json::to_string(&ChunkResult {
            onsets,
            spectral_flux: spectral_result.spectral_flux,
            rms_energy: spectral_result.rms_energy,
        }).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn get_tempo(&self) -> f32 {
        self.tempo_detector.current_tempo()
    }
}

#[wasm_bindgen]
pub struct AnalysisConfig {
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub onset_threshold: f32,
    pub window_size: usize,
    pub hop_size: usize,
}

#[wasm_bindgen]
impl AnalysisConfig {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            min_bpm: 60.0,
            max_bpm: 200.0,
            onset_threshold: 0.3,
            window_size: 2048,
            hop_size: 512,
        }
    }
}
