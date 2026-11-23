import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import SharePage from './+page.svelte';
import { setLocale } from '$lib/stores/i18n';
import { draftShare } from '$lib/stores/share';
import type { ShareUploadResult } from '$lib/services/share';

const SERVER = 'http://127.0.0.1:3030';
const LINK = `webdev-useful-tools://view?server=${SERVER}&id=05a055ac-2cbb-4e0a-9f0d-8b0d9d0e4b21`;

/** The page registers its drag-drop listener through a dynamic import that needs window metadata. */
const webview = vi.hoisted(() => {
	const onDragDropEvent = vi.fn(async () => () => {});
	return { onDragDropEvent, getCurrentWebview: vi.fn(() => ({ onDragDropEvent })) };
});

vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: webview.getCurrentWebview }));

const upload: ShareUploadResult = {
	id: '05a055ac-2cbb-4e0a-9f0d-8b0d9d0e4b21',
	link: LINK,
	server: SERVER,
	filename: 'order.xml',
	size: 4096,
	stored: false,
	expires_at: null
};

let invoke: Mock<(command: string) => Promise<unknown>> | undefined;
let component: ReturnType<typeof mount> | undefined;

/**
 * `isTauriRuntime()` only looks for this key and the Tauri API forwards every command to it, so one
 * stub turns the page into the desktop build without a backend. Commands that are not listed here —
 * the settings read and the event listener — answer `null`, which both services accept.
 */
function desktopBuild(failing: Partial<Record<string, string>> = {}): void {
	invoke = vi.fn(async (command: string) => {
		if (failing[command]) throw new Error(failing[command]);
		switch (command) {
			case 'list_share_interfaces':
				return [{ name: 'lo', ip: '127.0.0.1', url: SERVER, is_loopback: true }];
			case 'get_share_server_status':
				return { running: true, port: 3030, error: null };
			case 'plugin:dialog|open':
				return '/tmp/order.xml';
			case 'upload_share_file':
			case 'upload_share_content':
				return upload;
			default:
				return null;
		}
	});
	vi.stubGlobal('__TAURI_INTERNALS__', { invoke });
}

function render(): void {
	component = mount(SharePage, { target: document.body });
	flushSync();
}

/** Uploads reach the backend through IPC, so assertions have to wait for those promises. */
async function until(assertion: () => void): Promise<void> {
	const started = Date.now();
	let last: unknown;
	while (Date.now() - started < 2000) {
		try {
			assertion();
			return;
		} catch (error) {
			last = error;
			await new Promise((resolve) => setTimeout(resolve, 10));
		}
	}
	throw new Error(
		`${(last as Error)?.message ?? 'assertion failed'}\nCommands sent: ${commandsSent().join(', ') || 'none'}`
	);
}

function pill(label: string): HTMLButtonElement | undefined {
	return [...document.querySelectorAll<HTMLButtonElement>('button')].find(
		(button) => button.textContent?.trim() === label
	);
}

/** The active pill is the accent-filled one, which colour alone would not announce. */
function isPillActive(label: string): boolean {
	return pill(label)?.getAttribute('aria-pressed') === 'true';
}

function linkField(): HTMLInputElement | null {
	return document.querySelector<HTMLInputElement>('input[aria-label="Share link"]');
}

function retentionSelect(): HTMLElement | null {
	return document.querySelector('#share-retention');
}

function commandsSent(): string[] {
	return invoke?.mock.calls.map(([command]) => command) ?? [];
}

async function uploadPickedFile(): Promise<void> {
	// The button stays disabled until the interface and the server status have arrived, so waiting
	// for it also proves the page is ready — clicking earlier would only flash "select a server".
	await until(() => expect(pill('Upload file')?.disabled).toBe(false));
	pill('Upload file')?.click();
	await until(() => expect(commandsSent()).toContain('upload_share_file'));
}

beforeEach(() => {
	setLocale('en');
});

afterEach(() => {
	if (component) unmount(component);
	component = undefined;
	vi.unstubAllGlobals();
	draftShare.set(null);
	localStorage.clear();
});

describe('Share page', () => {
	it('offers no upload controls in the browser build', async () => {
		render();

		await until(() => {
			expect(document.body.textContent).toContain(
				'Sharing works in the desktop app so files stay on your network.'
			);
			// The stub branch has nothing to load, so the start-up line must not stay on screen.
			expect(document.body.textContent).not.toContain('Starting share server…');
		});
		expect(pill('File')).toBeUndefined();
		expect(pill('Upload file')).toBeUndefined();
		expect(retentionSelect()).toBeNull();
		expect(webview.getCurrentWebview).not.toHaveBeenCalled();
	});

	it('keeps the link tab out of the tab bar until an upload exists', async () => {
		desktopBuild();
		render();

		await until(() => expect(pill('File')).toBeTruthy());
		expect(pill('Content')).toBeTruthy();
		expect(pill('Link')).toBeUndefined();
		expect(linkField()).toBeNull();
		expect(retentionSelect()).not.toBeNull();
	});

	it('moves the link onto its own tab and activates it as soon as the upload succeeds', async () => {
		desktopBuild();
		render();

		await uploadPickedFile();
		await until(() => expect(linkField()).not.toBeNull());

		expect(isPillActive('Link')).toBe(true);
		expect(isPillActive('File')).toBe(false);
		expect(linkField()?.value).toBe(LINK);
		expect(document.body.textContent).toContain('Kept in memory only');
		// The result lives inside the card that carries the pills, never in a block below it.
		expect(linkField()?.closest('section')?.textContent).toContain('Link');
	});

	it('hides the retention choice while the link tab is open and brings it back on File', async () => {
		desktopBuild();
		render();

		await until(() => expect(retentionSelect()).not.toBeNull());
		await uploadPickedFile();
		await until(() => expect(linkField()).not.toBeNull());
		expect(retentionSelect()).toBeNull();

		pill('File')?.click();
		flushSync();
		expect(retentionSelect()).not.toBeNull();
		expect(isPillActive('File')).toBe(true);
	});

	it('returns to the file tab and shows no link when the upload fails', async () => {
		desktopBuild({ upload_share_file: 'Share server is unavailable' });
		render();

		await uploadPickedFile();

		expect(pill('Link')).toBeUndefined();
		expect(linkField()).toBeNull();
		expect(isPillActive('File')).toBe(true);
	});

	it('drops the link tab when a new file is picked after an upload', async () => {
		desktopBuild();
		render();

		await uploadPickedFile();
		await until(() => expect(linkField()).not.toBeNull());

		// Leaving the tab hides the link but keeps the upload, so the pill stays available.
		pill('File')?.click();
		flushSync();
		expect(linkField()).toBeNull();
		expect(pill('Link')).toBeTruthy();

		const dialogsBefore = commandsSent().filter((command) => command === 'plugin:dialog|open').length;
		pill('Select file')?.click();
		await until(() =>
			expect(commandsSent().filter((command) => command === 'plugin:dialog|open')).toHaveLength(
				dialogsBefore + 1
			)
		);
		await until(() => expect(pill('Link')).toBeUndefined());

		expect(linkField()).toBeNull();
		expect(isPillActive('File')).toBe(true);
	});
});
