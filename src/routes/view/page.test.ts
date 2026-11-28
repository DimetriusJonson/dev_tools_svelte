import { flushSync, mount, unmount } from 'svelte';
import { writable } from 'svelte/store';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import ViewPage from './+page.svelte';
import { setLocale } from '$lib/stores/i18n';

const SERVER = 'http://127.0.0.1:3030';
const ID = '05a055ac-2cbb-4e0a-9f0d-8b0d9d0e4b21';

vi.mock('$app/stores', () => ({
	page: writable({
		url: new URL('http://localhost:5173/view?server=http://127.0.0.1:3030&id=05a055ac-2cbb-4e0a-9f0d-8b0d9d0e4b21')
	})
}));

/** Shared mutable holder: the hoisted mock factory cannot read later-initialised test state. */
const tauri = vi.hoisted(() => ({
	enabled: false,
	invoke: vi.fn()
}));

vi.mock('$lib/services/db', () => ({
	isTauriRuntime: () => tauri.enabled,
	invokeCommand: tauri.invoke,
	getSetting: async () => null,
	setSetting: async () => {}
}));

const UPLOAD = {
	id: ID,
	viewer: 'text',
	language: 'xml',
	content: '<root/>',
	filename: 'order.xml',
	content_type: 'application/xml',
	size: 9,
	created_at: '2026-11-28T10:00:00Z',
	expires_at: null,
	raw_url: '',
	stored: false
};

/** jsdom lays out no text; CodeMirror's measure pass needs these to answer "no rectangles". */
const originalClientRects = Range.prototype.getClientRects;
const originalBoundingRect = Range.prototype.getBoundingClientRect;

beforeAll(() => {
	Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
	Range.prototype.getBoundingClientRect = () => new DOMRect();
});

afterAll(() => {
	Range.prototype.getClientRects = originalClientRects;
	Range.prototype.getBoundingClientRect = originalBoundingRect;
});

function stubMatchMedia(): void {
	vi.stubGlobal(
		'matchMedia',
		vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }))
	);
}

function render(): ReturnType<typeof mount> {
	return mount(ViewPage, { target: document.body });
}

function deleteTrigger(): HTMLButtonElement | undefined {
	return [...document.querySelectorAll<HTMLButtonElement>('button')].find(
		(button) => button.getAttribute('aria-label') === 'Delete this upload'
	);
}

function deleteConfirmButton(): HTMLButtonElement | undefined {
	return [...document.querySelectorAll<HTMLButtonElement>('button')].find(
		(button) => button.textContent?.trim() === 'Delete'
	);
}

function cancelButton(): HTMLButtonElement | undefined {
	return [...document.querySelectorAll<HTMLButtonElement>('button')].find(
		(button) => button.textContent?.trim() === 'Cancel'
	);
}

/** IPC is async, so DOM assertions about its results have to keep polling. */
async function until(assertion: () => void): Promise<void> {
	const started = Date.now();
	let last: unknown;
	while (Date.now() - started < 2500) {
		try {
			assertion();
			return;
		} catch (error) {
			last = error;
			await new Promise((resolve) => setTimeout(resolve, 10));
		}
	}
	throw new Error(`timed out: ${(last as Error)?.message ?? 'assertion failed'}`);
}

beforeEach(() => {
	setLocale('en');
	stubMatchMedia();
	localStorage.clear();
	tauri.enabled = false;
	tauri.invoke.mockReset();
});

afterEach(() => {
	vi.unstubAllGlobals();
	localStorage.clear();
});

describe('View page', () => {
	it('loads an upload on the desktop and asks once before deleting', async () => {
		tauri.enabled = true;
		tauri.invoke.mockImplementation(async (command: string) => {
			switch (command) {
				case 'load_shared_upload':
					return UPLOAD;
				case 'delete_shared_upload':
					return undefined;
				default:
					return null;
			}
		});
		const component = render();

		await until(() => expect(document.body.textContent).toContain('order.xml'));

		const trigger = deleteTrigger();
		expect(trigger).not.toBeUndefined();
		trigger?.click();
		flushSync();
		expect(document.body.textContent).toContain(
			'Delete this upload? The link stops working for everyone.'
		);

		deleteConfirmButton()?.click();

		await until(() => {
			expect(document.body.textContent).toContain('Upload deleted. The link no longer works.');
			expect(tauri.invoke.mock.calls.map((call) => call[0])).toContain('delete_shared_upload');
		});

		unmount(component);
	});

	it('does not delete when the confirmation is cancelled', async () => {
		tauri.enabled = true;
		tauri.invoke.mockImplementation(async (command: string) => {
			switch (command) {
				case 'load_shared_upload':
					return UPLOAD;
				default:
					return null;
			}
		});
		const component = render();

		await until(() => expect(document.body.textContent).toContain('order.xml'));

		deleteTrigger()?.click();
		flushSync();
		expect(document.body.textContent).toContain('Delete this upload? The link stops working for everyone.');

		cancelButton()?.click();
		flushSync();
		expect(document.body.textContent).not.toContain('Upload deleted. The link no longer works.');
		await until(() => expect(tauri.invoke.mock.calls.map((call) => call[0])).not.toContain('delete_shared_upload'));

		unmount(component);
	});
});