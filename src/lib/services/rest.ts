import { invokeCommand, isTauriRuntime } from './db';
import {
	buildRawRequestPreview,
	defaultContentType,
	detectLanguageFromContentType,
	detectLanguageFromHeaders,
	resolveRequestBody,
	type BodyType,
	type RestProperty
} from '$lib/utils/rest';

export type { BodyType, RestProperty };

export interface RestProject {
	id: string;
	name: string;
	created_at: string;
}

export interface RestRequest {
	id: string;
	project_id: string | null;
	name: string;
	method: string;
	url: string;
	body: string;
	body_type: BodyType;
	save_response: boolean;
	headers: RestProperty[];
	form: RestProperty[];
	response_body: string | null;
	response_headers: RestProperty[];
	raw_request: string | null;
	created_at: string;
	updated_at: string;
}

export interface RestRequestDraft {
	project_id: string | null;
	name: string;
	method: string;
	url: string;
	body: string;
	body_type: BodyType;
	save_response: boolean;
	headers: RestProperty[];
	form: RestProperty[];
}

export interface RestRequestUpdate extends RestRequestDraft {
	id: string;
}

export interface RestExecutionInput {
	method: string;
	url: string;
	headers: RestProperty[];
	body: string;
	body_type: BodyType;
	form: RestProperty[];
}

export interface RestResponse {
	status: number;
	status_text: string;
	headers: RestProperty[];
	body: string;
	size: number;
	duration_ms: number;
	content_type: string;
	language: 'json' | 'xml' | 'html' | 'text' | null;
	is_attachment: boolean;
	filename: string | null;
	is_binary: boolean;
	raw_request: string;
}

const STORAGE_KEY = 'devtools.rest';
const DEFAULT_METHOD = 'GET';
const DEFAULT_BODY_TYPE: BodyType = 'json';

interface LocalState {
	projects: RestProject[];
	requests: RestRequest[];
}

function newId(): string {
	if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
		return crypto.randomUUID();
	}
	return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (char) => {
		const random = (Math.random() * 16) | 0;
		const value = char === 'x' ? random : (random & 0x3) | 0x8;
		return value.toString(16);
	});
}

function timestamp(): string {
	return new Date().toISOString().replace('T', ' ').slice(0, 19);
}

function loadLocal(): LocalState {
	if (typeof window === 'undefined') return { projects: [], requests: [] };
	try {
		const raw = window.localStorage.getItem(STORAGE_KEY);
		if (!raw) return { projects: [], requests: [] };
		const parsed = JSON.parse(raw) as Partial<LocalState>;
		return {
			projects: Array.isArray(parsed.projects) ? parsed.projects : [],
			requests: Array.isArray(parsed.requests)
				? parsed.requests.map((request) => ({
						...request,
						headers: Array.isArray(request.headers) ? request.headers : [],
						form: Array.isArray(request.form) ? request.form : [],
						response_headers: Array.isArray(request.response_headers)
							? request.response_headers
							: []
					}))
				: []
		};
	} catch {
		return { projects: [], requests: [] };
	}
}

function saveLocal(state: LocalState): void {
	if (typeof window === 'undefined') return;
	try {
		window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
	} catch {
		/* storage can be unavailable */
	}
}

function normalizeDraft(draft: Partial<RestRequestDraft>): RestRequestDraft {
	return {
		project_id: draft.project_id ?? null,
		name: draft.name ?? '',
		method: (draft.method ?? DEFAULT_METHOD).toUpperCase(),
		url: draft.url ?? '',
		body: draft.body ?? '',
		body_type: draft.body_type ?? DEFAULT_BODY_TYPE,
		save_response: draft.save_response ?? false,
		headers: Array.isArray(draft.headers) ? draft.headers : [],
		form: Array.isArray(draft.form) ? draft.form : []
	};
}

function localCreateRequest(draft: RestRequestDraft): RestRequest {
	const state = loadLocal();
	const now = timestamp();
	const normalized = normalizeDraft(draft);
	const request: RestRequest = {
		id: newId(),
		...normalized,
		response_body: null,
		response_headers: [],
		raw_request: null,
		created_at: now,
		updated_at: now
	};
	state.requests.push(request);
	saveLocal(state);
	return request;
}

export async function listRestProjects(): Promise<RestProject[]> {
	if (isTauriRuntime()) return invokeCommand<RestProject[]>('list_rest_projects');
	return loadLocal().projects;
}

export async function createRestProject(name: string): Promise<RestProject> {
	if (isTauriRuntime()) return invokeCommand<RestProject>('create_rest_project', { name });
	const state = loadLocal();
	const project: RestProject = { id: newId(), name, created_at: timestamp() };
	state.projects.push(project);
	saveLocal(state);
	return project;
}

export async function renameRestProject(id: string, name: string): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('rename_rest_project', { id, name });
		return;
	}
	const state = loadLocal();
	state.projects = state.projects.map((project) =>
		project.id === id ? { ...project, name } : project
	);
	saveLocal(state);
}

export async function deleteRestProject(id: string): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('delete_rest_project', { id });
		return;
	}
	const state = loadLocal();
	state.projects = state.projects.filter((project) => project.id !== id);
	state.requests = state.requests.map((request) =>
		request.project_id === id ? { ...request, project_id: null } : request
	);
	saveLocal(state);
}

export async function listRestRequests(): Promise<RestRequest[]> {
	if (isTauriRuntime()) return invokeCommand<RestRequest[]>('list_rest_requests');
	return loadLocal().requests;
}

export async function createRestRequest(draft: RestRequestDraft): Promise<RestRequest> {
	if (isTauriRuntime()) return invokeCommand<RestRequest>('create_rest_request', { draft });
	return localCreateRequest(draft);
}

export async function updateRestRequest(request: RestRequestUpdate): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('update_rest_request', { request });
		return;
	}
	const state = loadLocal();
	state.requests = state.requests.map((item) =>
		item.id === request.id
			? { ...item, ...request, updated_at: timestamp() }
			: item
	);
	saveLocal(state);
}

export async function renameRestRequest(id: string, name: string): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('rename_rest_request', { id, name });
		return;
	}
	const state = loadLocal();
	state.requests = state.requests.map((request) =>
		request.id === id ? { ...request, name, updated_at: timestamp() } : request
	);
	saveLocal(state);
}

export async function deleteRestRequest(id: string): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('delete_rest_request', { id });
		return;
	}
	const state = loadLocal();
	state.requests = state.requests.filter((request) => request.id !== id);
	saveLocal(state);
}

export async function duplicateRestRequest(id: string): Promise<RestRequest> {
	if (isTauriRuntime()) {
		return invokeCommand<RestRequest>('duplicate_rest_request', { id });
	}
	const state = loadLocal();
	const source = state.requests.find((request) => request.id === id);
	if (!source) throw new Error('Request not found');
	const now = timestamp();
	const copy: RestRequest = {
		...source,
		id: newId(),
		name: source.name.trim() ? `${source.name} copy` : source.url,
		headers: source.headers.map((header) => ({ ...header })),
		form: source.form.map((row) => ({ ...row })),
		response_body: null,
		response_headers: [],
		raw_request: null,
		created_at: now,
		updated_at: now
	};
	state.requests.push(copy);
	saveLocal(state);
	return copy;
}

export async function saveRestResponse(
	id: string,
	body: string,
	headers: RestProperty[],
	rawRequest: string
): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('save_rest_response', {
			id,
			body,
			headers,
			raw_request: rawRequest
		});
		return;
	}
	const state = loadLocal();
	state.requests = state.requests.map((request) =>
		request.id === id
			? {
					...request,
					response_body: body,
					response_headers: headers,
					raw_request: rawRequest,
					updated_at: timestamp()
				}
			: request
	);
	saveLocal(state);
}

export async function clearRestResponse(id: string): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('clear_rest_response', { id });
		return;
	}
	const state = loadLocal();
	state.requests = state.requests.map((request) =>
		request.id === id
			? { ...request, response_body: null, response_headers: [], raw_request: null }
			: request
	);
	saveLocal(state);
}

function normalizeUrl(raw: string): URL {
	const trimmed = raw.trim();
	if (!trimmed) throw new Error('Enter a request URL');
	const url = new URL(trimmed.includes('://') ? trimmed : `http://${trimmed}`);
	if (url.protocol !== 'http:' && url.protocol !== 'https:') {
		throw new Error('Only http and https URLs are supported');
	}
	return url;
}

let browserAbortController: AbortController | null = null;

function filenameFromDisposition(value: string): string | null {
	const star = value.match(/filename\*=([^;]+)/i);
	if (star) {
		const raw = star[1].trim().replace(/^UTF-8''/i, '').replace(/^"|"$/g, '');
		try {
			return decodeURIComponent(raw);
		} catch {
			return raw;
		}
	}
	const plain = value.match(/filename=([^;]+)/i);
	if (plain) return plain[1].trim().replace(/^"|"$/g, '') || null;
	return null;
}

function isTextContentType(contentType: string): boolean {
	return contentType.trim() === '' || detectLanguageFromContentType(contentType) !== null;
}

/**
 * Body that actually goes on the wire: the Form-encode table has priority over the
 * JSON/XML/Text buffer when that tab is active, mirroring the Rust backend.
 */
function wireBody(input: RestExecutionInput): string {
	return resolveRequestBody({
		body: input.body,
		body_type: input.body_type,
		form: input.form
	});
}

async function browserFetch(
	input: RestExecutionInput,
	signal: AbortSignal
): Promise<{ response: Response; duration: number }> {
	const url = normalizeUrl(input.url);
	const body = wireBody(input);
	const requestHeaders = new Headers();
	for (const header of input.headers) {
		const name = header.name.trim();
		if (!name) continue;
		const lower = name.toLowerCase();
		if (lower === 'host' || lower === 'content-length') continue;
		requestHeaders.set(name, header.value);
	}
	if (body && !requestHeaders.has('content-type')) {
		requestHeaders.set('content-type', defaultContentType(input.body_type));
	}
	const started = performance.now();
	let response: Response;
	try {
		response = await fetch(url.toString(), {
			method: input.method.toUpperCase(),
			headers: requestHeaders,
			body: body || undefined,
			signal,
			redirect: 'follow'
		});
	} catch (error) {
		if (error instanceof DOMException && error.name === 'AbortError') {
			throw new Error('Request aborted');
		}
		throw new Error(`Request failed: ${error instanceof Error ? error.message : String(error)}`);
	}
	return { response, duration: performance.now() - started };
}

function headersFromResponse(response: Response): RestProperty[] {
	const headers: RestProperty[] = [];
	response.headers.forEach((value, name) => headers.push({ name, value }));
	return headers;
}

async function browserExecute(input: RestExecutionInput): Promise<RestResponse> {
	const controller = new AbortController();
	browserAbortController = controller;
	try {
		const { response, duration } = await browserFetch(input, controller.signal);
		const headers = headersFromResponse(response);
		const contentType = response.headers.get('content-type') ?? '';
		const disposition = response.headers.get('content-disposition') ?? '';
		const isAttachment = disposition.toLowerCase().includes('attachment');
		const filename = filenameFromDisposition(disposition);
		const declaredLength = Number.parseInt(response.headers.get('content-length') ?? '', 10);

		let body = '';
		let size = Number.isFinite(declaredLength) ? declaredLength : 0;
		let isBinary = false;
		let language = detectLanguageFromContentType(contentType);

		if (!isAttachment && isTextContentType(contentType)) {
			body = await response.text();
			size = new TextEncoder().encode(body).length;
			language = detectLanguageFromHeaders(headers, body);
		} else if (!isAttachment) {
			isBinary = true;
			language = null;
		}

		if (isAttachment) {
			isBinary = true;
			language = null;
		}

		return {
			status: response.status,
			status_text: response.statusText,
			headers,
			body,
			size,
			duration_ms: duration,
			content_type: contentType,
			language,
			is_attachment: isAttachment,
			filename,
			is_binary: isBinary,
			raw_request: buildRawRequestPreview(
				input.method,
				input.url,
				input.headers,
				wireBody(input),
				input.body_type
			)
		};
	} finally {
		if (browserAbortController === controller) browserAbortController = null;
	}
}

export async function executeRestRequest(input: RestExecutionInput): Promise<RestResponse> {
	if (isTauriRuntime()) {
		return invokeCommand<RestResponse>('execute_rest_request', {
			method: input.method,
			url: input.url,
			headers: input.headers,
			body: input.body,
			body_type: input.body_type,
			form: input.form
		});
	}
	return browserExecute(input);
}

export async function cancelRestRequest(): Promise<boolean> {
	if (isTauriRuntime()) return invokeCommand<boolean>('cancel_rest_request');
	if (!browserAbortController) return false;
	browserAbortController.abort();
	browserAbortController = null;
	return true;
}

export async function downloadRestResponse(
	input: RestExecutionInput,
	path: string,
	filename: string
): Promise<void> {
	if (isTauriRuntime()) {
		await invokeCommand('download_rest_response', {
			method: input.method,
			url: input.url,
			headers: input.headers,
			body: input.body,
			body_type: input.body_type,
			form: input.form,
			path
		});
		return;
	}

	const controller = new AbortController();
	const { response } = await browserFetch(input, controller.signal);
	if (!response.ok) {
		throw new Error(`Server returned ${response.status} ${response.statusText}`);
	}
	const blob = await response.blob();
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = filename || 'response.bin';
	anchor.click();
	URL.revokeObjectURL(url);
}
