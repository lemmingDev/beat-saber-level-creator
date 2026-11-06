import { useState, useCallback } from "react";
import { CreateMapForm, ImportMapForm } from "$/components/app/forms";
import { SongsDataTable } from "$/components/app/templates/tables";
import { Button, Dialog, Heading } from "$/components/ui/compositions";
import { addSongFromFile } from "$/store/actions";
import { useNavigate } from "@tanstack/react-router";
import { useAppDispatch, useAppSelector } from "$/store/hooks";
import { selectSongIds } from "$/store/selectors";
import { styled } from "$:styled-system/jsx";
import { stack, vstack } from "$:styled-system/patterns";

const SAMPLE_MAPS = [
	{ name: "Beat It (MJ)", url: "/maps/beat_it_mj.zip" },
	{ name: "Billie Jean (MJ)", url: "/maps/billie_jean.zip" },
	{ name: "Bohemian Like You", url: "/maps/bohemian_like_you.zip" },
	{ name: "CALAMITY RHAPSODY", url: "/maps/calamity_rhapsody.zip" },
];

function ReturningHome() {
	const dispatch = useAppDispatch();
	const songIds = useAppSelector(selectSongIds);
	const [loadingMap, setLoadingMap] = useState<string | null>(null);
	const navigate = useNavigate();

	const loadSampleMap = useCallback(async (map: { name: string; url: string }) => {
		setLoadingMap(map.name);
		try {
			const file = await fetch(map.url).then((response) => response.blob());
			const { songId, beatmapId } = await dispatch(addSongFromFile({ file, options: { currentSongIds: songIds } })).unwrap();
			navigate({ to: "/edit/$sid/$bid/notes", params: { sid: songId.toString(), bid: beatmapId.toString() } });
		} catch (err) {
			console.error("Failed to load sample map:", JSON.stringify(err), err);
		} finally {
			setLoadingMap(null);
		}
	}, [dispatch, songIds, navigate]);

	return (
		<Wrapper>
			<Heading rank={1}>Select map to edit</Heading>
			<Contents>
				<MainColumn>
					<SongsDataTable />
				</MainColumn>
				<SideColumn>
					<Dialog title="Create new map" description="Build a new map from scratch, using music from your computer" unmountOnExit render={(ctx) => <CreateMapForm dialog={ctx} />}>
						<Button variant="solid" size="md">
							Create new map
						</Button>
					</Dialog>
					{/** biome-ignore lint/suspicious/useIterableCallbackReturn: doesn't matter */}
					<Dialog title="Import existing map" description="Edit an existing map by selecting it from your computer" unmountOnExit render={(ctx) => <ImportMapForm dialog={ctx} onAccept={(files) => files.forEach((file) => void dispatch(addSongFromFile({ file, options: { currentSongIds: songIds } })))} />}>
						<Button variant="solid" size="md">
							Import existing map
						</Button>
					</Dialog>
					<Heading rank={3}>Sample Maps</Heading>
					{SAMPLE_MAPS.map((map) => (
						<Button key={map.name} variant="subtle" size="sm" loading={loadingMap === map.name} onClick={() => loadSampleMap(map)}>
							{map.name}
						</Button>
					))}
				</SideColumn>
			</Contents>
		</Wrapper>
	);
}

const Wrapper = styled("div", {
	base: stack.raw({
		gap: 4,
		align: { base: "center", md: "flex-start" },
	}),
});

const Contents = styled("div", {
	base: stack.raw({
		width: "100%",
		direction: { base: "column-reverse", md: "row" },
		align: { base: "center", md: "flex-start" },
		gap: 4,
	}),
});

const MainColumn = styled("div", {
	base: stack.raw({
		gap: 2,
		width: "100%",
	}),
});

const SideColumn = styled("div", {
	base: vstack.raw({
		colorPalette: "slate",
		layerStyle: "fill.surface",
		gap: 2,
		padding: 4,
		borderRadius: "sm",
		minWidth: "280px",
	}),
});

export default ReturningHome;
