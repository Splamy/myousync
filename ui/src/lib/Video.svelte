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
	} from "svelte-ux";
	import { mdiAlertOctagonOutline } from "@mdi/js";

	import Bms from "./BMS.svelte";
	import BRes from "./BRes.svelte";
	import { state_to_color, state_to_icon } from "$lib";
	import { crossfade, fade, fly } from "svelte/transition";

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
		return localStorage.getItem("jwt");
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
		let res = await authFetch(
			`${API_URL}/video/${video.video_id}/query`,
			JSON.stringify(override_query),
		);
		if (res.ok) {
			video.override_query = override_query;
		}
	}

	async function overrideResult() {
		let res = await authFetch(
			`${API_URL}/video/${video.video_id}/result`,
			JSON.stringify(override_result),
		);
		if (res.ok) {
			video.override_result = override_result;
		}
	}

	async function retryFetch() {
		await authFetch(`${API_URL}/video/${video.video_id}/retry_fetch`);
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
		navigator.clipboard.writeText(yturl);
		copyPopover = true;
		clearTimeout(copyPopoverTimeout);
		copyPopoverTimeout = setTimeout(() => {
			copyPopover = false;
		}, 1000);
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
				{#if video.fetch_status !== FetchStatus.FETCH_ERROR}
					<Bms
						search={video.last_query}
						bind:override={override_query}
					/>
					<div class="flex justify-end mt-3 gap-3">
						<Button
							on:click={copyQuery}
							variant="fill"
							color="neutral">Copy Input</Button
						>

						<Button
							on:click={clearQuery}
							variant="fill"
							color="neutral">Clear Input</Button
						>

						<Button
							on:click={overrideQuery}
							variant="fill"
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
							variant="fill"
							color="neutral">Copy Result</Button
						>
						<Button
							on:click={clearResult}
							variant="fill"
							color="neutral">Clear Result</Button
						>
						<Button
							on:click={overrideResult}
							variant="fill"
							color="secondary">Apply Result</Button
						>
					</div>
				{:else}
					<h3>Could not fetch video</h3>
					<Button
						on:click={retryFetch}
						variant="fill"
						color="secondary">Retry</Button
					>
				{/if}
			</div>
		</Collapse>
	</Card>
</div>
