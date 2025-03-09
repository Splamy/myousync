// place files you want to import through the `$lib` alias in this folder.

import { FetchStatus } from "./defs";
import {
	mdiCheckCircleOutline,
	mdiAlertOutline,
	mdiDownload,
	mdiBrain,
	mdiTimerSandEmpty,
	mdiClose,
	mdiDownloadOff,
} from "@mdi/js";

export enum ConState {
	Connected,
	Connecting,
	Disconnected,
}

export enum SortMode {
	Unsorted,
	VideoId,
	FetchTime,
	LastUpdate,
	FailedFirst,
}

export const SortModes = [SortMode.Unsorted, SortMode.VideoId, SortMode.FetchTime, SortMode.LastUpdate, SortMode.FailedFirst] as const;

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
		case FetchStatus.DISABLED:
			return mdiDownloadOff;
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
		case FetchStatus.DISABLED:
			return "grey";
		default:
			return "yellow";
	}
}
