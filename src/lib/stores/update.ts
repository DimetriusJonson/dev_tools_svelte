import { get, writable } from 'svelte/store';
import { version } from '$app/environment';
import { getSetting, setSetting } from '$lib/services/db';
import {
	checkForUpdate,
	closeUpdate,
	downloadAndInstallUpdate,
	isUpdaterSupported,
	type UpdateAnnouncement
} from '$lib/services/updater';
import {
	classifyUpdateFailure,
	DISMISSED_VERSION_KEY,
	downloadPercent,
	IDLE_UPDATE_STATE,
	isDismissedVersion,
	isUpdateBusy,
	updateFailureMessage,
	type UpdateState
} from '$lib/utils/update';
import { t } from './i18n';
import { notify } from './toast';

/** Shared between the navbar button and the banner, so one check feeds both. */
export const updateState = writable<UpdateState>(IDLE_UPDATE_STATE);

/** Announced update held on the Rust side; the plugin needs it for install and for cleanup. */
let announced: UpdateAnnouncement | null = null;

function translate(key: string, params?: Record<string, string | number>): string {
	return get(t)(key, params);
}

function patch(next: Partial<UpdateState>): void {
	updateState.update((state) => ({ ...state, ...next }));
}

async function forgetAnnouncement(): Promise<void> {
	const pending = announced;
	announced = null;
	if (pending) await closeUpdate(pending.handle);
}

async function handleFailure(error: unknown, manual: boolean): Promise<void> {
	const message = updateFailureMessage(error, translate('common.error'));
	// Any failure replaces the announcement, so the old handle must be released here:
	// the banner leaves "available" and the pending download would otherwise leak.
	await forgetAnnouncement();
	if (!manual) {
		console.warn('Automatic update check failed', message);
		updateState.set(IDLE_UPDATE_STATE);
		return;
	}
	const kind = classifyUpdateFailure(message);
	if (kind === 'unavailable') {
		notify(translate('update.unavailable'), 'error');
		updateState.set(IDLE_UPDATE_STATE);
		return;
	}
	if (kind === 'notFound') {
		notify(translate('update.notFound'), 'info');
		updateState.set(IDLE_UPDATE_STATE);
		return;
	}
	patch({ ...IDLE_UPDATE_STATE, phase: 'error', message: translate('update.failed', { message }) });
}

/**
 * Reads the configured endpoint. Startup checks stay silent: they only raise the banner when
 * the announced version was not dismissed, while a manual check always answers.
 */
export async function checkForUpdates({ manual }: { manual: boolean }): Promise<void> {
	if (!isUpdaterSupported()) return;
	const phase = get(updateState).phase;
	if (isUpdateBusy(phase) || phase === 'ready') return;

	patch({ ...IDLE_UPDATE_STATE, phase: 'checking' });
	try {
		const update = await checkForUpdate();
		if (!update) {
			await forgetAnnouncement();
			updateState.set(IDLE_UPDATE_STATE);
			if (manual) notify(translate('update.upToDate', { version }), 'success');
			return;
		}
		// A manual check may run while the previous banner is still "available", so a
		// replaced announcement must close its handle before the new one is stored.
		await forgetAnnouncement();
		announced = update;
		if (!manual && isDismissedVersion(await getSetting(DISMISSED_VERSION_KEY), update.version)) {
			await forgetAnnouncement();
			updateState.set(IDLE_UPDATE_STATE);
			return;
		}
		patch({ phase: 'available', version: update.version, notes: update.notes, percent: 0, message: null });
	} catch (error) {
		await handleFailure(error, manual);
	}
}

/**
 * Downloads and installs the announced release. On Windows the plugin exits the app to launch
 * the installer, so this promise never resolves there and the banner simply stays on "installing".
 */
export async function installUpdate(): Promise<void> {
	const pending = announced;
	if (!pending || isUpdateBusy(get(updateState).phase)) return;

	let downloaded = 0;
	let total = 0;
	patch({ phase: 'downloading', percent: 0, message: null });

	try {
		await downloadAndInstallUpdate(pending.handle, (event) => {
			if (event.event === 'Started') {
				total = event.data.contentLength ?? 0;
				downloaded = 0;
				patch({ phase: 'downloading', percent: downloadPercent(downloaded, total) });
			} else if (event.event === 'Progress') {
				downloaded += event.data.chunkLength;
				patch({ phase: 'downloading', percent: downloadPercent(downloaded, total) });
			} else if (event.event === 'Finished') {
				patch({ phase: 'installing', percent: 100 });
			}
		});

		announced = null;
		patch({ phase: 'ready', percent: 100 });
		// The installer owns relaunch. On Windows it runs outside the app, on macOS/Linux the
		// updater replaces the bundle in place, so neither needs a restart prompt from us.
	} catch (error) {
		handleFailure(error, true);
	}
}

/** "Not now": hides exactly this version, so the next release is reported again. */
export async function dismissUpdate(): Promise<void> {
	const shown = get(updateState).version;
	await forgetAnnouncement();
	if (shown) await setSetting(DISMISSED_VERSION_KEY, shown);
	updateState.set(IDLE_UPDATE_STATE);
}

/** Closes the banner without hiding the version: used for a failed attempt. */
export async function clearUpdate(): Promise<void> {
	await forgetAnnouncement();
	updateState.set(IDLE_UPDATE_STATE);
}
