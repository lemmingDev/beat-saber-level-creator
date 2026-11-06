import { useParams } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ActionPanelGroup } from "$/components/app/layouts";
import { Show } from "$/components/ui/atoms";
import { Button, Tooltip, usePrompt, Select } from "$/components/ui/compositions";
import { createListCollection } from "@ark-ui/react/collection";
import { useAppDispatch, useAppSelector } from "$/store/hooks";
import { useAudioContext, useBeatmapFilestore } from "$/components/context";
import { addColorNote, jumpToBeat } from "$/store/actions";
import { selectModuleEnabled, selectCursorPositionInBeats, selectSelectedBeatmap, selectBpm } from "$/store/selectors";
import { decodeAudioData } from "$/helpers/audio.helpers";
import { useWasmAudio } from "$/hooks/useWasmAudio";

type PatternGenerationMethod = "heuristic" | "local-ml" | "server-ml";
type PatternGenerationOptions = {
	method: PatternGenerationMethod;
	useAudioAnalysis: boolean;
	followSegments: boolean;
	targetDifficulty: "easy" | "normal" | "hard" | "expert" | "expert+";
	adaptToMusic: boolean;
	seed?: number;
	sectionLength: "auto" | "4beats" | "8beats" | "16beats" | "32beats";
};

function AiSuggestActionPanelGroup() {
	const { sid } = useParams({ from: "/_/edit/$sid/$bid/_" });

	const dispatch = useAppDispatch();
	const audioContext = useAudioContext();
	const filestore = useBeatmapFilestore();
	const cursorPositionInBeats = useAppSelector((state) => selectCursorPositionInBeats(state, sid));
	const selectedBeatmap = useAppSelector((state) => selectSelectedBeatmap(state, sid));
	const bpm = useAppSelector((state) => selectBpm(state, sid));
	const mappingExtensionsEnabled = useAppSelector((state) => selectModuleEnabled(state, sid, "mappingExtensions"));

	const { isReady, loadingProgress, error, generatePatterns, analyzeAudio, loadHMMModel, hasHMMModel } = useWasmAudio();
	const [isGenerating, setIsGenerating] = useState(false);
	const [lastError, setLastError] = useState<string | null>(null);
	
	// Pattern generation options
	// Load HMM model when method changes to local-ml
	const [hmmLoaded, setHmmLoaded] = useState(false);
	const [hmmLoading, setHmmLoading] = useState(false);
	const [hmmError, setHmmError] = useState<string | null>(null);
	const loadAttemptedRef = useRef(false);

	const loadModel = useCallback(async (difficulty: string) => {
		if (loadAttemptedRef.current) return;
		loadAttemptedRef.current = true;
		setHmmLoading(true);
		setHmmError(null);
		const modelUrl = `/models/HMM_${difficulty}_v1.json`;
		const success = await loadHMMModel(modelUrl);
		setHmmLoaded(success);
		if (!success) {
			setHmmError("Failed to load model");
		}
		setHmmLoading(false);
	}, [loadHMMModel]);

	const [options, setOptions] = useState<PatternGenerationOptions>({
		method: "heuristic",
		useAudioAnalysis: true,
		followSegments: true,
		targetDifficulty: "normal",
		adaptToMusic: true,
		sectionLength: "auto",
	});

	const { trigger: triggerBeatPrompt } = usePrompt({
		render: ({ form }) => (
			<form.AppField name="beatNum">
				{(ctx) => <ctx.NumberInput autoFocus label="Beat" placeholder={cursorPositionInBeats.toFixed(2)} />}
			</form.AppField>
		),
		onSubmit: ({ value: { beatNum } }) => dispatch(jumpToBeat({ value: beatNum })),
	});

	const handleGenerate = useCallback(async () => {
		if (!isReady || isGenerating) return;
		
		setIsGenerating(true);
		setLastError(null);

		// Convert beats to seconds for the WASM module
		const beatsPerSecond = bpm / 60;
		const startTimeSeconds = cursorPositionInBeats / beatsPerSecond;
		
		// Convert section length to beats
		let sectionBeats = 4;
		if (options.sectionLength !== "auto") {
			sectionBeats = parseInt(options.sectionLength);
		}
		
		const durationSeconds = sectionBeats / beatsPerSecond;

		// Load and analyze audio if enabled
		let audioAnalysis = null;
		if (options.useAudioAnalysis) {
			try {
				const songFile = await filestore.loadSongFile(sid);
				const audioBuffer = await decodeAudioData(songFile, audioContext);
				audioAnalysis = await analyzeAudio(audioBuffer);
			} catch (err) {
				console.warn("Audio analysis failed, using heuristic only:", err);
			}
		}

		// Scale NPS based on difficulty (based on real Beat Saber map analysis)
		const difficultyNps: Record<string, number> = {
			easy: 0.6,
			normal: 1.1,
			hard: 1.5,
			expert: 2.0,
			"expert+": 3.5,
		};
		const targetNps = difficultyNps[options.targetDifficulty] || 1.1;

		const request = {
			start_time: startTimeSeconds,
			end_time: startTimeSeconds + durationSeconds,
			target_nps: targetNps,
			difficulty: 0.5,
			styles: ["Alternating", "Flow", "CrossBody", "OneHanded"],
			constraints: {
				min_note_spacing: 0.125,
				max_nps: 10.0,
				allowed_types: ["Red", "Blue"],
				forbidden_lanes: [],
				symmetric: false,
			},
			section_beats: sectionBeats,
			repeat_for_segments: options.followSegments,
			seed: options.seed,
			generation_method: options.method,
			audio_analysis: audioAnalysis ? JSON.stringify(audioAnalysis) : undefined,
		};

		try {
			const suggestions = await generatePatterns(request);
			
			if (!suggestions || suggestions.length === 0) {
				setLastError("No patterns generated");
				return;
			}

			const best = suggestions[0];
			
			// Convert pattern notes (in seconds) to beatmap notes (in beats)
			const beatsPerSecond = bpm / 60;
			const notesToAdd = best.notes.map((pn) => {
				const beatTime = pn.time * beatsPerSecond;
				return {
					time: beatTime,
					posX: pn.lane_x,
					posY: pn.lane_y,
					direction: pn.cut_direction,
					color: pn.note_type === "Red" ? 0 : pn.note_type === "Blue" ? 1 : -1,
					angleOffset: pn.angle_offset ?? 0,
					customData: {},
				};
			});

			// Dispatch each note
			for (const note of notesToAdd) {
				if (note.color === 0) {
					dispatch(addColorNote({ ...note, color: 0 } as any));
				} else if (note.color === 1) {
					dispatch(addColorNote({ ...note, color: 1 } as any));
				}
			}
		} catch (err) {
			setLastError(err instanceof Error ? err.message : "Failed to generate patterns");
		} finally {
			setIsGenerating(false);
		}
	}, [isReady, isGenerating, cursorPositionInBeats, bpm, generatePatterns, dispatch, options]);

	const handleMapEntireSong = useCallback(async () => {
		if (!isReady || isGenerating) return;
		
		setIsGenerating(true);
		setLastError(null);

		const beatsPerSecond = bpm / 60;
		
		// Try to get actual song duration and audio analysis from audio
		let songDuration = 120; // fallback
		let audioAnalysis = null;
		try {
			const songFile = await filestore.loadSongFile(sid);
			const audioBuffer = await decodeAudioData(songFile, audioContext);
			songDuration = audioBuffer.duration;
			if (options.useAudioAnalysis) {
				audioAnalysis = await analyzeAudio(audioBuffer);
			}
		} catch (err) {
			console.warn("Could not get song duration, using fallback:", err);
		}
		
		const totalBeats = songDuration * beatsPerSecond;
		
		// Scale NPS based on difficulty (based on real Beat Saber map analysis)
		const difficultyNps: Record<string, number> = {
			easy: 0.6,
			normal: 1.1,
			hard: 1.5,
			expert: 2.0,
			"expert+": 3.5,
		};
		const targetNps = difficultyNps[options.targetDifficulty] || 1.1;

		// Process in chunks to avoid memory issues
		const chunkSize = 16; // 16 beats per chunk
		const numChunks = Math.ceil(totalBeats / chunkSize);

		try {
			const styles = ["Alternating", "Flow", "CrossBody"];
			for (let chunk = 0; chunk < numChunks; chunk++) {
				const startBeat = chunk * chunkSize;
				const endBeat = Math.min(startBeat + chunkSize, totalBeats);
				const startTimeSeconds = startBeat / beatsPerSecond;
				const endTimeSeconds = endBeat / beatsPerSecond;

				// Rotate styles per chunk for variety
				const chunkStyle = styles[chunk % styles.length];

				const request = {
					start_time: startTimeSeconds,
					end_time: endTimeSeconds,
					target_nps: targetNps,
					difficulty: 0.5,
					styles: [chunkStyle],
					constraints: {
						min_note_spacing: 0.125,
						max_nps: 10.0,
						allowed_types: ["Red", "Blue"],
						forbidden_lanes: [],
						symmetric: false,
					},
					section_beats: chunkSize,
					repeat_for_segments: options.followSegments,
					seed: options.seed ? options.seed + chunk : undefined,
					generation_method: options.method,
					audio_analysis: audioAnalysis ? JSON.stringify(audioAnalysis) : undefined,
				};

				const suggestions = await generatePatterns(request);
				if (!suggestions || suggestions.length === 0) continue;

				const best = suggestions[0];
				const notesToAdd = best.notes.map((pn: any) => {
					const beatTime = pn.time * beatsPerSecond;
					return {
						time: beatTime,
						posX: pn.lane_x,
						posY: pn.lane_y,
						direction: pn.cut_direction,
						color: pn.note_type === "Red" ? 0 : pn.note_type === "Blue" ? 1 : -1,
						angleOffset: pn.angle_offset ?? 0,
						customData: {},
					};
				});

				for (const note of notesToAdd) {
					if (note.color === 0) {
						dispatch(addColorNote({ ...note, color: 0 } as any));
					} else if (note.color === 1) {
						dispatch(addColorNote({ ...note, color: 1 } as any));
					}
				}
			}
		} catch (err) {
			setLastError(err instanceof Error ? err.message : "Failed to map entire song");
		} finally {
			setIsGenerating(false);
		}
	}, [isReady, isGenerating, bpm, generatePatterns, dispatch, options]);

	useEffect(() => {
		if (options.method === "local-ml" && !hmmLoaded && !hmmLoading && !loadAttemptedRef.current) {
			loadModel(options.targetDifficulty);
		}
	}, [options.method, options.targetDifficulty, hmmLoaded, hmmLoading]);

	const handleOptionChange = (key: keyof PatternGenerationOptions, value: any) => {
		if (key === "method") {
			loadAttemptedRef.current = false;
			setHmmLoaded(false);
			setHmmError(null);
		}
		setOptions(prev => ({ ...prev, [key]: value }));
	};

	const difficultyOptions = useMemo(() => [
		{ value: "easy", label: "Easy" },
		{ value: "normal", label: "Normal" },
		{ value: "hard", label: "Hard" },
		{ value: "expert", label: "Expert" },
		{ value: "expert+", label: "Expert+" },
	], []);

	const sectionLengthOptions = useMemo(() => [
		{ value: "auto", label: "Auto (4 beats)" },
		{ value: "4beats", label: "4 beats" },
		{ value: "8beats", label: "8 beats" },
		{ value: "16beats", label: "16 beats" },
		{ value: "32beats", label: "32 beats" },
	], []);

	const methodOptions = useMemo(() => [
		{ value: "heuristic", label: "Heuristic (Instant)", description: "Rule-based patterns, instant" },
		{ value: "local-ml", label: "Local ML (WASM)", description: "HMM model, runs in browser" },
		{ value: "server-ml", label: "Server ML (Best)", description: "Requires server, best quality" },
	], []);

	const methodCollection = useMemo(() => 
		createListCollection({
			items: methodOptions.map(m => m.value),
			itemToString: (v) => {
				const opt = methodOptions.find(m => m.value === v);
				return opt ? `${opt.label} — ${opt.description}` : v;
			},
		}), []);

	const sectionLengthCollection = useMemo(() =>
		createListCollection({
			items: ["auto", "4beats", "8beats", "16beats", "32beats"],
			itemToString: (v) => {
				const opt = ["auto", "4beats", "8beats", "16beats", "32beats"].find(o => o === v);
				const labels: Record<string, string> = {
					auto: "Auto (4 beats)",
					"4beats": "4 beats",
					"8beats": "8 beats",
					"16beats": "16 beats",
					"32beats": "32 beats",
				};
				return labels[v] || v;
			},
		}), []);

	const difficultyCollection = useMemo(() =>
		createListCollection({
			items: ["easy", "normal", "hard", "expert", "expert+"],
			itemToString: (v) => v.charAt(0).toUpperCase() + v.slice(1),
		}), []);


	return (
		<Show when={mappingExtensionsEnabled}>
			<ActionPanelGroup.Root label="AI Suggest">
				<ActionPanelGroup.ActionGroup>
				 {!isReady ? (
					<Button variant="subtle" size="sm" disabled>
						Loading WASM... {loadingProgress > 0 && <span>{loadingProgress}%</span>}
					</Button>
				) : options.method === "local-ml" && !hmmLoaded ? (
					<Button variant="subtle" size="sm" disabled>
						{hmmLoading ? "Loading Model..." : "Model not loaded"}
					</Button>
				) : (
					<>
						<Tooltip render={() => "Generate AI pattern suggestions at the playhead"}>
							<Button
								variant={isGenerating ? "subtle" : "solid"}
								size="sm"
								unfocusOnPress
								onClick={handleGenerate}
								disabled={isGenerating}
							>
								{isGenerating ? "Generating..." : "Generate Pattern"}
							</Button>
						</Tooltip>
						<Tooltip render={() => "Jump to a specific beat to generate patterns there"}>
							<Button variant="subtle" size="sm" unfocusOnPress onClick={triggerBeatPrompt}>
								Jump to Beat
							</Button>
						</Tooltip>
						<Tooltip render={() => "Generate patterns for the entire song"}>
							<Button variant="solid" size="sm" unfocusOnPress onClick={handleMapEntireSong} disabled={isGenerating}>
								{isGenerating ? "Mapping..." : "Map Entire Song"}
							</Button>
						</Tooltip>
					</>
				)}
				</ActionPanelGroup.ActionGroup>

				{/* Options Panel */}
				<ActionPanelGroup.ActionGroup style={{ marginTop: 8, paddingTop: 8, borderTop: "1px solid var(--border-color)" }}>
					<Select
						unfocusOnPress
						size="sm"
						collection={methodCollection}
						value={[options.method]}
						onValueChange={({ value }) => handleOptionChange("method", value[0])}
						disabled={isGenerating}
					/>
				</ActionPanelGroup.ActionGroup>

				<ActionPanelGroup.ActionGroup>
					<Select
						unfocusOnPress
						size="sm"
						collection={sectionLengthCollection}
						value={[options.sectionLength]}
						onValueChange={({ value }) => handleOptionChange("sectionLength", value[0])}
						disabled={isGenerating}
					/>
				</ActionPanelGroup.ActionGroup>

				<ActionPanelGroup.ActionGroup>
					<Select
						unfocusOnPress
						size="sm"
						collection={difficultyCollection}
						value={[options.targetDifficulty]}
						onValueChange={({ value }) => handleOptionChange("targetDifficulty", value[0])}
						disabled={isGenerating}
					/>
				</ActionPanelGroup.ActionGroup>

				<ActionPanelGroup.ActionGroup>
					<label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--fg-muted)" }}>
						<input
							type="checkbox"
							checked={options.useAudioAnalysis}
							onChange={(e) => handleOptionChange("useAudioAnalysis", e.target.checked)}
							disabled={isGenerating}
						/>
						Use Audio Analysis
					</label>
				</ActionPanelGroup.ActionGroup>

				<ActionPanelGroup.ActionGroup>
					<label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--fg-muted)" }}>
						<input
							type="checkbox"
							checked={options.followSegments}
							onChange={(e) => handleOptionChange("followSegments", e.target.checked)}
							disabled={isGenerating}
						/>
						Repeat for Segments
					</label>
				</ActionPanelGroup.ActionGroup>

				<ActionPanelGroup.ActionGroup>
					<label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--fg-muted)" }}>
						<input
							type="checkbox"
							checked={options.adaptToMusic}
							onChange={(e) => handleOptionChange("adaptToMusic", e.target.checked)}
							disabled={isGenerating}
						/>
						Adapt to Music Intensity
					</label>
				</ActionPanelGroup.ActionGroup>

				<ActionPanelGroup.ActionGroup>
					<input
						type="number"
						placeholder="Seed (optional)"
						value={options.seed ?? ""}
						onChange={(e) => handleOptionChange("seed", e.target.value ? parseInt(e.target.value) : undefined)}
						style={{ width: "100%", fontSize: 12, padding: 4, borderRadius: 4 }}
						disabled={isGenerating}
					/>
				</ActionPanelGroup.ActionGroup>

				{lastError && (
					<ActionPanelGroup.ActionGroup>
						<Tooltip render={() => lastError} variant="error">
							<Button variant="subtle" size="sm" disabled>
								⚠ Error
							</Button>
						</Tooltip>
					</ActionPanelGroup.ActionGroup>
				)}
				{error && (
					<ActionPanelGroup.ActionGroup>
						<Tooltip render={() => error} variant="error">
							<Button variant="subtle" size="sm" disabled>
								WASM Error
							</Button>
						</Tooltip>
					</ActionPanelGroup.ActionGroup>
				)}
				{hmmError && (
					<ActionPanelGroup.ActionGroup>
						<Tooltip render={() => hmmError} variant="error">
							<Button variant="subtle" size="sm" disabled>
								Model Error
							</Button>
						</Tooltip>
					</ActionPanelGroup.ActionGroup>
				)}
			</ActionPanelGroup.Root>
		</Show>
	);
}

export default AiSuggestActionPanelGroup;
