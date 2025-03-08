<script lang="ts">
	import { Field, Grid, TextField } from "svelte-ux";
	import type { BrainzMultiSearch } from "./defs";

	let {
		search = $bindable(),
		override = $bindable(),
	}: {
		search: BrainzMultiSearch | undefined | null;
		override: BrainzMultiSearch;
	} = $props();
</script>

<div class="grid grid-cols-4 gap-2">
	{#if search}
		<h3 style="grid-column:span 4">Query</h3>

		<Field label="Brainz ID" labelPlacement="top" value={search.trackid} />
		<Field label="Title" labelPlacement="top" value={search.title} />
		<Field label="Artist" labelPlacement="top" value={search.artist} />
		<Field label="Album" labelPlacement="top" value={search.album} />
	{:else}
		<h3 style="grid-column:span 4">No Query</h3>
	{/if}
	<TextField placeholder="Brainz ID" bind:value={override.trackid} />
	<TextField placeholder="Title" bind:value={override.title} />
	<TextField placeholder="Artist" bind:value={override.artist} />
	<TextField placeholder="Album" bind:value={override.album} />
</div>
