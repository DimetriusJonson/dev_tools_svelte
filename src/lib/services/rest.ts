import { getSetting, invokeCommand, isTauriRuntime, setSetting } from './db';
import {
	buildMultipartFormData,
	buildRawRequestPreview,
	defaultContentType,
	detectLanguageFromContentType,
	detectLanguageFromHeaders,
	multipartBoundary,
	resolveRequestBody,
	type BodyType,
	type MultipartPreviewOptions,
	type RestFileRow,
	type RestProperty
} from '$lib/utils/rest';

export type { BodyType, RestFileRow, RestProperty };

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
	multipart: RestProperty[];
	files: RestFileRow[];
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
	multipart: RestProperty[];
	files: RestFileRow[];
}

export interface RestRequestUpdate extends RestRequestDraft {
	id: string;
}

/** What the explorer had open: a project (or the no-project view) and a request. */
export interface RestSelection {
	project_id: string | null;
	request_id: string | null;
}

export interface RestExecutionInput {
	method: string;
	url: string;
	headers: RestProperty[];
	body: string;
	body_type: BodyType;
	form: RestProperty[];
	multipart?: RestProperty[];
	files?: RestFileRow[];
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

/**
 * Only the picked path and the display name are persisted. The browser `File` object
 * lives in memory for the session and never reaches storage, where it would serialize
 * as an empty object and then look like a loaded file.
 */
function storedFileRow(row: RestFileRow): RestFileRow {
	return { name: row.name, path: row.path, filename: row.filename };
}

function storedRequest(request: RestRequest): RestRequest {
	return {
		...request,
		headers: request.headers.map((row) => ({ ...row })),
		form: request.form.map((row) => ({ ...row })),
		multipart: (request.multipart ?? []).map((row) => ({ ...row })),
		files: (request.files ?? []).map(storedFileRow),
		response_headers: request.response_headers.map((row) => ({ ...row }))
	};
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
		const rows = (value: unknown): RestProperty[] =>
			Array.isArray(value) ? (value as RestProperty[]) : [];
		const fileRows = (value: unknown): RestFileRow[] =>
			Array.isArray(value)
				? (value as Partial<RestFileRow>[]).map((row) => ({
						name: row?.name ?? '',
						path: row?.path ?? '',
						filename: row?.filename ?? ''
					}))
				: [];
		return {
			projects: Array.isArray(parsed.projects) ? parsed.projects : [],
			requests: Array.isArray(parsed.requests)
				? parsed.requests.map((request) => ({
						...request,
						headers: rows(request.headers),
						form: rows(request.form),
						multipart: rows(request.multipart),
						files: fileRows(request.files),
						response_headers: rows(request.response_headers)
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
		window.localStorage.setItem(
			STORAGE_KEY,
			JSON.stringify({ projects: state.projects, requests: state.requests.map(storedRequest) })
		);
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
		form: Array.isArray(draft.form) ? draft.form : [],
		multipart: Array.isArray(draft.multipart) ? draft.multipart : [],
		files: Array.isArray(draft.files) ? draft.files : []
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
		multipart: (source.multipart ?? []).map((row) => ({ ...row })),
		// A copied row keeps the path only. The desktop backend re-reads it from disk,
		// while the browser build never stores bytes, so the copy needs a re-pick.
		files: (source.files ?? []).map((row) => ({ ...row })),
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

const SELECTION_PROJECT_KEY = 'rest.activeProject';
const SELECTION_REQUEST_KEY = 'rest.activeRequest';

/**
 * Turns a stored selection into one the current data can actually show. Ids of
 * deleted projects or requests are dropped, and the returned project always has the
 * returned request visible in its list — the explorer never restores a row that is
 * filtered out.
 */
export function resolveRestSelection(
	projects: RestProject[],
	requests: RestRequest[],
	stored: RestSelection
): RestSelection {
	const storedProject = projects.some((project) => project.id === stored.project_id)
		? stored.project_id
		: null;
	// `null` is the no-project view, which lists every request, so any row is visible.
	const isVisible = (request: RestRequest) =>
		storedProject === null || request.project_id === storedProject;

	const storedRequest = requests.find((request) => request.id === stored.request_id) ?? null;
	if (storedRequest) {
		if (isVisible(storedRequest)) {
			return { project_id: storedProject, request_id: storedRequest.id };
		}
		// The stored project no longer holds this request: follow the request instead
		// of silently selecting another row.
		const owner = projects.find((project) => project.id === storedRequest.project_id) ?? null;
		return { project_id: owner?.id ?? null, request_id: storedRequest.id };
	}

	const visible = requests.filter(isVisible);
	return { project_id: storedProject, request_id: visible[0]?.id ?? null };
}

export async function loadRestSelection(): Promise<RestSelection> {
	const [project, request] = await Promise.all([
		getSetting(SELECTION_PROJECT_KEY),
		getSetting(SELECTION_REQUEST_KEY)
	]);
	return { project_id: project || null, request_id: request || null };
}

/** An absent selection is stored as an empty string so a cleared row stays cleared. */
export async function saveRestSelection(selection: RestSelection): Promise<void> {
	await Promise.all([
		setSetting(SELECTION_PROJECT_KEY, selection.project_id ?? ''),
		setSetting(SELECTION_REQUEST_KEY, selection.request_id ?? '')
	]);
}

/**
 * The scheme test is anchored: a query may hold `://` (`?next=http://x`) while the
 * URL itself carries no protocol, and handing that to the constructor throws a raw
 * TypeError instead of the localized-shape error the page reports.
 */
export function normalizeUrl(raw: string): URL {
	const trimmed = raw.trim();
	if (!trimmed) throw new Error('Enter a request URL');
	const candidate = /^[a-z][a-z\d+.-]*:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
	let url: URL;
	try {
		url = new URL(candidate);
	} catch {
		throw new Error('Only http and https URLs are supported');
	}
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
 * Text body that goes on the wire: the Form-encode table has priority over the
 * JSON/XML/Text buffer when that tab is active, mirroring the Rust backend. A
 * multipart body is binary and is assembled through `FormData` instead.
 */
function wireBody(input: RestExecutionInput): string {
	return resolveRequestBody({
		body: input.body,
		body_type: input.body_type,
		form: input.form,
		multipart: input.multipart,
		files: input.files
	});
}

/** The raw dump carries a boundary of its own; the browser picks one inside `FormData`. */
function multipartPreview(input: RestExecutionInput): MultipartPreviewOptions {
	return {
		fields: input.multipart ?? [],
		files: input.files ?? [],
		boundary: multipartBoundary()
	};
}

/** The desktop backend reads file parts from disk; the in-memory `File` stays client-side. */
function wireFileRows(input: RestExecutionInput): RestFileRow[] {
	return (input.files ?? []).map(storedFileRow);
}

async function browserFetch(
	input: RestExecutionInput,
	signal: AbortSignal
): Promise<{ response: Response; duration: number }> {
	const url = normalizeUrl(input.url);
	const multipart = input.body_type === 'multipart';
	const body = multipart ? '' : wireBody(input);
	const form = multipart
		? buildMultipartFormData(input.multipart ?? [], input.files ?? [])
		: null;
	if (multipart && !form) {
		throw new Error('Choose the file again: its content is not loaded in this browser session');
	}

	const requestHeaders = new Headers();
	for (const header of input.headers) {
		const name = header.name.trim();
		if (!name) continue;
		const lower = name.toLowerCase();
		if (lower === 'host' || lower === 'content-length') continue;
		// `FormData` sets `multipart/form-data` with the browser's own boundary, so a
		// Content-Type from the table would contradict the body the fetch layer builds.
		if (multipart && lower === 'content-type') continue;
		requestHeaders.set(name, header.value);
	}
	if (!multipart && body && !requestHeaders.has('content-type')) {
		requestHeaders.set('content-type', defaultContentType(input.body_type));
	}
	const started = performance.now();
	let response: Response;
	try {
		response = await fetch(url.toString(), {
			method: input.method.toUpperCase(),
			headers: requestHeaders,
			body: multipart ? form : body || undefined,
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
				input.body_type,
				input.body_type === 'multipart' ? multipartPreview(input) : undefined
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
			form: input.form,
			multipart: input.multipart ?? [],
			files: wireFileRows(input)
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
			multipart: input.multipart ?? [],
			files: wireFileRows(input),
			path
		});
		return;
	}

	const controller = new AbortController();
	browserAbortController = controller;
	try {
		const { response } = await browserFetch(input, controller.signal);
		if (!response.ok) {
			throw new Error(`Server returned ${response.status} ${response.statusText}`);
		}
		const blob = await response.blob();
		const fileUrl = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = fileUrl;
		anchor.download = filename || 'response.bin';
		anchor.click();
		URL.revokeObjectURL(fileUrl);
	} finally {
		if (browserAbortController === controller) browserAbortController = null;
	}
}
