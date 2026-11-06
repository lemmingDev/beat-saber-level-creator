//! HMM-based pattern generation using Markovify-style chains.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct HmmModel {
    pub state_size: usize,
    pub transitions: HashMap<String, HashMap<String, u32>>,
}

pub struct HmmWalker {
    model: HmmModel,
    state_size: usize,
    rng_state: u64,
}

impl HmmWalker {
    pub fn new(model_json: &str, rng_state: u64) -> Result<Self, String> {
        let model: HmmModel = serde_json::from_str(model_json)
            .map_err(|e| format!("Failed to parse HMM model: {}", e))?;
        
        Ok(HmmWalker {
            state_size: model.state_size,
            model,
            rng_state,
        })
    }

    fn next_u64(&mut self) -> u64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        self.rng_state
    }

    fn rand_choice(&mut self, options: &[(String, u32)]) -> Option<String> {
        if options.is_empty() {
            return None;
        }
        
        let total: u32 = options.iter().map(|(_, w)| w).sum();
        if total == 0 {
            return None;
        }
        
        let r = (self.next_u64() as f64) / (u64::MAX as f64) * total as f64;
        let mut acc = 0.0;
        
        for (key, weight) in options {
            acc += *weight as f64;
            if r < acc {
                return Some(key.clone());
            }
        }
        
        Some(options.last().unwrap().0.clone())
    }

    /// Walk the Markov chain to generate a sequence of tokens
    pub fn walk(&mut self, max_length: usize) -> Vec<String> {
        let mut result = Vec::new();
        let mut state: Vec<String> = vec!["___BEGIN__".to_string(); self.state_size];
        
        for _ in 0..max_length {
            let state_key = state.join("|");
            let transitions = match self.model.transitions.get(&state_key) {
                Some(t) => t,
                None => break,
            };
            
            let options: Vec<(String, u32)> = transitions.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            
            let next = match self.rand_choice(&options) {
                Some(n) => n,
                None => break,
            };
            
            if next == "___END__" {
                break;
            }
            
            result.push(next.clone());
            
            // Update state (shift and append)
            state.remove(0);
            state.push(next);
        }
        
        result
    }
}

// Function to parse a walk token into note properties
pub fn parse_token(token: &str) -> Option<NoteToken> {
    // Format: "type,lineIndex,lineLayer,direction,..."
    // Example: "0,1,0,1,999,999,999,999,999,999,999,999"
    let parts: Vec<&str> = token.split(',').collect();
    if parts.len() < 4 {
        return None;
    }
    
    let note_type = parts[0].parse::<u8>().ok()?;
    let line_index = parts[1].parse::<u8>().ok()?;
    let line_layer = parts[2].parse::<u8>().ok()?;
    let direction = parts[3].parse::<u8>().ok()?;
    
    Some(NoteToken {
        note_type,
        line_index,
        line_layer,
        direction,
    })
}

#[derive(Debug, Clone)]
pub struct NoteToken {
    pub note_type: u8,
    pub line_index: u8,
    pub line_layer: u8,
    pub direction: u8,
}
