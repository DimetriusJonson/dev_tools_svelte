import { invokeCommand, isTauriRuntime } from './db';

/** The update resource the plugin keeps on the Rust side. */
export type UpdateHandle = import('@tauri-apps/plugin-updater').Update;

export type UpdateDownloadEvent = import('@tauri-apps/plugin-updater').DownloadEvent;

export interface UpdateAnnouncement {
	handle: UpdateHandle;
	version: string;
	notes: string | null;
	date: string | null;
}

/** The check request must not hold the UI waiting for a slow release host. */
const CHECK_TIMEOUT_MS = 20_000;

/** Update bundles are tens of megabytes, so the download gets a much longer timeout. */
const DOWNLOAD_TIMEOUT_MS = 300_000;

/** Updates are a desktop feature: the browser build has neither the plugin nor signed artifacts. */
export function isUpdaterSupported(): boolean {
	return isTauriRuntime();
}

export async function checkForUpdate(): Promise<UpdateAnnouncement | null> {
	const { check } = await import('@tauri-apps/plugin-updater');
	const update = await check({ timeout: CHECK_TIMEOUT_MS });
	if (!update) return null;
	return {
		handle: update,
		version: update.version,
		notes: update.body ?? null,
		date: update.date ?? null
	};
}

export async function downloadAndInstallUpdate(
	update: UpdateHandle,
	onEvent: (event: UpdateDownloadEvent) => void
): Promise<void> {
	await update.downloadAndInstall(onEvent, { timeout: DOWNLOAD_TIMEOUT_MS });
}

/** Releases the Rust-side resource; a failure here is only a leaked temp file. */
export async function closeUpdate(update: UpdateHandle): Promise<void> {
	try {
		await update.close();
	} catch (error) {
		console.warn('Failed to release the update resource', error);
	}
}

/**
 * Restarts the app through the backend. On macOS/Linux the process usually dies before the
 * IPC reply arrives, so a rejection here is expected and must not surface as an error toast.
 */
export async function restartApplication(): Promise<void> {
	try {
		await invokeCommand('restart_app');
	} catch (error) {
		console.warn('Failed to restart the app after the update', error);
	}
}
