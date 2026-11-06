//! Shared types between WASM modules and the web frontend.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Audio analysis results passed from Rust to JS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioAnalysisResult {
    /// Sample rate of the analyzed audio
    pub sample_rate: u32,
    /// Duration in seconds
    pub duration: f32,
    /// Detected tempo in BPM
    pub tempo: f32,
    /// Confidence of tempo detection (0.0-1.0)
    pub tempo_confidence: f32,
    /// Beat timestamps in seconds
    pub beats: Vec<f32>,
    /// Downbeat timestamps (first beat of each measure)
    pub downbeats: Vec<f32>,
    /// Onset detection results
    pub onsets: Vec<Onset>,
    /// Frequency band energy over time (for visualization)
    pub spectral_flux: Vec<f32>,
    /// RMS energy over time
    pub rms_energy: Vec<f32>,
    /// Estimated key (0-11 for C-B, -1 if unknown)
    pub key: i8,
    /// Key confidence (0.0-1.0)
    pub key_confidence: f32,
    /// Whether the track is likely major (true) or minor (false)
    pub is_major: bool,
    /// Song segments (intro, verse, chorus, etc.)
    pub segments: Vec<SongSegment>,
}

/// Individual onset detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Onset {
    /// Time in seconds
    pub time: f32,
    /// Strength of the onset (0.0-1.0)
    pub strength: f32,
    /// Type of onset
    pub onset_type: OnsetType,
}

/// Type of onset detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[wasm_bindgen]
pub enum OnsetType {
    /// Percussive onset (kick, snare, etc.)
    Percussive = 0,
    /// Harmonic onset (synth, vocals, etc.)
    Harmonic = 1,
    /// General broadband onset
    Broadband = 2,
}

/// Song segment (intro, verse, chorus, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongSegment {
    /// Start time in seconds
    pub start_time: f32,
    /// End time in seconds
    pub end_time: f32,
    /// Segment identifier
    pub segment_id: i32,
}

/// Pattern suggestion request from JS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRequest {
    /// Target time range in seconds
    pub start_time: f32,
    pub end_time: f32,
    /// Desired note density (notes per second)
    pub target_nps: f32,
    /// Difficulty hint (0.0-1.0)
    pub difficulty: f32,
    /// Preferred pattern styles
    pub styles: Vec<PatternStyle>,
    /// Constraints
    pub constraints: PatternConstraints,
    /// Section length in beats (0 = auto)
    pub section_beats: f32,
    /// Whether to repeat patterns for repeated sections
    pub repeat_for_segments: bool,
    pub seed: Option<u64>,
    pub generation_method: String,
    pub audio_analysis: Option<String>,
}

/// Pattern style preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[wasm_bindgen]
pub enum PatternStyle {
    /// Alternating left/right
    Alternating = 0,
    /// One-handed streams
    OneHanded = 1,
    /// Cross-body patterns
    CrossBody = 2,
    /// Jump patterns
    Jumps = 3,
    /// Flow/tech patterns
    Flow = 4,
    /// Random/variety
    Random = 5,
}

/// Constraints for pattern generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternConstraints {
    /// Minimum time between notes (seconds)
    pub min_note_spacing: Option<f32>,
    /// Maximum NPS
    pub max_nps: Option<f32>,
    /// Allowed note types
    pub allowed_types: Vec<NoteType>,
    /// Forbid certain lanes
    pub forbidden_lanes: Vec<u8>,
    /// Require symmetry
    pub symmetric: bool,
}

impl Default for PatternConstraints {
    fn default() -> Self {
        Self {
            min_note_spacing: Some(0.05),
            max_nps: Some(20.0),
            allowed_types: vec![NoteType::Red, NoteType::Blue],
            forbidden_lanes: vec![],
            symmetric: false,
        }
    }
}

/// Beat Saber note types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[wasm_bindgen]
pub enum NoteType {
    Red = 0,
    Blue = 1,
    Bomb = 2,
    Wall = 3,
    Arc = 4,
    Chain = 5,
}

/// Generated pattern response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternSuggestion {
    /// Unique ID for this suggestion
    pub id: String,
    /// Notes in the pattern
    pub notes: Vec<PatternNote>,
    /// Pattern style used
    pub style: PatternStyle,
    /// Estimated difficulty (0.0-1.0)
    pub difficulty: f32,
    /// NPS of this pattern
    pub nps: f32,
    /// Confidence score
    pub confidence: f32,
}

/// A single note in a pattern suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternNote {
    /// Time in seconds
    pub time: f32,
    /// Lane X (0-3)
    pub lane_x: u8,
    /// Lane Y (0-2)
    pub lane_y: u8,
    /// Note type
    pub note_type: NoteType,
    /// Cut direction (0-8, 9 = any/dot)
    pub cut_direction: u8,
    /// Angle offset for arcs/chains
    pub angle_offset: Option<f32>,
}

/// Initialize panic hook for better error messages in browser
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Convert JSON string to AudioAnalysisResult
#[wasm_bindgen]
pub fn parse_audio_analysis(json: &str) -> Result<String, JsValue> {
    let _: AudioAnalysisResult = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(json.to_string())
}

/// Convert PatternRequest JSON to validated JSON
#[wasm_bindgen]
pub fn validate_pattern_request(json: &str) -> Result<String, JsValue> {
    let _: PatternRequest = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(json.to_string())
}
