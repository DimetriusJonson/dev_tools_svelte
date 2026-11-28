import { flushSync, mount } from 'svelte';
import { get } from 'svelte/store';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import RestPage from './+page.svelte';
import { setLocale, t, type Translate } from '$lib/stores/i18n';
import { toasts } from '$lib/stores/toast';

interface Property {
	name: string;
	value: string;
}

interface Request {
	id: string;
	project_id: string | null;
	name: string;
	method: string;
	url: string;
	body: string;
	body_type: string;
	save_response: boolean;
	headers: Property[];
	form: Property[];
	multipart: Property[];
	files: Property[];
	response_body: string | null;
	response_headers: Property[];
	raw_request: string | null;
	response_status_line: string | null;
	request_tab: string;
	multipart_tab: string;
	response_tab: string;
	created_at: string;
	updated_at: string;
}

/** A request whose header value carries a template, so every guard below has a reason. */
function templatedRequest(): Request {
	return {
		id: 'aaaaaaaa-0000-4000-8000-000000000001',
		project_id: null,
		name: 'Tokened',
		method: 'GET',
		url: 'https://example.com/p',
		body: '',
		body_type: 'json',
		save_response: false,
		headers: [{ name: 'Authorization', value: 'Bearer {{ token }}' }],
		form: [],
		multipart: [],
		files: [],
		response_body: null,
		response_headers: [],
		raw_request: null,
		response_status_line: null,
		// Reopened on the Raw tab, where the substitution hint belongs.
		request_tab: 'raw',
		multipart_tab: 'fields',
		response_tab: 'response',
		created_at: '2026-01-01 00:00:00',
		updated_at: '2026-01-01 00:00:00'
	};
}

let calls: Array<[string, Record<string, unknown>]>;
let executeFailure: string | null;
let component: ReturnType<typeof mount> | undefined;

const say: Translate = (key, params) => get(t)(key, params);

/** A Rust command rejects with its error **string**, not an `Error`. */
function failBackend(text: string): never {
	throw text;
}

function desktopBuild(request: Request): void {
	calls = [];
	executeFailure = null;
	(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
		invoke: async (command: string, args: Record<string, unknown> = {}) => {
			calls.push([command, args]);
			if (command === 'list_rest_projects') return [];
			if (command === 'list_rest_requests') return [request];
			if (command === 'execute_rest_request') {
				if (executeFailure) failBackend(executeFailure);
				return {
					status: 200,
					status_text: 'OK',
					status_line: 'HTTP/1.1 200 OK',
					body: '<ok/>',
					headers: [],
					language: 'xml',
					time_ms: 12,
					size: 5,
					is_attachment: false,
					is_binary: false,
					content_type: 'application/xml',
					attachment_name: null
				};
			}
			return null;
		}
	};
}

/** The browser build: no Tauri wire, so the request list and the selection read storage. */
function browserBuild(request: Request): void {
	calls = [];
	window.localStorage.setItem(
		'devtools.rest',
		JSON.stringify({ projects: [], requests: [request] })
	);
	window.localStorage.setItem('devtools.setting.rest.activeRequest', request.id);
}

function byText(text: string): HTMLButtonElement[] {
	return [...document.querySelectorAll('button')].filter(
		(button) => button.textContent?.trim() === text
	);
}

function byTitle(title: string): HTMLButtonElement | undefined {
	return [...document.querySelectorAll('button')].find(
		(button) => button.getAttribute('title') === title
	);
}

function messages(): string[] {
	return get(toasts).map((item) => item.message);
}

async function until(fn: () => void): Promise<void> {
	let attempts = 0;
	for (;;) {
		try {
			fn();
			return;
		} catch (caught) {
			if (++attempts > 80) throw caught instanceof Error ? caught : new Error(String(caught));
			await new Promise((resolve) => setTimeout(resolve, 10));
		}
	}
}

describe('rest page and global variables', () => {
	const originalClientRects = Range.prototype.getClientRects;
	const originalRect = Range.prototype.getBoundingClientRect;

	beforeAll(() => {
		// jsdom lays out no text, so CodeMirror's measure pass would throw on a Range.
		Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
		Range.prototype.getBoundingClientRect = () => new DOMRect();
	});

	afterAll(() => {
		Range.prototype.getClientRects = originalClientRects;
		Range.prototype.getBoundingClientRect = originalRect;
	});

	beforeEach(() => {
		setLocale('en');
		toasts.set([]);
		delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
		window.localStorage.clear();
		(window as unknown as Record<string, unknown>).matchMedia = () => ({
			matches: false,
			addEventListener: () => {},
			removeEventListener: () => {}
		});
		vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, status: 200 })));
	});

	afterEach(() => {
		try {
			component?.unmount();
		} catch {
			/* the page may already be gone */
		}
		component = undefined;
		document.body.innerHTML = '';
		delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
		window.localStorage.clear();
		vi.unstubAllGlobals();
	});

	async function openPage(): Promise<void> {
		component = mount(RestPage, { target: document.body });
		flushSync();
		await until(() => {
			if (!document.body.textContent?.includes('Tokened')) throw new Error('not loaded');
		});
		flushSync();
	}

	it('says on the Raw tab that a template is expanded only on the desktop', async () => {
		desktopBuild(templatedRequest());
		await openPage();
		expect(document.body.textContent).toContain(say('vars.rawHint'));
	});

	it('names nothing on the Raw tab when the request references no variable', async () => {
		const request = { ...templatedRequest(), headers: [] };
		desktopBuild(request);
		await openPage();
		expect(document.body.textContent).not.toContain(say('vars.rawHint'));
	});

	it('localizes a substitution failure instead of showing the backend code', async () => {
		desktopBuild(templatedRequest());
		executeFailure = 'unknown-var:token';
		await openPage();

		byTitle(say('rest.execute'))?.click();
		flushSync();
		await until(() => {
			expect(messages()).toContain(say('vars.unknown', { name: 'token' }));
		});
		expect(messages().join(' ')).not.toContain('unknown-var:');
	});

	it('refuses to send a templated request in the browser build', async () => {
		browserBuild(templatedRequest());
		await openPage();

		byTitle(say('rest.execute'))?.click();
		flushSync();
		await until(() => {
			expect(messages()).toContain(say('vars.desktopOnlySend'));
		});
		expect(vi.mocked(fetch)).not.toHaveBeenCalled();
	});

	it('refuses to export a templated request as cURL', async () => {
		desktopBuild(templatedRequest());
		await openPage();

		byText('cURL (Posix)')[0].click();
		flushSync();
		await until(() => {
			expect(messages()).toContain(say('vars.curlUnavailable'));
		});
		// Nothing was written out, so the toast stack holds only the refusal.
		expect(get(toasts)).toHaveLength(1);
		expect(calls.some(([command]) => command.startsWith('plugin:clipboard'))).toBe(false);
	});
});
