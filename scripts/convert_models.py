#!/usr/bin/env python3
"""Convert HMM pickle models to compressed JSON for WASM use."""

import pickle
import json
import gzip
import os

MODELS_DIR = "/Users/jwill872/Downloads/HMM_models copy"
OUTPUT_DIR = "/Applications/XAMPP/xamppfiles/htdocs/beatSaberLevelCreator/beatmapper/wasm/models"

DIFFICULTIES = ["easy", "normal", "hard", "expert", "expertPlus"]
VERSION = "1"  # Use v1 models (smaller, higher quality)

def convert_model(difficulty, version):
    """Convert a single model to compressed JSON."""
    name = f"HMM_{difficulty}.pkl" if version == "1" else f"HMM_{difficulty}_v2.pkl"
    path = os.path.join(MODELS_DIR, name)
    
    if not os.path.exists(path):
        print(f"  Model not found: {path}")
        return None
    
    print(f"  Loading {name}...")
    with open(path, 'rb') as f:
        model = pickle.load(f)
    
    print(f"    States: {len(model.model)}, state_size: {model.state_size}")
    
    # Convert to JSON using markovify's to_json
    json_str = model.to_json()
    json_data = json.loads(json_str)
    
    # Compact the format: store as object with state -> transitions
    compact = {
        "state_size": model.state_size,
        "transitions": {}
    }
    
    for state_transitions in json_data:
        if isinstance(state_transitions, list) and len(state_transitions) == 2:
            state = state_transitions[0]
            transitions = state_transitions[1]
            state_key = "|".join(state) if isinstance(state, list) else str(state)
            compact["transitions"][state_key] = transitions
    
    # Compress with gzip
    json_bytes = json.dumps(compact).encode('utf-8')
    compressed = gzip.compress(json_bytes, compresslevel=9)
    
    # Save compressed file
    output_name = f"HMM_{difficulty}_v{version}.json.gz"
    output_path = os.path.join(OUTPUT_DIR, output_name)
    with open(output_path, 'wb') as f:
        f.write(compressed)
    
    print(f"    JSON: {len(json_bytes) / 1024:.0f} KB, Compressed: {len(compressed) / 1024:.0f} KB")
    return output_path

def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    
    print(f"Converting v{VERSION} models...")
    for difficulty in DIFFICULTIES:
        print(f"\n{difficulty}:")
        convert_model(difficulty, VERSION)
    
    print(f"\nDone! Models saved to {OUTPUT_DIR}")

if __name__ == "__main__":
    main()
