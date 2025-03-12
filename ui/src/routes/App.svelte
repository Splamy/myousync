<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { SvelteMap } from "svelte/reactivity";
	import Video from "$lib/Video.svelte";
	import {
		API_URL,
		BrainzMetadata_contains,
		BrainzMultiSearch_contains,
		FetchStatus,
		type VideoData,
	} from "$lib/defs";
	import { ConState, SortMode, SortModes } from "$lib";
	import {
		AppBar,
		AppLayout,
		Button,
		Card,
		Switch,
		TextField,
		ThemeSelect,
		Notification,
		ToggleGroup,
		ToggleOption,
		Field,
		Toggle,
		Dialog,
	} from "svelte-ux";
	import AuthForm from "$lib/AuthForm.svelte";
	import { AUTH } from "$lib/auth";
	import { mdiClose } from "@mdi/js";

	AUTH.init();

	const CAT_FAILED = [FetchStatus.FETCH_ERROR, FetchStatus.BRAINZ_ERROR];
	const CAT_FETCHING = [FetchStatus.NOT_FETCHED, FetchStatus.FETCHED];
	const CAT_OK = [FetchStatus.CATEGORIZED];
	const CAT_DISABLED = [FetchStatus.DISABLED];

	let show_ok = $state(true);
	let show_err = $state(true);
	let show_fetching = $state(true);
	let show_disabled = $state(true);
	let show_filter = $state("");
	let show_sort = $state(SortMode.Unsorted);

	let videos = new SvelteMap<string, VideoData>();
	let sorted_videos = $derived.by(() => {
		let rlist: VideoData[] = [];
		for (let v of videos.values()) {
			if (CAT_FETCHING.includes(v.fetch_status) && !show_fetching)
				continue;
			if (CAT_OK.includes(v.fetch_status) && !show_ok) continue;
			if (CAT_FAILED.includes(v.fetch_status) && !show_err) continue;
			if (CAT_DISABLED.includes(v.fetch_status) && !show_disabled)
				continue;

			if (show_filter) {
				let matches =
					v.video_id.includes(show_filter) ||
					(v.last_query &&
						BrainzMultiSearch_contains(
							v.last_query,
							show_filter,
						)) ||
					(v.override_query &&
						BrainzMultiSearch_contains(
							v.override_query,
							show_filter,
						)) ||
					(v.last_result &&
						BrainzMetadata_contains(v.last_result, show_filter)) ||
					(v.override_result &&
						BrainzMetadata_contains(
							v.override_result,
							show_filter,
						));
				if (!matches) continue;
			}

			rlist.push(v);
		}

		switch (show_sort) {
			case SortMode.Unsorted:
				break;
			case SortMode.FetchTime:
				rlist.sort((a, b) => b.fetch_time - a.fetch_time);
				break;
			case SortMode.LastUpdate:
				rlist.sort((a, b) => b.last_update - a.last_update);
				break;
			case SortMode.VideoId:
				rlist.sort((a, b) => a.video_id.localeCompare(b.video_id));
				break;
			case SortMode.FailedFirst:
				rlist.sort((a, b) => {
					if (CAT_FAILED.includes(a.fetch_status)) {
						return -1;
					}
					if (CAT_FAILED.includes(b.fetch_status)) {
						return 1;
					}
					return a.video_id.localeCompare(b.video_id);
				});
				break;
		}

		return rlist;
	});

	let connected = $state(ConState.Disconnected);
	let ws: WebSocket | undefined;

	let jwt = $derived(AUTH.jwt);
	let hasInit = false;

	$effect(() => {
		if ($jwt && !hasInit) {
			try {
				ws?.send($jwt);
			} catch (e) {
				if (connected === ConState.Connected) {
					connected = ConState.Disconnected;
					ws?.close();
				}
			}
			hasInit = true;
		}
	});

	function tryConnectWs() {
		if (connected !== ConState.Disconnected) {
			return;
		}

		hasInit = false;
		connected = ConState.Connecting;

		ws = new WebSocket(`${API_URL}/ws`);
		ws.onmessage = (event) => {
			let newvid = JSON.parse(event.data);
			for (const vid of newvid) {
				videos.set(vid.video_id, vid);
			}
		};

		ws.onopen = async function (event) {
			connected = ConState.Connected;
			console.log("ws opened");

			if ($jwt) {
				this.send($jwt);
				hasInit = true;
			}
		};
		ws.onclose = function (event) {
			connected = ConState.Disconnected;
			console.log("ws closed");
			this.close();
		};
		ws.onerror = function (event) {
			connected = ConState.Disconnected;
			console.log("ws error");
			this.close();
		};
	}

	function reindexAllInView() {
		let ids = sorted_videos.map((v) => v.video_id);
		fetch(`${API_URL}/reindex`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				Authorization: `Bearer ${$jwt}`,
			},
			body: JSON.stringify(ids),
		});
	}

	onMount(() => {
		tryConnectWs();

		let interval = setInterval(() => {
			if (connected === ConState.Disconnected) {
				tryConnectWs();
			}
		}, 1000);

		return () => {
			clearInterval(interval);
		};
	});

	onDestroy(() => {
		console.log("destroying");
		ws?.close();
	});
</script>

<AppLayout
	areas="'header header' 'aside main'"
	classes={{ nav: "bg-surface-300 pr-4" }}
>
	<svelte:fragment slot="nav">
		<div class="pt-4 pl-4 grid gap-3">
			<TextField
				bind:value={show_filter}
				label="Filter"
				labelPlacement="float"
			>
				<span slot="append">
					<Button
						on:click={() => (show_filter = "")}
						icon={mdiClose}
						class="text-surface-content/50 p-2"
					/>
				</span>
			</TextField>

			<label class="flex gap-2 items-center justify-end text-sm">
				Ok
				<Switch bind:checked={show_ok} color="success" />
			</label>
			<label class="flex gap-2 items-center justify-end text-sm">
				Err
				<Switch bind:checked={show_err} color="danger" />
			</label>
			<label class="flex gap-2 items-center justify-end text-sm">
				Unfetched
				<Switch bind:checked={show_fetching} color="warning" />
			</label>
			<label class="flex gap-2 items-center justify-end text-sm">
				Disabled
				<Switch bind:checked={show_disabled} color="warning" />
			</label>

			<Field label="Sort">
				<ToggleGroup
					bind:value={show_sort}
					class="w-full"
					inset
					vertical
				>
					{#each SortModes as mode}
						<ToggleOption value={mode} class="text-end"
							>{SortMode[mode]}</ToggleOption
						>
					{/each}
				</ToggleGroup>
			</Field>

			<Toggle let:on={open} let:toggle let:toggleOff>
				<Button on:click={toggle} variant="outline" color="danger"
					>Reindex all in View</Button
				>
				<Dialog {open} on:close={toggleOff}>
					<div slot="title">Reindex all in view?</div>
					<div class="px-6 py-3">
						Reindexing {sorted_videos.filter((x) =>
							CAT_OK.includes(x.fetch_status),
						).length} videos
					</div>
					<div slot="actions">
						<Button
							on:click={reindexAllInView}
							variant="fill"
							color="danger"
						>
							Start Reindex
						</Button>
						<Button>Cancel</Button>
					</div>
				</Dialog>
			</Toggle>
		</div>
	</svelte:fragment>

	<AppBar title="Myousync" class="bg-primary text-primary-content">
		{#if connected !== ConState.Connected}
			<Notification
				title="Connection Lost, Connecting..."
				color="danger"
				variant="fill"
				class="ml-4"
			/>
		{/if}

		<div slot="actions">
			<ThemeSelect />
		</div>
	</AppBar>

	<main>
		{#if !$jwt}
			<div class="flex justify-center items-center pt-8">
				<AuthForm />
			</div>
		{:else}
			<div class="p-4 flex gap-4 flex-col">
				{#each sorted_videos as video (video.video_id)}
					<Video {video} />
				{/each}
			</div>
		{/if}
	</main>
</AppLayout>
