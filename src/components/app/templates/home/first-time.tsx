import { useNavigate } from "@tanstack/react-router";
import { CirclePlusIcon, DownloadIcon, PackageOpenIcon } from "lucide-react";
import { useCallback, useState } from "react";

import { demoFileUrl, heroVideo } from "$/assets";
import { CreateMapForm, ImportMapForm } from "$/components/app/forms";
import { Button, Dialog, Heading } from "$/components/ui/compositions";
import { addSongFromFile } from "$/store/actions";
import { useAppDispatch, useAppSelector } from "$/store/hooks";
import { selectSongIds } from "$/store/selectors";
import { styled, VStack, Wrap } from "$:styled-system/jsx";
import OptionColumn from "./option";

const SAMPLE_MAPS = [
	{ name: "Beat It (MJ)", url: "/maps/beat_it_mj.zip" },
	{ name: "Billie Jean (MJ)", url: "/maps/billie_jean.zip" },
	{ name: "Bohemian Like You", url: "/maps/bohemian_like_you.zip" },
	{ name: "CALAMITY RHAPSODY", url: "/maps/calamity_rhapsody.zip" },
];

function FirstTimeHome() {
	const dispatch = useAppDispatch();
	const songIds = useAppSelector(selectSongIds);

	const navigate = useNavigate();

	const [isLoadingDemo, setIsLoadingDemo] = useState(false);
	const [loadingSample, setLoadingSample] = useState<string | null>(null);

	const handleDemoClick = useCallback(async () => {
		setIsLoadingDemo(true);
		const file = await fetch(demoFileUrl).then((response) => response.blob());
		const { songId, beatmapId } = await dispatch(addSongFromFile({ file, options: { readonly: true } })).unwrap();
		navigate({ to: "/edit/$sid/$bid/notes", params: { sid: songId.toString(), bid: beatmapId.toString() } });
		setIsLoadingDemo(false);
	}, [dispatch, navigate]);

	const handleLoadSample = useCallback(async (map: { name: string; url: string }) => {
		setLoadingSample(map.name);
		try {
			const file = await fetch(map.url).then((response) => response.blob());
			const { songId, beatmapId } = await dispatch(addSongFromFile({ file, options: { readonly: true } })).unwrap();
			navigate({ to: "/edit/$sid/$bid/notes", params: { sid: songId.toString(), bid: beatmapId.toString() } });
		} catch (err) {
			console.error("Failed to load sample map:", JSON.stringify(err), err);
		} finally {
			setLoadingSample(null);
		}
	}, [dispatch, navigate]);

	return (
		<VStack gap={8}>
			<Title rank={1}>Beatmapper is a web-based level editor for Beat Saber™</Title>
			<VStack gap={6}>
				<video src={heroVideo} autoPlay muted loop controls />
			</VStack>
			<VStack gap={6}>
				<Heading rank={2}>Get started now</Heading>
				<Wrap gap={4}>
					<OptionColumn icon={PackageOpenIcon} title="Try a demo map" description="Take the editor for a test-drive with some surprisingly good public-domain dubstep">
						<Button variant="solid" size="md" loading={isLoadingDemo} onClick={handleDemoClick}>
							Start mapping
						</Button>
					</OptionColumn>
					<OptionColumn icon={CirclePlusIcon} title="Create new map" description="Build a new map from scratch, using music from your computer">
						<Dialog title="Create new map" description="Build a new map from scratch, using music from your computer" unmountOnExit render={(ctx) => <CreateMapForm dialog={ctx} />}>
							<Button variant="solid" size="md">
								Create from scratch
							</Button>
						</Dialog>
					</OptionColumn>
					<OptionColumn icon={DownloadIcon} title="Import existing map" description="Edit an existing map by selecting it from your computer">
						<Dialog title="Import existing map" description="Edit an existing map by selecting it from your computer" unmountOnExit render={(ctx) => <ImportMapForm dialog={ctx} onAccept={(files) => files.forEach((file) => void dispatch(addSongFromFile({ file, options: { currentSongIds: songIds } })))} />}>
							<Button variant="solid" size="md">
								Import map
							</Button>
						</Dialog>
					</OptionColumn>
				</Wrap>
				<Heading rank={3}>Or try a sample map</Heading>
				<Wrap gap={4}>
					{SAMPLE_MAPS.map((map) => (
						<Button key={map.name} variant="subtle" size="md" loading={loadingSample === map.name} onClick={() => handleLoadSample(map)}>
							{map.name}
						</Button>
					))}
				</Wrap>
			</VStack>
		</VStack>
	);
}

const Title = styled(Heading, {
	base: {
		fontWeight: "normal",
		whiteSpace: "wrap",
		textAlign: "center",
	},
});

export default FirstTimeHome;
