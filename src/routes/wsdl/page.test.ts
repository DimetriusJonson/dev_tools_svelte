import { flushSync, mount } from 'svelte';
import { get } from 'svelte/store';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import WsdlPage from './+page.svelte';
import { setLocale, t, type Translate } from '$lib/stores/i18n';
import { toasts } from '$lib/stores/toast';

interface Property {
	name: string;
	value: string;
}

interface VarRow {
	id: string;
	name: string;
	secure: boolean;
	value: string | null;
}

interface WsdlEndpoint {
	id: string;
	url: string;
	soap_version: string;
	soap_action: string;
	port_name: string;
	binding_name: string;
}

interface WsdlOperation {
	id: string;
	name: string;
	soap_version: string;
	soap_action: string;
	endpoint_count: number;
}

interface WsdlService {
	id: string;
	name: string;
	operations: WsdlOperation[];
}

interface WsdlFileSummary {
	id: string;
	project_id: string;
	name: string;
	source_url: string;
	warnings: string[];
	services: WsdlService[];
	imported_at: string;
}

interface WsdlState {
	endpoint_id: string;
	request_body: string;
	headers: Property[];
	save_response: boolean;
	response_body: string;
	response_headers: Property[];
	raw_request: string;
	response_status_line: string;
}

interface WsdlOperationDetail {
	id: string;
	name: string;
	service_id: string;
	service_name: string;
	soap_version: string;
	soap_action: string;
	input_element: string;
	output_element: string;
	input_template: string;
	endpoints: WsdlEndpoint[];
	state: WsdlState;
}

const PLAIN_VAR: VarRow = {
	id: 'aaaaaaaa-0000-4000-8000-0000000000a1',
	name: 'host',
	secure: false,
	value: 'v1'
};

const SECURE_VAR: VarRow = {
	id: 'aaaaaaaa-0000-4000-8000-0000000000a2',
	name: 'token',
	secure: true,
	value: null
};

const SECRET = 's3cret';

const endpoint: WsdlEndpoint = {
	id: 'ep-1',
	url: 'https://example.com/soap',
	soap_version: 'soap11',
	soap_action: 'urn:demo:echo',
	port_name: 'EchoPort',
	binding_name: 'EchoBinding'
};

function operationDetail(body: string, headers: Property[]): WsdlOperationDetail {
	return {
		id: 'svc::Op',
		name: 'CloseSession',
		service_id: 'svc',
		service_name: 'EchoService',
		soap_version: 'soap11',
		soap_action: 'urn:demo:echo',
		input_element: 'echo',
		output_element: 'echoResponse',
		input_template: body,
		endpoints: [endpoint],
		state: {
			endpoint_id: '',
			request_body: body,
			headers,
			save_response: false,
			response_body: '',
			response_headers: [],
			raw_request: '',
			response_status_line: ''
		}
	};
}

function wsdlFile(headers: Property[], body: string): WsdlFileSummary {
	return {
		id: 'file-1',
		project_id: 'p-1',
		name: 'echo.wsdl',
		source_url: 'http://127.0.0.1:9000/echo?wsdl',
		warnings: [],
		services: [{ id: 'svc', name: 'EchoService', operations: [{ id: 'svc::Op', name: 'CloseSession', soap_version: 'soap11', soap_action: 'urn:demo:echo', endpoint_count: 1 }] }],
		imported_at: '2026-11-22 10:00:00'
	};
}

let calls: Array<[string, Record<string, unknown>]>;
let written: string[];
let component: ReturnType<typeof mount> | undefined;

const say: Translate = (key, params) => get(t)(key, params);

/** A Rust command rejects with its error **string**, not an `Error`. */
function failBackend(text: string): never {
	throw text;
}

function desktopBuild(rows: VarRow[], detail: WsdlOperationDetail): void {
	calls = [];
	written = [];
	(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
		invoke: async (command: string, args: Record<string, unknown> = {}) => {
			calls.push([command, args]);
			if (command === 'list_wsdl_projects') return [];
			if (command === 'list_wsdl_files') return [wsdlFile(detail.state.headers, detail.state.request_body)];
			if (command === 'load_wsdl_selection') return { project_id: '', operation_id: 'svc::Op' };
			if (command === 'get_wsdl_operation') return detail;
			if (command === 'list_global_vars') return rows;
			if (command === 'reveal_global_var') {
				if (detail.state.request_body.includes('boom')) failBackend('secret-store:boom');
				return SECRET;
			}
			return null;
		}
	};
}

/** The browser build: no Tauri wire, so the page renders the desktop stub. */
function browserBuild(): void {
	calls = [];
	written = [];
	delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
}

function byText(text: string): HTMLButtonElement[] {
	return [...document.querySelectorAll('button')].filter((button) => button.textContent?.trim() === text);
}

function byTitle(title: string): HTMLButtonElement[] {
	return [...document.querySelectorAll('button')].filter((button) => button.getAttribute('title') === title);
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

describe('wsdl page cURL export', () => {
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
		written = [];
		delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
		window.localStorage.clear();
		(window as unknown as Record<string, unknown>).matchMedia = () => ({
			matches: false,
			addEventListener: () => {},
			removeEventListener: () => {}
		});
		// `copyText` prefers the async clipboard whenever the context is secure.
		Object.defineProperty(window, 'isSecureContext', { value: true, configurable: true });
		Object.defineProperty(navigator, 'clipboard', {
			value: { writeText: async (text: string) => written.push(text) },
			configurable: true
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

	async function openPage(detail: WsdlOperationDetail): Promise<void> {
		component = mount(WsdlPage, { target: document.body });
		flushSync();
		await until(() => {
			if (!document.body.textContent?.includes('CloseSession')) throw new Error('not loaded');
		});
		flushSync();
		// The export buttons live on the request `Raw` tab.
		byText(say('wsdl.tabRaw'))[0]?.click();
		flushSync();
	}

	it('renders the cURL buttons on the desktop Raw tab', async () => {
		const detail = operationDetail('<echo>hi</echo>', []);
		desktopBuild([PLAIN_VAR], detail);
		await openPage(detail);

		expect(byTitle(say('rest.curlMenu'))).toHaveLength(1);
		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		expect(byText(say('rest.copyCurlPosix'))).toHaveLength(1);
		expect(byText(say('rest.copyCurlWindows'))).toHaveLength(1);
	});

	it('renders no cURL buttons in the browser build', async () => {
		browserBuild();
		component = mount(WsdlPage, { target: document.body });
		flushSync();
		await until(() => {
			if (!document.body.textContent?.includes(say('wsdl.desktopOnly'))) throw new Error('stub not rendered');
		});
		expect(byTitle(say('rest.curlMenu'))).toHaveLength(0);
		expect(byText(say('rest.copyCurlPosix'))).toHaveLength(0);
	});

	it('copies a cURL command with plain variables substituted', async () => {
		const detail = operationDetail('<echo>{{ host }}</echo>', [{ name: 'X-Host', value: '{{ host }}' }]);
		desktopBuild([PLAIN_VAR], detail);
		await openPage(detail);

		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		byText(say('rest.copyCurlPosix'))[0]?.click();
		flushSync();
		await until(() => {
			expect(written).toHaveLength(1);
		});
		expect(written[0]).toContain('v1');
		expect(written[0]).not.toContain('{{');
		// No safe variable was referenced, so nothing was asked and nothing revealed.
		expect(document.body.textContent).not.toContain(say('vars.secureWarning'));
		expect(calls.some(([command]) => command === 'reveal_global_var')).toBe(false);
		expect(messages()).toEqual([say('common.copied')]);
	});

	it('asks about safe templates and replaces them with the real value', async () => {
		const detail = operationDetail('<echo>{{ token }}</echo>', []);
		desktopBuild([PLAIN_VAR, SECURE_VAR], detail);
		await openPage(detail);

		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		byText(say('rest.copyCurlPosix'))[0]?.click();
		flushSync();
		await until(() => {
			expect(document.body.textContent).toContain(say('vars.secureWarning'));
		});
		expect(written).toHaveLength(0);

		byText(say('vars.replace'))[0].click();
		flushSync();
		await until(() => {
			expect(written).toHaveLength(1);
		});
		expect(written[0]).toContain(SECRET);
		expect(calls.some(([command]) => command === 'reveal_global_var')).toBe(true);
		expect(document.body.textContent).not.toContain(say('vars.secureWarning'));
	});

	it('keeps the safe template as text when the answer is "do not replace"', async () => {
		const detail = operationDetail('<echo>{{ token }}</echo>', []);
		desktopBuild([PLAIN_VAR, SECURE_VAR], detail);
		await openPage(detail);

		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		byText(say('rest.copyCurlWindows'))[0]?.click();
		flushSync();
		await until(() => {
			expect(document.body.textContent).toContain(say('vars.secureWarning'));
		});

		byText(say('vars.keepTemplates'))[0].click();
		flushSync();
		await until(() => {
			expect(written).toHaveLength(1);
		});
		expect(written[0]).toContain('{{ token }}');
		expect(written[0]).not.toContain(SECRET);
		expect(calls.some(([command]) => command === 'reveal_global_var')).toBe(false);
	});

	it('copies nothing when the safe-variable question is cancelled', async () => {
		const detail = operationDetail('<echo>{{ token }}</echo>', []);
		desktopBuild([SECURE_VAR], detail);
		await openPage(detail);

		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		byText(say('rest.copyCurlPosix'))[0]?.click();
		flushSync();
		await until(() => {
			expect(document.body.textContent).toContain(say('vars.secureWarning'));
		});

		byText(say('common.cancel'))[0].click();
		flushSync();
		expect(written).toHaveLength(0);
		expect(document.body.textContent).not.toContain(say('vars.secureWarning'));
		expect(get(toasts)).toHaveLength(0);
	});

	it('blocks the export when no endpoint is picked', async () => {
		const detail = operationDetail('<echo>{{ host }}</echo>', []);
		detail.endpoints = [];
		desktopBuild([PLAIN_VAR], detail);
		await openPage(detail);

		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		byText(say('rest.copyCurlPosix'))[0]?.click();
		flushSync();
		await until(() => {
			expect(messages()).toContain(say('wsdl.noEndpoint'));
		});
		expect(written).toHaveLength(0);
	});

	it('localizes a safe-value failure instead of showing the backend code', async () => {
		const detail = operationDetail('<echo>{{ token }} boom</echo>', []);
		desktopBuild([SECURE_VAR], detail);
		await openPage(detail);

		byTitle(say('rest.curlMenu'))[0]?.click();
		flushSync();
		byText(say('rest.copyCurlPosix'))[0]?.click();
		flushSync();
		await until(() => {
			expect(document.body.textContent).toContain(say('vars.secureWarning'));
		});

		byText(say('vars.replace'))[0].click();
		flushSync();
		await until(() => {
			expect(messages()).toContain(say('vars.secretStore', { message: 'boom' }));
		});
		expect(messages().join(' ')).not.toContain('secret-store:');
		expect(written).toHaveLength(0);
	});
});
