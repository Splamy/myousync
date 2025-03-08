import { derived, Readable, Writable, writable } from 'svelte/store';
import { API_URL } from './defs';

class Auth {
	private _jwt: Writable<string | null> = writable(null);
	private _loggedIn: Readable<boolean> = derived(this._jwt, jwt => jwt !== null);

	public get loggedIn(): Readable<boolean> {
		return this._loggedIn;
	}

	public get jwt(): Readable<string | null> {
		return this._jwt;
	}

	public async init() {
		let jwt = localStorage.getItem("jwt");
		if (!jwt) {
			this._jwt.set(null);
			return;
		}

		let res = await fetch(`${API_URL}/login/check`, {
			method: "POST",
			headers: {
				Authorization: `Bearer ${jwt}`,
			},
		});

		if (!res.ok) {
			localStorage.removeItem("jwt");
			this._jwt.set(null);
			console.error("auth expired");
			return;
		}

		this._jwt.set(jwt);
	}

	async login(username: string, password: string) {
		const res = await fetch(`${API_URL}/login`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
			},
			body: JSON.stringify({ username, password }),
		});

		if (!res.ok) {
			let error = await res.json();
			console.error("login failed:", error);
			return error.error;
		}

		console.log("logged in");
		let jwt = await res.json();
		localStorage.setItem("jwt", jwt);

		this._jwt.set(jwt);
		return true;
	}
}

export const AUTH = new Auth();

