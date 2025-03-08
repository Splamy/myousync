// place files you want to import through the `$lib` alias in this folder.

import { FetchStatus } from "./defs";
import {
	mdiCheckCircleOutline,
	mdiInformationOutline,
	mdiAlertOutline,
	mdiAlertOctagonOutline,
	mdiDownload,
	mdiBrain,
	mdiTimerSandEmpty,
} from "@mdi/js";

export enum ConState {
	Connected,
	Connecting,
	Disconnected,
}

export enum SortMode {
	Unsorted,
	Title,
	FetchTime,
	LastUpdate,
	FailedFirst,
}

export const SortModes = [ SortMode.Unsorted, SortMode.Title, SortMode.FetchTime, SortMode.LastUpdate, SortMode.FailedFirst ] as const;

export function state_to_icon(state: FetchStatus) {
	switch (state) {
		case FetchStatus.NOT_FETCHED:
			return mdiTimerSandEmpty;
		case FetchStatus.FETCHED:
			return mdiDownload;
		case FetchStatus.FETCH_ERROR:
			return mdiDownload;
		case FetchStatus.BRAINZ_ERROR:
			return mdiBrain;
		case FetchStatus.CATEGORIZED:
			return mdiCheckCircleOutline;
		default:
			return mdiAlertOutline;
	}
}

export function state_to_color(state: FetchStatus) {
	switch (state) {
		case FetchStatus.NOT_FETCHED:
			return "cyan";
		case FetchStatus.FETCHED:
			return "green";
		case FetchStatus.FETCH_ERROR:
			return "red";
		case FetchStatus.BRAINZ_ERROR:
			return "red";
		case FetchStatus.CATEGORIZED:
			return "green";
		default:
			return "yellow";
	}
}
