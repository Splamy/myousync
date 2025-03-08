export const API_URL = process.env.NODE_ENV === "development"
	? 'http://localhost:3001'
	: `${import.meta.env.ASSET_PREFIX}`.replace(/\/*$/, '');

export interface VideoData {
	video_id: string;
	last_update: number;
	fetch_time: number;
	fetch_status: FetchStatus;
	last_query?: BrainzMultiSearch;
	last_result?: BrainzMetadata;
	last_error?: string;
	override_query?: BrainzMultiSearch;
	override_result?: BrainzMetadata;
}

export interface BrainzMultiSearch {
	trackid?: string;
	title: string;
	artist?: string;
	album?: string;
}

export interface BrainzMetadata {
	brainz_recording_id?: string;
	title: string;
	artist: string[];
	album?: string;
}

export const enum FetchStatus {
	NOT_FETCHED = "NotFetched",
	FETCHED = "Fetched",
	FETCH_ERROR = "FetchError",
	BRAINZ_ERROR = "BrainzError",
	CATEGORIZED = "Categorized",
}

export function BrainzMetadata_contains(data: BrainzMetadata, text: string) {
	if (data.title.toLowerCase().includes(text)) return true;
	if (data.artist.some(a => a.toLowerCase().includes(text))) return true;
	if (data.album && data.album.toLowerCase().includes(text)) return true;
	return false;
}

export function BrainzMultiSearch_contains(data: BrainzMultiSearch, text: string) {
	if (data.title.toLowerCase().includes(text)) return true;
	if (data.artist && data.artist.toLowerCase().includes(text)) return true;
	if (data.album && data.album.toLowerCase().includes(text)) return true;
	return false;
}
