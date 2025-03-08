<script lang="ts">
	import { Button, Card, TextField } from "svelte-ux";
	import { AUTH } from "./auth";

	let username = $state("");
	let password = $state("");
	let fetching = $state(false);
	let success = $state("" as string | true);
	let user_err = $derived(success === "User not found" ? "User not found" : false);
	let pass_err = $derived(success === "Invalid password" ? "Invalid password" : false);

	function login(e: Event) {
		e.preventDefault();
		async function async_login() {
			fetching = true;
			try {
				success = await AUTH.login(username, password);
			} finally {
				fetching = false;
			}
		}
		async_login();
	}
</script>

<form>
	<Card class="flex flex-col gap-2 w-full max-w-xs p-4">
		<TextField label="Username" bind:value={username} error={user_err} />
		<TextField label="Password" type="password" bind:value={password} error={pass_err} />
		<Button loading={fetching} type="submit" on:click={login}>Login</Button>
	</Card>
</form>
