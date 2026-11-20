/** Lifecycle of the in-app updater, mirrored by the banner and the navbar button. */
export type UpdatePhase =
	| 'idle'
	| 'checking'
	| 'available'
	| 'downloading'
	| 'installing'
	| 'ready'
	| 'error';

export interface UpdateState {
	phase: UpdatePhase;
	/** Version announced by the update endpoint, `null` while nothing is known. */
	version: string | null;
	/** Release notes from the endpoint, shown when the release publishes them. */
	notes: string | null;
	/** Download progress in percent; 0 until the endpoint reports a content length. */
	percent: number;
	/** Localized failure text for the banner, `null` while nothing failed. */
	message: string | null;
}

export const IDLE_UPDATE_STATE: UpdateState = {
	phase: 'idle',
	version: null,
	notes: null,
	percent: 0,
	message: null
};

/** Phases that already own a running download or installer, so a new check must not start. */
export function isUpdateBusy(phase: UpdatePhase): boolean {
	return phase === 'checking' || phase === 'downloading' || phase === 'installing';
}

/**
 * Percent of the update download that has been received. A missing or zero content length
 * keeps the value at 0, so an unreported size never fakes a full progress bar.
 */
export function downloadPercent(downloaded: number, total: number | undefined): number {
	if (!total || total <= 0 || downloaded <= 0) return 0;
	return Math.min(100, Math.round((downloaded / total) * 100));
}

/** Dismissing hides exactly the announced version; a newer release is reported again. */
export function isDismissedVersion(dismissed: string | null, candidate: string): boolean {
	return dismissed !== null && dismissed === candidate;
}

export type UpdateFailureKind = 'unavailable' | 'notFound' | 'failed';

/**
 * Groups the plugin's error text. `unavailable` means this build cannot update at all
 * (no endpoints, plain HTTP endpoint, unsupported OS or CPU); `notFound` means the endpoint
 * answered but has no release for this platform yet, which is not worth a red banner.
 */
export function classifyUpdateFailure(message: string): UpdateFailureKind {
	const text = message.toLowerCase();
	if (
		text.includes('does not have any endpoints') ||
		text.includes('secure protocol') ||
		text.includes('unsupported os') ||
		text.includes('unsupported application architecture')
	) {
		return 'unavailable';
	}
	if (
		text.includes('could not fetch a valid release json') ||
		text.includes('not found in the response') ||
		text.includes('were found in the response')
	) {
		return 'notFound';
	}
	return 'failed';
}

/** Text of whatever the plugin threw, for the localized failure message. */
export function updateFailureMessage(error: unknown, fallback: string): string {
	if (typeof error === 'string') return error.trim() || fallback;
	if (error instanceof Error) return error.message.trim() || fallback;
	return fallback;
}

/**
 * Windows installers are launched after the app exits and relaunch it themselves, so the
 * frontend must not ask the backend to restart there.
 */
export function isWindowsUserAgent(userAgent: string): boolean {
	return /windows/i.test(userAgent);
}

/** Wait after hydration before the startup check so the first paint is not competing with a request. */
export const STARTUP_UPDATE_DELAY_MS = 1200;

/** `app_settings` key holding the version the user dismissed with "Not now". */
export const DISMISSED_VERSION_KEY = 'update.dismissedVersion';
