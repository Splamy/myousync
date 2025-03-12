<script lang="ts">
	import {
		API_URL,
		FetchStatus,
		type BrainzMetadata,
		type BrainzMultiSearch,
		type VideoData,
	} from "./defs";
	import {
		Button,
		Collapse,
		Icon,
		Card,
		Notification,
		Tooltip,
		Popover,
		Toggle,
		Dialog,
	} from "svelte-ux";
	import { mdiAlertOctagonOutline, mdiTrashCan, mdiReload } from "@mdi/js";

	import Bms from "./BMS.svelte";
	import BRes from "./BRes.svelte";
	import { state_to_color, state_to_icon, UiState } from "$lib";
	import { crossfade, fade, fly } from "svelte/transition";
	import { AUTH } from "./auth";
	import { get } from "svelte/store";

	const NO_LOCAL_FILE = [FetchStatus.FETCH_ERROR, FetchStatus.DISABLED];

	let { video }: { video: VideoData } = $props();

	let state_icon = $derived(state_to_icon(video.fetch_status));
	let state_color = $derived(state_to_color(video.fetch_status));

	let override_query: BrainzMultiSearch = $state(
		video.override_query ?? {
			title: "",
		},
	);
	let override_result: BrainzMetadata = $state(
		video.override_result ?? {
			title: "",
			artist: [],
		},
	);

	let display_text = $derived.by(() => {
		if (video.override_result) {
			return `OResult: ${video.override_result.title} - ${video.override_result.artist.join(
				"; ",
			)}`;
		} else if (video.last_result) {
			return `Result: ${video.last_result.title} - ${video.last_result.artist.join(
				"; ",
			)}`;
		} else if (video.override_query) {
			return `OQuery: ${video.override_query.title} - ${video.override_query.artist}`;
		} else if (video.last_query) {
			return `Query: ${video.last_query.title} - ${video.last_query.artist}`;
		} else {
			return video.video_id;
		}
	});

	function getAuth() {
		return get(AUTH.jwt);
	}

	function copyQuery() {
		if (video.last_query) {
			override_query = { ...video.last_query };
		}
	}

	function copyResult() {
		if (video.last_result) {
			override_result = { ...video.last_result };
		}
	}

	function clearQuery() {
		override_query = { title: "" };
	}

	function clearResult() {
		override_result = { title: "", artist: [] };
	}

	async function overrideQuery() {
		let body;
		if (!override_query.title && !override_query.trackid) {
			body = JSON.stringify(null);
		} else {
			body = JSON.stringify(override_query);
		}
		let res = await authFetch(
			`${API_URL}/video/${video.video_id}/query`,
			body,
		);
		if (res.ok) {
			video.override_query = override_query;
		}
	}

	async function overrideResult() {
		let body;
		if (!override_result.title && !override_result.brainz_recording_id) {
			body = JSON.stringify(null);
		} else {
			body = JSON.stringify(override_result);
		}
		let res = await authFetch(
			`${API_URL}/video/${video.video_id}/result`,
			body,
		);
		if (res.ok) {
			video.override_result = override_result;
		}
	}

	async function retryFetch() {
		await authFetch(`${API_URL}/video/${video.video_id}/retry_fetch`);
	}

	async function deleteVideo() {
		await authFetch(`${API_URL}/video/${video.video_id}/delete`);
	}

	async function authFetch(url: string, body?: BodyInit) {
		return await fetch(url, {
			method: "POST",
			mode: "cors",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${getAuth()}`,
			},
			body,
		});
	}

	let copyPopover = $state(false);
	let copyPopoverTimeout: NodeJS.Timeout;
	function copyIdToClipboard(e: Event) {
		e.stopPropagation();
		let yturl = `https://www.youtube.com/watch?v=${video.video_id}`;
		try {
			navigator.clipboard.writeText(yturl);
		} catch (e) {
			console.error(e);
		}
		copyPopover = true;
		clearTimeout(copyPopoverTimeout);
		copyPopoverTimeout = setTimeout(() => {
			copyPopover = false;
		}, 1000);
	}

	function handle_volume_change(e: Event) {
		UiState.volume = (e.target as HTMLAudioElement).volume;
	}
</script>

<div class="px-3">
	<Card>
		<Collapse>
			<div slot="trigger" class="flex-1 px-3 py-3">
				<Tooltip title={video.fetch_status} placement="top" delay={100}>
					<Icon data={state_icon} style={"color:" + state_color} />
				</Tooltip>
				<div class="inline-block">
					<Popover bind:open={copyPopover} placement="top"
						><div transition:fly={{ y: 10 }}>Copied!</div></Popover
					>
					<Button
						class="font-mono"
						color="accent"
						variant="outline"
						on:click={copyIdToClipboard}
					>
						{video.video_id}</Button
					>
				</div>
				<span class="ml-1 text-center">
					{display_text}
				</span>
			</div>
			<div class="p-3 border-t">
				{#if video.last_error}
					<Notification
						title={video.last_error}
						icon={mdiAlertOctagonOutline}
						color="danger"
						variant="fill"
					/>
				{/if}
				<div class="flex gap-3">
					<div>
						<audio
							controls
							volume={UiState.volume}
							onvolumechange={handle_volume_change}
						>
							<source
								src={`${API_URL}/video/${video.video_id}/preview`}
								type="audio/mpeg"
							/>
							Your browser does not support the audio element.
						</audio>
					</div>

					<div class="flex-1"></div>

					{#if !NO_LOCAL_FILE.includes(video.fetch_status)}
						<Toggle let:on={open} let:toggle let:toggleOff>
							<Button
								icon={mdiTrashCan}
								on:click={toggle}
								variant="outline"
								color="danger">Delete</Button
							>
							<Dialog {open} on:close={toggleOff}>
								<div slot="title">Delete {video.video_id}</div>
								<div class="px-6 py-3">
									Delete and disable Video
								</div>
								<div slot="actions">
									<Button
										on:click={deleteVideo}
										variant="fill"
										color="danger"
									>
										Yes, delete item
									</Button>
									<Button>Cancel</Button>
								</div>
							</Dialog>
						</Toggle>
					{/if}

					{#if NO_LOCAL_FILE.includes(video.fetch_status)}
						<Button
							icon={mdiReload}
							on:click={retryFetch}
							variant="outline"
							color="secondary">Retry</Button
						>
					{/if}
				</div>
				{#if !NO_LOCAL_FILE.includes(video.fetch_status)}
					<Bms
						search={video.last_query}
						bind:override={override_query}
					/>
					<div class="flex justify-end mt-3 gap-3">
						<Button
							on:click={copyQuery}
							variant="outline"
							color="default">Copy Input</Button
						>

						<Button
							on:click={clearQuery}
							variant="outline"
							color="default">Clear Input</Button
						>

						<Button
							on:click={overrideQuery}
							variant="fill-outline"
							color="secondary">Run Query</Button
						>
					</div>

					<BRes
						result={video.last_result}
						bind:override={override_result}
					/>
					<div class="flex justify-end mt-3 gap-3">
						<Button
							on:click={copyResult}
							variant="outline"
							color="default">Copy Result</Button
						>
						<Button
							on:click={clearResult}
							variant="outline"
							color="default">Clear Result</Button
						>
						<Button
							on:click={overrideResult}
							variant="fill-outline"
							color="secondary">Apply Result</Button
						>
					</div>
				{:else}
					<h3>Could not fetch video</h3>
				{/if}
			</div>
		</Collapse>
	</Card>
</div>
