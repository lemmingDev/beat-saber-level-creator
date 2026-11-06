import { useCallback, useEffect, useRef, useState } from "react";

let audioModule: any = null;
let patternModule: any = null;
let audioInitPromise: Promise<any> | null = null;
let patternInitPromise: Promise<any> | null = null;

// Eagerly start loading WASM modules at app startup
audioInitPromise = (async () => {
  try {
    const wasmUrl = new URL("/wasm/beatmapper_audio.js", import.meta.url).href;
    const module = await import(/* @vite-ignore */ wasmUrl);
    await module.default();
    audioModule = module;
    audioModule.init_panic_hook();
    console.log("[WASM Audio] Module loaded, exports:", Object.keys(audioModule).filter(k => typeof audioModule[k] === 'function' || typeof audioModule[k] === 'class'));
    return audioModule!;
  } catch (err) {
    console.error("[WASM Audio] Failed to load module:", err);
    throw err;
  }
})();

patternInitPromise = (async () => {
  try {
    const wasmUrl = new URL("/wasm/beatmapper_pattern.js", import.meta.url).href;
    const module = await import(/* @vite-ignore */ wasmUrl);
    await module.default();
    patternModule = module;
    patternModule.init_panic_hook();
    console.log("[WASM Pattern] Module loaded, exports:", Object.keys(patternModule).filter(k => typeof patternModule[k] === 'function' || typeof patternModule[k] === 'class'));
    return patternModule!;
  } catch (err) {
    console.error("[WASM Pattern] Failed to load module:", err);
    throw err;
  }
})();

async function loadAudioModule(): Promise<any> {
  if (audioModule) return audioModule;
  return audioInitPromise!;
}

async function loadPatternModule(): Promise<any> {
  if (patternModule) return patternModule;
  return patternInitPromise!;
}

export function useWasmAudio() {
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loadingProgress, setLoadingProgress] = useState(0);
  const analyzerRef = useRef<any>(null);
  const generatorRef = useRef<any>(null);

  useEffect(() => {
    let mounted = true;
    let progressInterval: NodeJS.Timeout;
    
    let progress = 0;
    progressInterval = setInterval(() => {
      if (mounted) {
        progress = Math.min(progress + Math.random() * 15, 90);
        setLoadingProgress(progress);
      }
    }, 200);

    Promise.all([loadAudioModule(), loadPatternModule()])
      .then(([audioMod, patternMod]) => {
        if (mounted) {
          clearInterval(progressInterval);
          setLoadingProgress(100);
          audioMod.init_panic_hook();
          setIsReady(true);
          console.log("[WASM] Both modules ready. AudioAnalyzer:", typeof audioMod.AudioAnalyzer, "PatternGenerator:", typeof patternMod.PatternGenerator);
        }
      })
      .catch((err) => {
        if (mounted) {
          clearInterval(progressInterval);
          setError(err.message);
          setIsReady(false);
        }
      });
    return () => { 
      mounted = false; 
      clearInterval(progressInterval);
    };
  }, []);

  const analyzeAudio = useCallback(async (audioBuffer: AudioBuffer): Promise<any> => {
    if (!isReady) {
      setError("WASM module not ready");
      return null;
    }
    try {
      const mod = await loadAudioModule();
      if (!analyzerRef.current) {
        analyzerRef.current = new mod.AudioAnalyzer(audioBuffer.sampleRate);
      }
      
      const length = audioBuffer.length;
      const channels = audioBuffer.numberOfChannels;
      const mono = new Float32Array(length);
      
      for (let ch = 0; ch < channels; ch++) {
        const channelData = audioBuffer.getChannelData(ch);
        for (let i = 0; i < length; i++) {
          mono[i] += channelData[i];
        }
      }
      for (let i = 0; i < length; i++) {
        mono[i] /= channels;
      }
      
      const resultJson = analyzerRef.current.analyze(mono);
      return JSON.parse(resultJson);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Analysis failed");
      return null;
    }
  }, [isReady]);

  const generatePatterns = useCallback(async (request: any): Promise<any> => {
    if (!isReady) {
      setError("WASM module not ready");
      return null;
    }
    try {
      const mod = await loadPatternModule();
      console.log("[WASM] PatternGenerator type:", typeof mod.PatternGenerator);
      
      if (!generatorRef.current) {
        generatorRef.current = new mod.PatternGenerator();
      }
      
      const requestJson = JSON.stringify(request);
      const resultJson = generatorRef.current.generate(requestJson);
      return JSON.parse(resultJson);
    } catch (err) {
      console.error("[WASM] Pattern generation error:", err);
      setError(err instanceof Error ? err.message : "Pattern generation failed");
      return null;
    }
  }, [isReady]);

  const loadHMMModel = useCallback(async (modelUrl: string): Promise<boolean> => {
    if (!isReady) return false;
    try {
      const mod = await loadPatternModule();
      if (!generatorRef.current) {
        generatorRef.current = new mod.PatternGenerator();
      }
      const response = await fetch(modelUrl);
      console.log("[WASM] Fetch response status:", response.status, response.statusText);
      const decompressedText = await response.text();
      console.log("[WASM] Model loaded, size:", decompressedText.length);
      
      generatorRef.current.load_hmm_model(decompressedText);
      console.log("[WASM] Model loaded successfully");
      return true;
    } catch (err) {
      console.error("[WASM] Model loading error:", err);
      setError(err instanceof Error ? err.message : "Failed to load HMM model");
      return false;
    }
  }, [isReady]);

  const hasHMMModel = useCallback((): boolean => {
    if (!generatorRef.current) return false;
    return generatorRef.current.has_hmm_model();
  }, []);

  return {
    isReady,
    loadingProgress,
    error,
    analyzeAudio,
    generatePatterns,
    loadHMMModel,
    hasHMMModel,
  };
}
