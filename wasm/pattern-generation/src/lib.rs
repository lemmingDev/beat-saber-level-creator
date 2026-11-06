//! AI-assisted pattern generation for Beat Saber maps.

use beatmapper_wasm_shared::{
    PatternRequest, PatternSuggestion, PatternNote, PatternStyle, PatternConstraints,
    NoteType, init_panic_hook,
};
use wasm_bindgen::prelude::*;
use web_sys::console;
use getrandom::getrandom;
use serde_json;
mod hmm;

#[derive(serde::Deserialize)]
struct AudioAnalysis {
    tempo: f32,
    onsets: Vec<AudioOnset>,
    spectral_flux: Vec<f32>,
    rms_energy: Vec<f32>,
    beats: Vec<f32>,
}

#[derive(serde::Deserialize)]
struct AudioOnset {
    time: f32,
    strength: f32,
    onset_type: i32,
}

use hmm::{HmmWalker, parse_token};

#[wasm_bindgen]
pub struct PatternGenerator {
    rng_state: u64,
    hmm_model: Option<hmm::HmmWalker>,
}

#[wasm_bindgen]
impl PatternGenerator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> PatternGenerator {
        init_panic_hook();
        let mut seed = [0u8; 8];
        getrandom(&mut seed).unwrap_or(());
        let rng_state = u64::from_le_bytes(seed);
        PatternGenerator { 
            rng_state,
            hmm_model: None,
        }
    }

    /// Generate pattern suggestions based on audio analysis and constraints
    /// Request and response are JSON strings
    #[wasm_bindgen]
    pub fn generate(&mut self, request_json: &str) -> Result<String, JsValue> {
        let request: PatternRequest = serde_json::from_str(request_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        // Seed the RNG
        if let Some(seed) = request.seed {
            self.rng_state = seed;
        } else {
            // Use getrandom for additional entropy
            let mut buf = [0u8; 8];
            getrandom(&mut buf).unwrap_or(());
            let random_seed = u64::from_le_bytes(buf);
            self.rng_state ^= random_seed;
        }

        console::log_1(&format!("Generating patterns for {:.1}-{:.1}s ({} beats, method: {})", 
            request.start_time, request.end_time, request.section_beats, request.generation_method).into());

        let mut suggestions = Vec::new();

        for style in &request.styles {
            let count = match style {
                PatternStyle::Random => 3,
                _ => 2,
            };

            for i in 0..count {
                if let Some(suggestion) = self.generate_for_style(&request, *style, i) {
                    suggestions.push(suggestion);
                }
            }
        }

        // Sort by confidence
        suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        serde_json::to_string(&suggestions)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    fn generate_for_style(
        &mut self,
        request: &PatternRequest,
        style: PatternStyle,
        variant: usize,
    ) -> Option<PatternSuggestion> {
        let duration = request.end_time - request.start_time;
        if duration <= 0.0 { return None; }

        let target_notes = ((request.end_time - request.start_time) * request.target_nps).round() as usize;
        if target_notes == 0 { return None; }

        // Add variant-based variation to the seed
        let variant_seed = self.next_u64() ^ (variant as u64).wrapping_mul(0x9e3779b97f4a7c15);
        self.rng_state = variant_seed;

        let notes = if request.generation_method == "local-ml" && self.hmm_model.is_some() {
            self.generate_hmm_pattern(request, target_notes)
        } else {
            match style {
                PatternStyle::Alternating => self.generate_alternating(request, target_notes),
                PatternStyle::OneHanded => self.generate_one_handed(request, target_notes),
                PatternStyle::CrossBody => self.generate_cross_body(request, target_notes),
                PatternStyle::Jumps => self.generate_jumps(request, target_notes),
                PatternStyle::Flow => self.generate_flow(request, target_notes),
                PatternStyle::Random => self.generate_random(request, target_notes),
            }
        };

        if notes.is_empty() { return None; }

        let nps = notes.len() as f32 / (request.end_time - request.start_time).max(0.001);
        let difficulty = self.estimate_difficulty(&notes, nps);

        Some(PatternSuggestion {
            id: format!("{:?}_{}_{}", style, variant, self.next_id()),
            notes: notes.clone(),
            style,
            difficulty,
            nps,
            confidence: self.compute_confidence(&notes, request),
        })
    }

    // --- Pattern generation methods ---

    fn generate_hmm_pattern(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        
        if let Some(ref mut walker) = self.hmm_model {
            let walk = walker.walk(target_notes * 2);
            
            let lanes = self.allowed_lanes(&request.constraints);
            if lanes.is_empty() { return notes; }
            
            let interval = (request.end_time - request.start_time) / target_notes as f32;
            let is_easy = self.is_easy(request);
            
            let mut last_color = NoteType::Red;
            let mut same_color_count = 0;
            
            for (i, token) in walk.iter().take(target_notes).enumerate() {
                if let Some(note_token) = parse_token(token) {
                    let time = request.start_time + i as f32 * interval;
                    let mut note_type = match note_token.note_type {
                        0 => NoteType::Red,
                        1 => NoteType::Blue,
                        2 => NoteType::Bomb,
                        _ => NoteType::Red,
                    };
                    
                    if note_type == last_color {
                        same_color_count += 1;
                        if same_color_count >= 2 {
                            note_type = if note_type == NoteType::Red { NoteType::Blue } else { NoteType::Red };
                            same_color_count = 0;
                        }
                    } else {
                        same_color_count = 0;
                    }
                    last_color = note_type;
                    
                    // Enforce color-lane correlation: Red=left, Blue=right
                    let is_easy = self.is_easy(request);
                    let color_lanes = self.lanes_for_color(&request.constraints, note_type, is_easy);
                    let (lane_x, lane_y) = if !color_lanes.is_empty() {
                        let l = color_lanes[self.rand_usize(color_lanes.len())];
                        (l.0, l.1)
                    } else {
                        (note_token.line_index.min(3), note_token.line_layer.min(2))
                    };
                    
                    notes.push(PatternNote {
                        time,
                        lane_x,
                        lane_y,
                        note_type,
                        cut_direction: self.get_cut_direction(i, note_type, is_easy),
                        angle_offset: None,
                    });
                } else {
                    let time = request.start_time + i as f32 * interval;
                    let lanes = self.allowed_lanes(&request.constraints);
                    if !lanes.is_empty() {
                        let color = if last_color == NoteType::Red { NoteType::Blue } else { NoteType::Red };
                        last_color = color;
                        
                        let color_lanes = self.lanes_for_color(&request.constraints, color, is_easy);
                        if !color_lanes.is_empty() {
                            let lane = color_lanes[self.rand_usize(color_lanes.len())];
                            notes.push(PatternNote {
                                time,
                                lane_x: lane.0,
                                lane_y: lane.1,
                                note_type: color,
                                cut_direction: self.get_cut_direction(i, color, is_easy),
                                angle_offset: None,
                            });
                        }
                    }
                }
            }
        }
        
        let mut result = Vec::new();
        let mut last_color = NoteType::Red;
        let mut same_count = 0;
        
        for note in notes {
            if note.note_type == last_color {
                same_count += 1;
                if same_count >= 3 {
                    let new_color = if note.note_type == NoteType::Red { NoteType::Blue } else { NoteType::Red };
                    let mut new_note = note.clone();
                    new_note.note_type = new_color;
                    result.push(new_note);
                    last_color = new_color;
                    same_count = 0;
                    continue;
                }
            } else {
                same_count = 0;
            }
            last_color = note.note_type;
            result.push(note);
        }
        
        result
    }
    
    fn generate_alternating(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        let interval = (request.end_time - request.start_time) / target_notes.max(1) as f32;
        let is_easy = self.is_easy(request);
        
        let start_color = if self.rand_u8(2) == 0 { NoteType::Red } else { NoteType::Blue };

        for i in 0..target_notes {
            let time = request.start_time + i as f32 * interval;
            
            // Alternate colors
            let note_type = if i % 2 == 0 { start_color } else {
                if start_color == NoteType::Red { NoteType::Blue } else { NoteType::Red }
            };
            
            // Get lanes for this color
            let color_lanes = self.lanes_for_color(&request.constraints, note_type, is_easy);
            if color_lanes.is_empty() { continue; }
            
            let lane_idx = self.rand_usize(color_lanes.len());
            let lane = color_lanes[lane_idx];
            let cut_dir = self.get_cut_direction(i, note_type, is_easy);
            
            notes.push(PatternNote {
                time,
                lane_x: lane.0,
                lane_y: lane.1,
                note_type,
                cut_direction: cut_dir,
                angle_offset: None,
            });
        }
        notes
    }

    fn generate_one_handed(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        let interval = (request.end_time - request.start_time) / target_notes.max(1) as f32;
        let is_easy = self.is_easy(request);
        
        // Choose which hand (left or right side)
        let left_hand = self.rand_u8(2) == 0;
        let color = if left_hand { NoteType::Red } else { NoteType::Blue };
        
        let hand_lanes = self.lanes_for_color(&request.constraints, color, is_easy);
        if hand_lanes.is_empty() { return notes; }

        let start_lane_idx = self.rand_usize(hand_lanes.len());

        for i in 0..target_notes {
            let time = request.start_time + i as f32 * interval;
            let lane = hand_lanes[(start_lane_idx + i) % hand_lanes.len()];
            
            let cut_dir = self.get_cut_direction(i, color, is_easy);
            
            notes.push(PatternNote {
                time,
                lane_x: lane.0,
                lane_y: lane.1,
                note_type: color,
                cut_direction: cut_dir,
                angle_offset: None,
            });
        }
        notes
    }

    fn generate_cross_body(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        let interval = (request.end_time - request.start_time) / target_notes.max(1) as f32;
        let is_easy = self.is_easy(request);
        
        let red_lanes = self.lanes_for_color(&request.constraints, NoteType::Red, is_easy);
        let blue_lanes = self.lanes_for_color(&request.constraints, NoteType::Blue, is_easy);
        
        if red_lanes.is_empty() || blue_lanes.is_empty() { return notes; }

        let start_left = self.rand_u8(2) == 0;

        for i in 0..target_notes {
            let time = request.start_time + i as f32 * interval;
            let (lane, color) = if start_left {
                if i % 2 == 0 {
                    (red_lanes[i / 2 % red_lanes.len()], NoteType::Red)
                } else {
                    (blue_lanes[i / 2 % blue_lanes.len()], NoteType::Blue)
                }
            } else {
                if i % 2 == 0 {
                    (blue_lanes[i / 2 % blue_lanes.len()], NoteType::Blue)
                } else {
                    (red_lanes[i / 2 % red_lanes.len()], NoteType::Red)
                }
            };
            
            notes.push(PatternNote {
                time,
                lane_x: lane.0,
                lane_y: lane.1,
                note_type: color,
                cut_direction: self.get_cut_direction(i, color, is_easy),
                angle_offset: None,
            });
        }
        notes
    }

    fn generate_jumps(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        let interval = (request.end_time - request.start_time) / (target_notes / 2).max(1) as f32;
        let is_easy = self.is_easy(request);
        
        let red_lanes = self.lanes_for_color(&request.constraints, NoteType::Red, is_easy);
        let blue_lanes = self.lanes_for_color(&request.constraints, NoteType::Blue, is_easy);
        
        if red_lanes.is_empty() || blue_lanes.is_empty() { return notes; }

        let start_red_idx = self.rand_usize(red_lanes.len());
        let start_blue_idx = self.rand_usize(blue_lanes.len());

        let mut i = 0;
        while i < target_notes {
            let time = request.start_time + (i / 2) as f32 * interval;
            
            let r = red_lanes[(start_red_idx + i / 2) % red_lanes.len()];
            let b = blue_lanes[(start_blue_idx + i / 2) % blue_lanes.len()];
            
            notes.push(PatternNote {
                time,
                lane_x: r.0,
                lane_y: r.1,
                note_type: NoteType::Red,
                cut_direction: self.get_cut_direction(i, NoteType::Red, is_easy),
                angle_offset: None,
            });
            i += 1;
            if i >= target_notes { break; }
            
            notes.push(PatternNote {
                time,
                lane_x: b.0,
                lane_y: b.1,
                note_type: NoteType::Blue,
                cut_direction: self.get_cut_direction(i, NoteType::Blue, is_easy),
                angle_offset: None,
            });
            i += 1;
        }
        notes
    }

    fn generate_flow(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        let interval = (request.end_time - request.start_time) / target_notes.max(1) as f32;
        let is_easy = self.is_easy(request);
        
        let red_lanes = self.lanes_for_color(&request.constraints, NoteType::Red, is_easy);
        let blue_lanes = self.lanes_for_color(&request.constraints, NoteType::Blue, is_easy);
        
        if red_lanes.is_empty() || blue_lanes.is_empty() { return notes; }

        let mut last_color = if self.rand_u8(2) == 0 { NoteType::Red } else { NoteType::Blue };

        for i in 0..target_notes {
            let time = request.start_time + i as f32 * interval;
            
            // Alternate colors
            let color = if i % 2 == 0 { last_color } else {
                if last_color == NoteType::Red { NoteType::Blue } else { NoteType::Red }
            };
            last_color = color;
            
            let lanes = if color == NoteType::Red { &red_lanes } else { &blue_lanes };
            let lane = lanes[self.rand_usize(lanes.len())];
            
            notes.push(PatternNote {
                time,
                lane_x: lane.0,
                lane_y: lane.1,
                note_type: color,
                cut_direction: self.get_cut_direction(i, color, is_easy),
                angle_offset: None,
            });
        }
        notes
    }

    fn generate_random(&mut self, request: &PatternRequest, target_notes: usize) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        let interval = (request.end_time - request.start_time) / target_notes.max(1) as f32;
        let is_easy = self.is_easy(request);
        
        let mut last_color = if self.rand_u8(2) == 0 { NoteType::Red } else { NoteType::Blue };

        for i in 0..target_notes {
            let time = request.start_time + i as f32 * interval;
            
            // Alternate colors
            let color = if i % 2 == 0 { last_color } else {
                if last_color == NoteType::Red { NoteType::Blue } else { NoteType::Red }
            };
            last_color = color;
            
            let color_lanes = self.lanes_for_color(&request.constraints, color, is_easy);
            if color_lanes.is_empty() { continue; }
            
            let lane = color_lanes[self.rand_usize(color_lanes.len())];
            let cut_dir = self.get_cut_direction(i, color, is_easy);
            
            notes.push(PatternNote {
                time,
                lane_x: lane.0,
                lane_y: lane.1,
                note_type: color,
                cut_direction: cut_dir,
                angle_offset: None,
            });
        }
        notes
    }

    /// Load an HMM model from JSON string
    #[wasm_bindgen]
    pub fn load_hmm_model(&mut self, model_json: &str) -> Result<(), JsValue> {
        console::log_1(&"Loading HMM model...".into());
        match hmm::HmmWalker::new(model_json, self.rng_state) {
            Ok(walker) => {
                self.hmm_model = Some(walker);
                console::log_1(&"HMM model loaded successfully".into());
                Ok(())
            }
            Err(e) => {
                console::error_1(&format!("Failed to load HMM model: {}", e).into());
                Err(JsValue::from_str(&e))
            }
        }
    }

    /// Check if HMM model is loaded
    #[wasm_bindgen]
    pub fn has_hmm_model(&self) -> bool {
        self.hmm_model.is_some()
    }

    // Parse audio analysis JSON if available
    fn parse_audio_analysis(&self, json_str: &Option<String>) -> Option<AudioAnalysis> {
        json_str.as_ref().and_then(|s| {
            serde_json::from_str::<AudioAnalysis>(s).ok()
        })
    }

    // --- Helpers ---

    fn allowed_lanes(&self, constraints: &PatternConstraints) -> Vec<(u8, u8)> {
        let mut lanes = Vec::new();
        for x in 0..4 {
            for y in 0..3 {
                if !constraints.forbidden_lanes.contains(&(x * 3 + y)) {
                    lanes.push((x, y));
                }
            }
        }
        lanes
    }

    /// Get lanes appropriate for a given color (Red=left, Blue=right)
    fn lanes_for_color(&self, constraints: &PatternConstraints, color: NoteType, is_easy: bool) -> Vec<(u8, u8)> {
        let all = self.allowed_lanes(constraints);
        let filtered: Vec<_> = all.into_iter().filter(|(x, _)| {
            match color {
                NoteType::Red => *x <= 1,
                NoteType::Blue => *x >= 2,
                _ => true,
            }
        }).collect();
        
        if is_easy {
            // Easy: only inner lanes, bottom layer
            filtered.into_iter().filter(|(x, y)| {
                (*x == 1 || *x == 2) && *y == 0
            }).collect()
        } else {
            filtered
        }
    }

    /// Check if this is an easy difficulty (based on target NPS)
    fn is_easy(&self, request: &PatternRequest) -> bool {
        request.target_nps <= 0.7
    }

    /// Get cut direction based on difficulty and color
    fn get_cut_direction(&self, index: usize, color: NoteType, is_easy: bool) -> u8 {
        if is_easy {
            return 8; // Dot notes for easy
        }
        match color {
            NoteType::Red => [0, 1, 2, 3, 4, 5, 6, 7][index % 8],
            NoteType::Blue => [0, 1, 2, 3, 4, 5, 6, 7][index % 8],
            _ => 8,
        }
    }

    fn estimate_difficulty(&self, notes: &[PatternNote], nps: f32) -> f32 {
        let mut difficulty = (nps / 20.0).min(1.0) * 0.5;
        
        let mut color_changes = 0;
        let mut direction_changes = 0;
        for i in 1..notes.len() {
            if notes[i].note_type != notes[i-1].note_type { color_changes += 1; }
            if notes[i].cut_direction != notes[i-1].cut_direction { direction_changes += 1; }
        }
        difficulty += (color_changes as f32 / notes.len() as f32) * 0.2;
        difficulty += (direction_changes as f32 / notes.len() as f32) * 0.15;
        
        for i in 1..notes.len() {
            let dx = notes[i].lane_x as i32 - notes[i-1].lane_x as i32;
            let dy = notes[i].lane_y as i32 - notes[i-1].lane_y as i32;
            let dist = ((dx*dx + dy*dy) as f32).sqrt();
            if dist > 2.0 { difficulty += 0.1; }
        }
        
        difficulty.clamp(0.0, 1.0)
    }

    fn compute_confidence(&self, notes: &[PatternNote], request: &PatternRequest) -> f32 {
        let mut conf = 0.7;
        
        let actual_nps = notes.len() as f32 / (request.end_time - request.start_time).max(0.001);
        let nps_diff = (actual_nps - request.target_nps).abs() / request.target_nps.max(1.0);
        conf -= nps_diff * 0.3;
        
        let diff = self.estimate_difficulty(notes, actual_nps);
        let diff_err = (diff - request.difficulty).abs();
        conf -= diff_err * 0.2;
        
        conf.clamp(0.1, 1.0)
    }

    fn alternating_cut_direction(&self, index: usize, note_type: NoteType) -> u8 {
        match (note_type, index % 4) {
            (NoteType::Red, 0) => 1,
            (NoteType::Red, 1) => 3,
            (NoteType::Red, 2) => 5,
            (NoteType::Red, _) => 7,
            (NoteType::Blue, 0) => 1,
            (NoteType::Blue, 1) => 2,
            (NoteType::Blue, 2) => 4,
            (NoteType::Blue, _) => 6,
            _ => 9,
        }
    }

    fn flow_cut_direction(&self, index: usize, note_type: NoteType) -> u8 {
        match note_type {
            NoteType::Red => [1, 3, 5, 7][index % 4],
            NoteType::Blue => [1, 2, 4, 6][index % 4],
            _ => 9,
        }
    }

    fn cross_body_cut_direction(&self, index: usize, note_type: NoteType) -> u8 {
        match note_type {
            NoteType::Red => [2, 6][index % 2],
            NoteType::Blue => [3, 7][index % 2],
            _ => 9,
        }
    }

    fn flow_next_lane(&mut self, lanes: &[(u8, u8)], current: (u8, u8)) -> (u8, u8) {
        let mut candidates: Vec<_> = lanes.iter()
            .map(|&l| {
                let dx = l.0 as i32 - current.0 as i32;
                let dy = l.1 as i32 - current.1 as i32;
                let dist = (dx*dx + dy*dy) as f32;
                (l, dist)
            })
            .collect();
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        let idx = self.rand_usize(candidates.len().min(3));
        candidates[idx].0
    }

    fn flow_angle_offset(&self, from: (u8, u8), to: (u8, u8)) -> f32 {
        let dx = to.0 as f32 - from.0 as f32;
        let dy = to.1 as f32 - from.1 as f32;
        dy.atan2(dx)
    }

    // RNG helpers (xorshift64*)
    fn next_u64(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    fn rand_usize(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max
    }

    fn rand_u8(&mut self, max: u8) -> u8 {
        (self.next_u64() as u8) % max
    }

    fn next_id(&mut self) -> u64 {
        self.next_u64()
    }
}
