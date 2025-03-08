<script lang="ts">
	import { Field, Grid, TextField } from "svelte-ux";
	import type { BrainzMetadata } from "./defs";

	let {
		result,
		override = $bindable(),
	}: {
		result: BrainzMetadata | undefined | null;
		override: BrainzMetadata;
	} = $props();

	let result_artist = $derived(result?.artist.join("; "));

	let artist_join = $state(override.artist.join("; "));
	$effect(() => {
		override.artist = artist_join.split(";").map((x) => x.trim());
	});
</script>

<div class="grid grid-cols-4 gap-2">
	{#if result}
		<h3 style="grid-column:span 4">Result</h3>
		<Field
			label="Brainz ID"
			labelPlacement="top"
			value={result.brainz_recording_id}
		/>
		<Field label="Title" labelPlacement="top" value={result.title} />
		<Field label="Artist" labelPlacement="top" value={result_artist} />
		<Field label="Album" labelPlacement="top" value={result.album} />
	{:else}
		<h3 style="grid-column:span 4">No Result</h3>
	{/if}

	<TextField
		placeholder="Brainz ID"
		bind:value={override.brainz_recording_id}
	/>
	<TextField placeholder="Title" bind:value={override.title} />
	<TextField placeholder="Artist" bind:value={artist_join} />
	<TextField placeholder="Album" bind:value={override.album} />
</div>
