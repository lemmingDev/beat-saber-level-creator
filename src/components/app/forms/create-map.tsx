import type { UseDialogContext } from "@ark-ui/react/dialog";
import { useStore } from "@tanstack/react-form";
import { type CharacteristicName, CharacteristicNameSchema, type DifficultyName, DifficultyNameSchema, EnvironmentName, type EnvironmentV2Name, type EnvironmentV3Name } from "bsmap";
import { useState } from "react";
import { array, endsWith, file, type GenericSchema, gtValue, length, maxLength, minLength, minValue, number, object, pipe, string, transform } from "valibot";

import { CHARACTERISTIC_COLLECTION, COVER_ART_FILE_ACCEPT_TYPE, DIFFICULTY_COLLECTION, ENVIRONMENT_COLLECTION, SONG_FILE_ACCEPT_TYPE } from "$/components/app/constants";
import { useAudioContext, useToaster } from "$/components/context";
import { Show } from "$/components/ui/atoms";
import { Audio, Button, Switch, useAppForm } from "$/components/ui/compositions";
import { createPlaceholderImageFile, remuxImageToSquare } from "$/helpers/file.helpers";
import { decodeAudioData } from "$/helpers/audio.helpers";
import { createSongId, resolveBeatmapId } from "$/helpers/song.helpers";
import { addSong } from "$/store/actions";
import { useWasmAudio } from "$/hooks/useWasmAudio";
import { useAppDispatch, useAppSelector } from "$/store/hooks";
import { selectSongIds, selectSongVolume, selectUsername } from "$/store/selectors";

const SCHEMA = object({
	songFile: pipe(array(file()), length(1)),
	coverArtFile: pipe(array(file()), maxLength(1)),
	name: pipe(string(), minLength(1)),
	subName: pipe(string()),
	artistName: pipe(string()),
	bpm: pipe(number(), gtValue(0)),
	offset: pipe(
		number(),
		minValue(0),
		transform((input) => (Number.isNaN(input) ? undefined : input)),
	),
	previewStartTime: pipe(number(), minValue(0)),
	previewDuration: pipe(number(), minValue(0)),
	environment: pipe(string(), endsWith("Environment")) as GenericSchema<EnvironmentV2Name | EnvironmentV3Name>,
	characteristic: CharacteristicNameSchema,
	difficulty: DifficultyNameSchema,
});

interface Props {
	dialog?: UseDialogContext;
}
function CreateMapForm({ dialog }: Props) {
	const dispatch = useAppDispatch();
	const currentSongIds = useAppSelector(selectSongIds);
	const username = useAppSelector(selectUsername);
	const volume = useAppSelector(selectSongVolume);
	const audioContext = useAudioContext();

	const toaster = useToaster();

	const { isReady, analyzeAudio } = useWasmAudio();
	const [isDetectingBpm, setIsDetectingBpm] = useState(false);
	const [bpmConfidence, setBpmConfidence] = useState<number | null>(null);
	const [enableBpmSnapping, setEnableBpmSnapping] = useState(true);

	const detectBpm = async (songFile: File) => {
		if (!isReady || !songFile) return;
		setIsDetectingBpm(true);
		setBpmConfidence(null);
		try {
			const audioBuffer = await decodeAudioData(songFile, audioContext);

			// Analyze middle 80% of the song for BPM (ignores intro/outro/silence)
			const startSample = Math.floor(audioBuffer.length * 0.1);
			const endSample = Math.floor(audioBuffer.length * 0.9);
			const middleBuffer = audioContext.createBuffer(
				audioBuffer.numberOfChannels,
				endSample - startSample,
				audioBuffer.sampleRate
			);
			for (let ch = 0; ch < audioBuffer.numberOfChannels; ch++) {
				const src = audioBuffer.getChannelData(ch);
				const dst = middleBuffer.getChannelData(ch);
				for (let i = 0; i < dst.length; i++) {
					dst[i] = src[startSample + i];
				}
			}

			const analysis = await analyzeAudio(middleBuffer);
			if (analysis && analysis.tempo) {
				let detectedBpm = analysis.tempo;
				// Snap to nearest integer if enabled and very close (most songs are at integer BPMs)
				if (enableBpmSnapping) {
					const nearestInt = Math.round(detectedBpm);
					if (Math.abs(detectedBpm - nearestInt) < 0.3) {
						detectedBpm = nearestInt;
					}
				}
				Form.setFieldValue("bpm", Math.round(detectedBpm * 10) / 10);
				setBpmConfidence(analysis.tempo_confidence ?? null);

				// Detect lead-in from full audio (first significant onset)
				const fullAnalysis = await analyzeAudio(audioBuffer);
				const leadIn = fullAnalysis ? detectLeadIn(fullAnalysis) : null;
				if (leadIn !== null) {
					Form.setFieldValue("offset", Math.round(leadIn * 100) / 100);
				}

				toaster.success({ description: `Detected BPM: ${detectedBpm.toFixed(1)} (confidence: ${((analysis.tempo_confidence ?? 0) * 100).toFixed(0)}%)${leadIn !== null ? `, Lead-in: ${leadIn.toFixed(2)}s` : ""}` });
			} else {
				toaster.error({ description: "Could not detect BPM from audio" });
			}
		} catch (err) {
			toaster.error({ description: `BPM detection failed: ${err instanceof Error ? err.message : "Unknown error"}` });
		} finally {
			setIsDetectingBpm(false);
		}
	};

	const detectLeadIn = (analysis: any): number | null => {
		// Find the first onset with significant strength (> 0.3)
		const onsets = analysis.onsets || [];
		if (onsets.length === 0) return null;

		const significantOnsets = onsets.filter((o: any) => o.strength > 0.3);
		if (significantOnsets.length === 0) return null;

		const firstOnset = significantOnsets[0];
		// If first onset is more than 0.5s into the song, there is lead-in
		if (firstOnset.time > 0.5) {
			return firstOnset.time;
		}
		return null;
	};

	const Form = useAppForm({
		defaultValues: {
			songFile: [] as File[],
			coverArtFile: [] as File[],
			name: "",
			subName: "",
			artistName: "",
			bpm: 0,
			offset: 0,
			previewStartTime: 12,
			previewDuration: 10,
			environment: EnvironmentName[0] as EnvironmentV2Name | EnvironmentV3Name,
			characteristic: "Standard" as CharacteristicName,
			difficulty: "Easy" as DifficultyName,
		},
		validators: {
			onMount: SCHEMA,
			onChange: SCHEMA,
			onSubmit: SCHEMA,
		},
		onSubmit: async ({ value }) => {
			try {
				const songId = createSongId(value, currentSongIds);
				const beatmapId = resolveBeatmapId({ characteristic: value.characteristic, difficulty: value.difficulty });

				const songFile = value.songFile[0];
				const coverArtFile = value.coverArtFile[0] ?? (await createPlaceholderImageFile());

				const mappers = username !== "" ? [username] : [];

				dispatch(
					addSong({
						songId,
						beatmapId,
						songFile,
						coverArtFile,
						songData: {
							id: songId,
							name: value.name,
							subName: value.subName,
							artistName: value.artistName,
							bpm: value.bpm,
							offset: value.offset ?? 0,
							previewStartTime: value.previewStartTime,
							previewDuration: value.previewDuration,
							environment: value.environment,
							songFilename: songFile.name,
							coverArtFilename: coverArtFile.name,
						},
						beatmapData: {
							characteristic: value.characteristic,
							difficulty: value.difficulty,
							mappers: mappers,
							lighters: mappers,
						},
					}),
				);

				if (dialog) dialog.setOpen(false);
			} catch (error) {
				toaster.error({ description: `Could not create map: ${error instanceof Error ? error.message : "See console for more information."}` });
				return console.error(error);
			}
		},
	});
	const songFile = useStore(Form.store, (state) => state.values.songFile);

	const previewStartTime = useStore(Form.store, (state) => state.values.previewStartTime);
	const previewDuration = useStore(Form.store, (state) => state.values.previewDuration);

	const [showOptionalFields, setShowOptionalFields] = useState(false);

	return (
		<Form.AppForm>
			<Form.Root>
				<Switch label="Show Optional Fields" checked={showOptionalFields} onCheckedChange={(x) => setShowOptionalFields(!!x.checked)} />
				<Show when={!showOptionalFields}>
					<Form.AppField name="songFile">
						{(ctx) => (
							<ctx.FileUpload label="Song File" required acceptText="Audio File" accept={SONG_FILE_ACCEPT_TYPE} maxFiles={1}>
								{(file) => <Audio file={file} startTime={previewStartTime} duration={previewDuration} volume={volume} />}
							</ctx.FileUpload>
						)}
					</Form.AppField>

					<Form.Row>
						<Form.AppField name="name">{(ctx) => <ctx.Input label="Song Title" required />}</Form.AppField>
						<Form.AppField name="bpm">{(ctx) => <ctx.NumberInput label="BPM (Beats per Minute)" min={0} required />}</Form.AppField>
					</Form.Row>
					<Form.Row>
						<Button variant="subtle" size="sm" loading={isDetectingBpm} disabled={!isReady || !(songFile?.[0])} onClick={() => {
							const file = songFile?.[0];
							if (file) detectBpm(file);
						}}>
							{isDetectingBpm ? "Detecting..." : "Detect BPM from Audio"}
						</Button>
						<label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--fg-muted)" }}>
							<input
								type="checkbox"
								checked={enableBpmSnapping}
								onChange={(e) => setEnableBpmSnapping(e.target.checked)}
							/>
							Snap to integer BPM
						</label>
						{bpmConfidence !== null && (
							<span style={{ fontSize: 12, color: "var(--fg-muted)" }}>
								Confidence: {(bpmConfidence * 100).toFixed(0)}%
							</span>
						)}
					</Form.Row>
				</Show>
				<Show when={showOptionalFields}>
					<Form.Row>
						<Form.AppField name="songFile">
							{(ctx) => (
								<ctx.FileUpload label="Song File" required acceptText="Audio File" accept={SONG_FILE_ACCEPT_TYPE} maxFiles={1}>
									{(file) => <Audio file={file} startTime={previewStartTime} duration={previewDuration} volume={volume} />}
								</ctx.FileUpload>
							)}
						</Form.AppField>
						<Form.AppField name="coverArtFile">
							{(ctx) => (
								<ctx.FileUpload label="Cover Art File" acceptText="Image File" accept={COVER_ART_FILE_ACCEPT_TYPE} maxFiles={1} transformFiles={(files) => Promise.all(files.map((file) => remuxImageToSquare(file)))}>
									{() => null}
								</ctx.FileUpload>
							)}
						</Form.AppField>
					</Form.Row>

					<Form.Row>
						<Form.AppField name="name">{(ctx) => <ctx.Input label="Song Title" required />}</Form.AppField>
						<Form.AppField name="subName">{(ctx) => <ctx.Input label="Song Subtitle" />}</Form.AppField>
						<Form.AppField name="artistName">{(ctx) => <ctx.Input label="Song Artist(s)" />}</Form.AppField>
					</Form.Row>
					<Form.Row>
						<Form.AppField name="bpm">{(ctx) => <ctx.NumberInput label="BPM (Beats per Minute)" required min={0} />}</Form.AppField>
						<Button variant="subtle" size="sm" loading={isDetectingBpm} disabled={!isReady || !(songFile?.[0])} onClick={() => {
							const file = songFile?.[0];
							if (file) detectBpm(file);
						}}>
							{isDetectingBpm ? "Detecting..." : "Detect BPM"}
						</Button>
						<label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--fg-muted)" }}>
							<input
								type="checkbox"
								checked={enableBpmSnapping}
								onChange={(e) => setEnableBpmSnapping(e.target.checked)}
							/>
							Snap
						</label>
						<Form.AppField name="offset">{(ctx) => <ctx.NumberInput label="Editor Offset" required />}</Form.AppField>
						<Form.AppField name="previewStartTime">{(ctx) => <ctx.NumberInput label="Preview start time" required min={0} />}</Form.AppField>
						<Form.AppField name="previewDuration">{(ctx) => <ctx.NumberInput label="Preview duration" required min={0} />}</Form.AppField>
					</Form.Row>
					<Form.Row>
						<Form.AppField name="environment">{(ctx) => <ctx.Combobox label="Base Environment" required creatable collection={ENVIRONMENT_COLLECTION} />}</Form.AppField>
					</Form.Row>
				</Show>
				<Form.AppField name="characteristic">{(ctx) => <ctx.RadioButtonGroup label="Beatmap Characteristic" required collection={CHARACTERISTIC_COLLECTION} />}</Form.AppField>
				<Form.AppField name="difficulty">{(ctx) => <ctx.RadioButtonGroup label="Beatmap Difficulty" required collection={DIFFICULTY_COLLECTION} />}</Form.AppField>
				<Form.Submit>Create new map</Form.Submit>
			</Form.Root>
		</Form.AppForm>
	);
}

export default CreateMapForm;
