<script lang="ts">
	import {
		API_URL,
		FetchStatus,
		type BrainzMetadata,
		type BrainzMultiSearch,
		type VideoData,
	} from "./defs";
	import { Button, Collapse, Icon, Card, Tooltip } from "svelte-ux";

	import Bms from "./BMS.svelte";
	import BRes from "./BRes.svelte";
	import { state_to_color, state_to_icon } from "$lib";

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
		if (video.last_result) {
			return `Result: ${video.last_result.title} - ${video.last_result.artist.join(
				"; ",
			)}`;
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
		const res = await fetch(`${API_URL}/video/${video.video_id}/query`, {
			method: "POST",
			mode: "cors",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${getAuth()}`,
			},
			body: JSON.stringify(override_query),
		});
		if (res.ok) {
			video.override_query = override_query;
		}
	}

	async function overrideResult() {
		const res = await fetch(`${API_URL}/video/${video.video_id}/result`, {
			method: "POST",
			mode: "cors",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${getAuth()}`,
			},
			body: JSON.stringify(override_result),
		});
		if (res.ok) {
			video.override_result = override_result;
		}
	}
</script>

<div class="px-3">
	<Card>
		<Collapse>
			<div slot="trigger" class="flex-1 px-3 py-3">
				<Tooltip title={video.fetch_status} placement="top">
					<Icon data={state_icon} style={"color:" + state_color} />
				</Tooltip>
				<Button
					class="font-mono"
					color="accent"
					variant="outline"
					disabled
				>
					{video.video_id}</Button
				>
				<span class="ml-1 text-center">
					{display_text}
				</span>
			</div>
			<div class="p-3 border-t">
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
				{/if}
			</div>
		</Collapse>
	</Card>
</div>
