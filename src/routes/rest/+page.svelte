<script lang="ts">
	import { onMount } from 'svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import PropertyEditor from '$lib/components/PropertyEditor.svelte';
	import PropertyFilesEditor from '$lib/components/PropertyFilesEditor.svelte';
	import RestExplorer from '$lib/components/RestExplorer.svelte';
	import { t } from '$lib/stores/i18n';
	import { isDark } from '$lib/stores/theme';
	import { notify, notifyError } from '$lib/stores/toast';
	import { copyText, readClipboardText } from '$lib/utils/clipboard';
	import { formatContent } from '$lib/utils/formatters';
	import { pickSaveFile, sharedFileName } from '$lib/services/files';
	import { isTauriRuntime } from '$lib/services/db';
	import {
		clearRestResponse,
		createRestProject,
		createRestRequest,
		deleteRestProject,
		deleteRestRequest,
		duplicateRestRequest,
		executeRestRequest,
		cancelRestRequest,
		downloadRestResponse,
		listRestProjects,
		listRestRequests,
		loadRestSelection,
		renameRestProject,
		renameRestRequest,
		resolveRestSelection,
		saveRestResponse,
		saveRestSelection,
		updateRestRequest,
		type BodyType,
		type RestProject,
		type RestRequest,
		type RestResponse
	} from '$lib/services/rest';
	import {
		buildCurl,
		buildRawRequestPreview,
		buildRawResponse,
		byteLength,
		detectLanguageFromHeaders,
		filenameFromDisposition,
		formatRestSize,
		formatRestTime,
		getHeader,
		multipartBoundary,
		parseCookies,
		parseCurl,
		parseQuery,
		resolveRequestBody,
		setCookieRows,
		updateUrlQuery,
		validateFileRow,
		validateFileRows,
		validateProperties,
		validateQueryUrl,
		validateRequest,
		validateRowName,
		type CurlShell,
		type PropertyKind,
		type RequestValidationSource,
		type RestFileRow,
		type RestProperty,
		type ValidationIssue
	} from '$lib/utils/rest';
	import {
		blockedMessage,
		propertyFieldHint,
		validationMessage
	} from '$lib/utils/validation';

	type RequestTab = 'headers' | 'query' | 'cookies' | 'raw';
	type ResponseTab = 'response' | 'headers' | 'raw';
	type MultipartTab = 'fields' | 'files';

	const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];
	const BODY_TYPES: BodyType[] = ['json', 'xml', 'text', 'form', 'multipart'];
	const MULTIPART_TABS: MultipartTab[] = ['fields', 'files'];
	// The raw preview needs a boundary that does not change on every keystroke. The
	// request itself always gets a fresh one from the encoder, so this marker is only
	// the layout hint shown before a run.
	const previewBoundary = multipartBoundary();

	let projects = $state<RestProject[]>([]);
	let requests = $state<RestRequest[]>([]);
	let activeProjectId = $state<string | null>(null);
	let activeId = $state<string | null>(null);
	let active = $state<RestRequest | null>(null);
	let hydrated = $state(false);
	let loading = $state(true);
	let running = $state(false);
	let aborting = $state(false);
	let requestTab = $state<RequestTab>('headers');
	let multipartTab = $state<MultipartTab>('fields');
	let responseTab = $state<ResponseTab>('response');
	let response = $state<RestResponse | null>(null);
	let responseText = $state('');
	let previewEnabled = $state(false);
	let downloading = $state(false);
	let explorerWidth = $state(280);
	let requestWidth = $state(600);
	let topHeight = $state(340);
	let resizingExplorer = $state(false);
	let resizingSplit = $state(false);
	let resizingHeight = $state(false);
	let saveTimer: ReturnType<typeof setTimeout> | undefined;

	const dark = $derived($isDark);
	const filteredRequests = $derived(
		activeProjectId
			? requests.filter((request) => request.project_id === activeProjectId)
			: requests
	);
	const queryRows = $derived(active ? parseQuery(active.url) : []);
	const cookieRows = $derived(active ? parseCookies(active.headers) : []);
	const effectiveBody = $derived(active ? resolveRequestBody(active) : '');
	const savedResponseHeaders = $derived(active?.response_headers ?? []);
	const responseHeaders = $derived(response?.headers ?? savedResponseHeaders);
	const savedResponseBody = $derived(active?.response_body ?? '');
	const responseLanguage = $derived(
		response?.language ?? detectLanguageFromHeaders(savedResponseHeaders, savedResponseBody)
	);
	const responseContentType = $derived(
		response?.content_type ?? getHeader(responseHeaders, 'content-type')
	);
	const attachmentHeader = $derived(getHeader(responseHeaders, 'content-disposition'));
	const attachmentName = $derived(
		response?.filename ?? filenameFromDisposition(attachmentHeader)
	);
	const isAttachment = $derived(
		Boolean(response?.is_attachment) || attachmentHeader.toLowerCase().includes('attachment')
	);
	const isBinary = $derived(
		Boolean(response?.is_binary) ||
			(!responseLanguage && responseContentType.length > 0 && !isAttachment)
	);
	const hasResponseText = $derived(responseText.length > 0);
	const canFormatResponse = $derived(
		responseLanguage === 'json' || responseLanguage === 'xml'
	);
	const canPreview = $derived(responseLanguage === 'html' && hasResponseText);
	const showSaveCheckbox = $derived(!isAttachment && !isBinary);
	const rawRequestText = $derived(
		response?.raw_request ??
			active?.raw_request ??
			(active
				? buildRawRequestPreview(
						active.method,
						active.url,
						active.headers,
						effectiveBody,
						active.body_type,
						{ fields: active.multipart, files: active.files, boundary: previewBoundary }
					)
				: '')
	);
	const requestStatus = $derived(response?.status ?? null);
	const responseStatusLine = $derived(
		response?.status_line ?? active?.response_status_line ?? ''
	);
	// The bytes as they arrived: never the formatted mirror, so `Raw` stays comparable
	// with what the server actually sent, including for a response read back from SQLite.
	const rawResponseText = $derived(
		buildRawResponse(responseStatusLine, responseHeaders, response?.body ?? savedResponseBody)
	);
	const requestSize = $derived(
		response ? response.size : savedResponseBody ? byteLength(savedResponseBody) : 0
	);
	const requestTime = $derived(response?.duration_ms ?? null);

	function cloneRequest(request: RestRequest): RestRequest {
		return {
			...request,
			headers: request.headers.map((header) => ({ ...header })),
			form: request.form.map((row) => ({ ...row })),
			multipart: request.multipart.map((row) => ({ ...row })),
			// The picked `File` object is runtime state: a clone must keep it, or the
			// browser build could not send the part it just picked.
			files: request.files.map((row) => ({ ...row })),
			response_headers: request.response_headers.map((header) => ({ ...header }))
		};
	}

	function selectRequest(id: string): void {
		if (active && active.id !== id) void flushSave(active);
		const found = requests.find((request) => request.id === id);
		if (!found) return;
		activeId = id;
		active = cloneRequest(found);
		response = null;
		responseText = found.response_body ?? '';
		previewEnabled = false;
		requestTab = 'headers';
		multipartTab = 'fields';
		responseTab = 'response';
	}

	function selectProject(id: string | null): void {
		activeProjectId = id;
		if (id && active && active.project_id !== id) {
			void flushSave(active);
			activeId = null;
			active = null;
			response = null;
			responseText = '';
		}
	}

	// Non-reactive: it records what is already persisted so the effect below does not
	// rewrite the same selection after the initial restore.
	let savedSelection = '';

	function selectionKey(projectId: string | null, requestId: string | null): string {
		return `${projectId ?? ''}|${requestId ?? ''}`;
	}

	// The explorer selection outlives the session: it goes to SQLite (and to
	// localStorage in the browser build) so the app reopens on the same project and request.
	$effect(() => {
		if (!hydrated) return;
		const key = selectionKey(activeProjectId, activeId);
		if (key === savedSelection) return;
		savedSelection = key;
		void saveRestSelection({ project_id: activeProjectId, request_id: activeId });
	});

	function toUpdate(request: RestRequest) {
		return {
			id: request.id,
			project_id: request.project_id,
			name: request.name,
			method: request.method,
			url: request.url,
			body: request.body,
			body_type: request.body_type,
			save_response: request.save_response,
			headers: request.headers,
			form: request.form,
			multipart: request.multipart,
			// Storage keeps the path and the display name only; the in-memory `File`
			// belongs to this session and would serialize as an empty object.
			files: request.files.map(({ name, path, filename }) => ({ name, path, filename }))
		};
	}

	async function flushSave(target: RestRequest | null = active): Promise<void> {
		if (saveTimer) {
			clearTimeout(saveTimer);
			saveTimer = undefined;
		}
		if (!target) return;
		try {
			await updateRestRequest(toUpdate(target));
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	function scheduleSave(): void {
		if (!hydrated || !active) return;
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			saveTimer = undefined;
			void flushSave();
		}, 450);
	}

	function patchActive(patch: Partial<RestRequest>): void {
		if (!active) return;
		const next = { ...active, ...patch };
		active = next;
		// Each body tab owns its table, so the merged list row always takes the current
		// tables and buffer from `next` rather than a patch that mentions only some.
		requests = requests.map((request) =>
			request.id === next.id
				? {
						...request,
						...patch,
						body: next.body,
						headers: next.headers,
						form: next.form,
						multipart: next.multipart,
						files: next.files
					}
				: request
		);
		scheduleSave();
	}

	function setBodyType(type: BodyType): void {
		if (!active || active.body_type === type) return;
		// The Form-encode table has its own storage, so switching tabs never copies
		// rows into the JSON/XML/Text buffer or parses that buffer into rows.
		patchActive({ body_type: type });
	}

	function onHeadersChange(rows: RestProperty[]): void {
		patchActive({ headers: rows });
	}

	function onQueryChange(rows: RestProperty[]): void {
		if (!active) return;
		patchActive({ url: updateUrlQuery(active.url, rows) });
	}

	function onCookiesChange(rows: RestProperty[]): void {
		if (!active) return;
		patchActive({ headers: setCookieRows(active.headers, rows) });
	}

	function onFormChange(rows: RestProperty[]): void {
		patchActive({ form: rows });
	}

	function onMultipartFieldChange(rows: RestProperty[]): void {
		patchActive({ multipart: rows });
	}

	function onMultipartFileChange(rows: RestFileRow[]): void {
		patchActive({ files: rows });
	}

	/** Cell-level hints for PropertyEditor: a localized reason, or `null` when valid. */
	function hintFor(kind: PropertyKind) {
		return (field: 'name' | 'value', row: RestProperty, index: number): string | null => {
			// Query rows are decoded, so the raw URL decides which row is at fault.
			if (kind === 'query') {
				const issue = validateQueryUrl(active?.url ?? '').find(
					(item) => item.row === index && item.field === field
				);
				return issue ? validationMessage(issue, $t) : null;
			}
			// A name is mandatory: the row is dropped on persistence and on the wire, so a
			// value with a blank name would silently never be sent. `propertyFieldHint`
			// checks that first, and it also covers the add-row draft (`index === -1`).
			return propertyFieldHint(kind, field, row, index, $t);
		};
	}

	/**
	 * Cell-level hints for the file table. `value` is the file cell: a row that kept a
	 * filename after a reload has no bytes left, and that is what this explains.
	 */
	function fileHint(field: 'name' | 'value', row: RestFileRow, index: number): string | null {
		const issue = validateFileRow(row, index).find((item) => item.field === field);
		return issue ? validationMessage(issue, $t) : null;
	}

	/** Message that explains why a request cannot be sent, or `null` when it is valid. */
	function validationFailure(request: RequestValidationSource): string | null {
		return blockedMessage(validateRequest(request), $t);
	}

	const headerIssues = $derived(active ? validateProperties('header', active.headers) : []);
	const queryIssues = $derived(active ? validateQueryUrl(active.url) : []);
	const cookieIssues = $derived(
		active ? validateProperties('cookie', parseCookies(active.headers)) : []
	);
	const formIssues = $derived(
		active && active.body_type === 'form' ? validateProperties('form', active.form) : []
	);
	const multipartFieldIssues = $derived(
		active && active.body_type === 'multipart' ? validateProperties('multipart', active.multipart) : []
	);
	const multipartFileIssues = $derived(
		active && active.body_type === 'multipart' ? validateFileRows(active.files) : []
	);
	// The Form tab is a container for two tables, so it is marked when either one fails.
	const multipartIssues = $derived([...multipartFieldIssues, ...multipartFileIssues]);
	// Which issues a body tab reports; tabs with no table never have any.
	const bodyIssues = $derived<Record<BodyType, ValidationIssue[]>>({
		json: [],
		xml: [],
		text: [],
		form: formIssues,
		multipart: multipartIssues
	});
	// The URL field is the source of the query rows, so it carries the same error.
	const urlClass = $derived(
		'h-11 min-w-0 flex-1 rounded-lg border bg-[var(--panel-bg)] px-3 font-mono text-sm text-[var(--text)] outline-none transition-colors duration-150 placeholder:text-[var(--text-muted)] disabled:opacity-50 ' +
			(queryIssues.length
				? 'border-[var(--danger)] focus-visible:ring-2 focus-visible:ring-[var(--danger)]'
				: 'border-[var(--border-strong)] hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]')
	);

	function formatBody(): void {
		if (!active) return;
		if (active.body_type !== 'json' && active.body_type !== 'xml') return;
		try {
			patchActive({
				body: formatContent(active.body_type, active.body, {
					indent: 4,
					doFormat: true,
					doUnescape: false
				})
			});
		} catch {
			notify($t('formatter.invalid'), 'error');
		}
	}

	function transformResponse(kind: 'format' | 'unescape'): void {
		if (responseLanguage !== 'json' && responseLanguage !== 'xml') return;
		try {
			responseText = formatContent(responseLanguage, responseText, {
				indent: 4,
				doFormat: kind === 'format',
				doUnescape: kind === 'unescape'
			});
		} catch {
			notify($t('formatter.invalid'), 'error');
		}
	}

	async function copyResponse(): Promise<void> {
		if (responseText) await copyText(responseText);
	}

	async function copyRawResponse(): Promise<void> {
		if (rawResponseText) await copyText(rawResponseText);
	}

	async function runRequest(): Promise<void> {
		if (running) {
			aborting = true;
			try {
				await cancelRestRequest();
			} catch (error) {
				aborting = false;
				notifyError(error, $t('common.error'));
			}
			return;
		}
		if (!active) return;
		if (!active.url.trim()) {
			notify($t('rest.urlRequired'), 'error');
			return;
		}
		// Rows that break RFC 9110/3986 never reach the network; they stay highlighted.
		const failure = validationFailure(active);
		if (failure) {
			notify(failure, 'error');
			return;
		}

		const target = active;
		void flushSave(target);
		running = true;
		aborting = false;
		previewEnabled = false;

		try {
			const result = await executeRestRequest({
				method: target.method,
				url: target.url,
				headers: target.headers,
				body: target.body,
				body_type: target.body_type,
				// Form-encode rows win over the text body when that tab is active, and
				// the Form tables replace it entirely when the Form tab is active.
				form: target.form,
				multipart: target.multipart,
				files: target.files
			});
			if (activeId === target.id) {
				response = result;
				responseText = result.body;
				responseTab = 'response';
			}
			if (target.save_response && isSavableResponse(result)) {
				await persistResponse(target, result);
			}
		} catch (error) {
			if (aborting || String(error).toLowerCase().includes('aborted')) {
				notify($t('rest.aborted'), 'info');
			} else {
				notifyError(error, $t('rest.requestFailed'));
			}
		} finally {
			running = false;
			aborting = false;
		}
	}

	function isSavableResponse(result: RestResponse): boolean {
		return !result.is_attachment && !result.is_binary && result.language !== null;
	}

	async function persistResponse(target: RestRequest, result: RestResponse): Promise<void> {
		const patch = {
			response_body: result.body,
			response_headers: result.headers,
			raw_request: result.raw_request,
			response_status_line: result.status_line
		};
		try {
			await saveRestResponse(
				target.id,
				result.body,
				result.headers,
				result.raw_request,
				result.status_line
			);
			requests = requests.map((request) =>
				request.id === target.id ? { ...request, ...patch } : request
			);
			if (active?.id === target.id) active = { ...active, ...patch };
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function toggleSaveResponse(checked: boolean): Promise<void> {
		if (!active) return;
		const target = active;
		patchActive({ save_response: checked });
		if (checked) {
			if (response && isSavableResponse(response)) {
				await persistResponse(target, response);
			}
			return;
		}

		try {
			await clearRestResponse(target.id);
			const cleared = {
				response_body: null,
				response_headers: [],
				raw_request: null,
				response_status_line: null
			};
			requests = requests.map((request) =>
				request.id === target.id ? { ...request, ...cleared } : request
			);
			if (active?.id === target.id) active = { ...active, ...cleared };
			if (!response) responseText = '';
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function createRequest(): Promise<void> {
		if (active) void flushSave(active);
		try {
			const created = await createRestRequest({
				project_id: activeProjectId,
				name: '',
				method: 'GET',
				url: '',
				body: '',
				body_type: 'json',
				save_response: false,
				headers: [],
				form: [],
				multipart: [],
				files: []
			});
			requests = [...requests, created];
			selectRequest(created.id);
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function importCurl(): Promise<void> {
		let text = '';
		try {
			text = await readClipboardText();
		} catch (error) {
			notifyError(error, $t('rest.clipboardUnavailable'));
			return;
		}
		if (!text.trim()) {
			notify($t('rest.clipboardEmpty'), 'error');
			return;
		}
		const parsed = parseCurl(text);
		if (!parsed) {
			notify($t('rest.importInvalid'), 'error');
			return;
		}

		try {
			const created = await createRestRequest({
				project_id: activeProjectId,
				name: '',
				method: parsed.method,
				url: parsed.url,
				body: parsed.body,
				body_type: parsed.bodyType,
				save_response: false,
				headers: parsed.headers,
				form: parsed.form,
				multipart: parsed.multipart,
				files: parsed.files
			});
			requests = [...requests, created];
			selectRequest(created.id);
			notify($t('rest.imported'), 'success');
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function createProject(name: string): Promise<void> {
		try {
			const project = await createRestProject(name);
			projects = [...projects, project];
			activeProjectId = project.id;
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function renameProject(id: string, name: string): Promise<void> {
		try {
			await renameRestProject(id, name);
			projects = projects.map((project) =>
				project.id === id ? { ...project, name } : project
			);
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function deleteProject(id: string): Promise<void> {
		try {
			await deleteRestProject(id);
			projects = projects.filter((project) => project.id !== id);
			requests = requests.map((request) =>
				request.project_id === id ? { ...request, project_id: null } : request
			);
			if (active?.project_id === id) {
				active = { ...active, project_id: null };
			}
			if (activeProjectId === id) activeProjectId = null;
			notify($t('rest.projectDeleted'), 'info');
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function renameRequest(id: string, name: string): Promise<void> {
		try {
			await renameRestRequest(id, name);
			requests = requests.map((request) =>
				request.id === id ? { ...request, name } : request
			);
			if (active?.id === id) active = { ...active, name };
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function duplicateRequest(id: string): Promise<void> {
		try {
			const copy = await duplicateRestRequest(id);
			requests = [...requests, copy];
			selectRequest(copy.id);
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function deleteRequest(id: string): Promise<void> {
		if (active?.id === id) void flushSave(active);
		try {
			await deleteRestRequest(id);
			const remaining = requests.filter((request) => request.id !== id);
			requests = remaining;
			if (activeId === id) {
				activeId = null;
				active = null;
				response = null;
				responseText = '';
				if (remaining.length > 0) selectRequest(remaining[0].id);
			}
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function executeRequestById(id: string): Promise<void> {
		selectRequest(id);
		await runRequest();
	}

	async function copyCurl(id: string, shell: CurlShell): Promise<void> {
		const request = requests.find((item) => item.id === id);
		if (!request) return;
		const failure = validationFailure(request);
		if (failure) {
			notify(failure, 'error');
			return;
		}
		const body = resolveRequestBody(request);
		const command = buildCurl(
			{
				method: request.method,
				url: request.url,
				headers: request.headers,
				body,
				body_type: request.body_type,
				multipart: request.multipart,
				files: request.files
			},
			shell
		);
		await copyText(command);
	}

	async function copyRawCurl(shell: CurlShell): Promise<void> {
		if (!active) return;
		const failure = validationFailure(active);
		if (failure) {
			notify(failure, 'error');
			return;
		}
		const command = buildCurl(
			{
				method: active.method,
				url: active.url,
				headers: active.headers,
				body: effectiveBody,
				body_type: active.body_type,
				multipart: active.multipart,
				files: active.files
			},
			shell
		);
		await copyText(command);
	}

	async function downloadResponse(): Promise<void> {
		if (!active) return;
		const failure = validationFailure(active);
		if (failure) {
			notify(failure, 'error');
			return;
		}
		// The name can come from the server's `content-disposition`, so it is reduced
		// to a base name before it reaches the save dialog or the browser download.
		const name = sharedFileName(attachmentName || 'response.bin');
		const input = {
			method: active.method,
			url: active.url,
			headers: active.headers,
			body: active.body,
			body_type: active.body_type,
			form: active.form,
			multipart: active.multipart,
			files: active.files
		};
		try {
			if (isTauriRuntime()) {
				const path = await pickSaveFile(name);
				if (!path) return;
				downloading = true;
				await downloadRestResponse(input, path, name);
			} else {
				downloading = true;
				await downloadRestResponse(input, '', name);
			}
			notify($t('rest.downloaded', { name }), 'success');
		} catch (error) {
			notifyError(error, $t('rest.downloadFailed'));
		} finally {
			downloading = false;
		}
	}

	function clampExplorer(value: number): number {
		const max = typeof window === 'undefined' ? 480 : Math.min(480, window.innerWidth - 640);
		return Math.min(Math.max(value, 220), Math.max(240, max));
	}

	function clampRequestWidth(value: number): number {
		const max =
			typeof window === 'undefined'
				? 1000
				: Math.max(380, window.innerWidth - explorerWidth - 340);
		return Math.min(Math.max(value, 360), max);
	}

	function clampTopHeight(value: number): number {
		const max = typeof window === 'undefined' ? 700 : Math.max(180, window.innerHeight - 300);
		return Math.min(Math.max(value, 160), max);
	}

	function startExplorerResize(event: PointerEvent): void {
		event.preventDefault();
		resizingExplorer = true;
		const startX = event.clientX;
		const startWidth = explorerWidth;
		const move = (moveEvent: PointerEvent) => {
			explorerWidth = clampExplorer(startWidth + moveEvent.clientX - startX);
		};
		const stop = () => {
			resizingExplorer = false;
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', stop);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', stop);
	}

	function startSplitResize(event: PointerEvent): void {
		event.preventDefault();
		resizingSplit = true;
		const startX = event.clientX;
		const startWidth = requestWidth;
		const move = (moveEvent: PointerEvent) => {
			requestWidth = clampRequestWidth(startWidth + moveEvent.clientX - startX);
		};
		const stop = () => {
			resizingSplit = false;
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', stop);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', stop);
	}

	function startHeightResize(event: PointerEvent): void {
		event.preventDefault();
		resizingHeight = true;
		const startY = event.clientY;
		const startHeight = topHeight;
		const move = (moveEvent: PointerEvent) => {
			topHeight = clampTopHeight(startHeight + moveEvent.clientY - startY);
		};
		const stop = () => {
			resizingHeight = false;
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', stop);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', stop);
	}

	function onWindowResize(): void {
		explorerWidth = clampExplorer(explorerWidth);
		requestWidth = clampRequestWidth(requestWidth);
		topHeight = clampTopHeight(topHeight);
	}

	onMount(() => {
		let cancelled = false;

		(async () => {
			try {
				const [loadedProjects, loadedRequests, stored] = await Promise.all([
					listRestProjects(),
					listRestRequests(),
					loadRestSelection()
				]);
				if (cancelled) return;
				projects = loadedProjects;
				requests = loadedRequests;
				// Reopen where the user left off; ids of deleted data are dropped and the
				// first visible request is selected instead.
				const restored = resolveRestSelection(loadedProjects, loadedRequests, stored);
				// Seeded with the stored value, so the effect below rewrites the keys only
				// when the restore had to repair a selection pointing at deleted data.
				savedSelection = selectionKey(stored.project_id, stored.request_id);
				activeProjectId = restored.project_id;
				if (restored.request_id) selectRequest(restored.request_id);
			} catch (error) {
				notifyError(error, $t('common.error'));
			} finally {
				if (!cancelled) {
					hydrated = true;
					loading = false;
				}
			}
		})();

		explorerWidth = clampExplorer(280);
		requestWidth = clampRequestWidth(Math.round((window.innerWidth - explorerWidth) * 0.48));
		topHeight = clampTopHeight(Math.round((window.innerHeight - 220) * 0.5));

		return () => {
			cancelled = true;
			if (saveTimer) clearTimeout(saveTimer);
		};
	});
</script>

<svelte:window onresize={onWindowResize} />

<div class="flex h-full min-h-0 gap-0 px-3 pb-3 pt-3">
	<div class="h-full min-h-0 shrink-0" style="width: {explorerWidth}px;">
		<RestExplorer
			{projects}
			requests={filteredRequests}
			{activeProjectId}
			{activeId}
			onSelectProject={selectProject}
			onCreateProject={createProject}
			onRenameProject={renameProject}
			onDeleteProject={deleteProject}
			onCreateRequest={createRequest}
			onImportCurl={importCurl}
			onSelectRequest={selectRequest}
			onExecuteRequest={executeRequestById}
			onCopyCurl={copyCurl}
			onRenameRequest={renameRequest}
			onDuplicateRequest={duplicateRequest}
			onDeleteRequest={deleteRequest}
		/>
	</div>

	<div
		class="resizer-v h-full shrink-0 {resizingExplorer ? 'active' : ''}"
		style="width: 10px;"
		role="separator"
		aria-orientation="vertical"
		aria-label={$t('rest.resizeExplorer')}
		onpointerdown={startExplorerResize}
	></div>

	<div class="flex min-h-0 min-w-0 flex-1 flex-col">
		<div class="flex h-14 shrink-0 items-center gap-2 pb-2">
			<div class="relative w-[120px] shrink-0">
				<select
					value={active?.method ?? 'GET'}
					onchange={(event) =>
						patchActive({ method: (event.currentTarget as HTMLSelectElement).value })}
					disabled={!active}
					class="h-11 w-full appearance-none rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 pr-8 text-sm font-bold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)] disabled:opacity-50"
					aria-label={$t('rest.method')}
				>
					{#each METHODS as method (method)}
						<option value={method}>{method}</option>
					{/each}
				</select>
				<Icon
					name="chevronDown"
					size={15}
					class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
				/>
			</div>

			<input
				value={active?.url ?? ''}
				oninput={(event) => patchActive({ url: (event.currentTarget as HTMLInputElement).value })}
				disabled={!active}
				placeholder={$t('rest.urlPlaceholder')}
				spellcheck="false"
				autocapitalize="off"
				autocorrect="off"
				class={urlClass}
				aria-label={$t('rest.url')}
				aria-invalid={queryIssues.length > 0}
				title={queryIssues.length ? validationMessage(queryIssues[0], $t) : undefined}
			/>

			<button
				type="button"
				onclick={runRequest}
				disabled={!active}
				class="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-[var(--accent)] text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95 disabled:opacity-50 {running
					? 'ring-2 ring-red-400 ring-offset-2 ring-offset-[var(--app-bg)]'
					: ''}"
				title={running ? $t('rest.abort') : $t('rest.execute')}
				aria-label={running ? $t('rest.abort') : $t('rest.execute')}
			>
				<Icon name="globe" size={22} strokeWidth={2.1} class={running ? 'animate-spin' : ''} />
			</button>
		</div>

		{#if loading}
			<div
				class="flex min-h-0 flex-1 items-center justify-center rounded-lg border border-[var(--border)] bg-[var(--panel-bg)]"
			>
				<p class="text-sm font-semibold text-[var(--text-muted)]">{$t('rest.loading')}</p>
			</div>
		{:else if !active}
			<div
				class="fade-in flex min-h-0 flex-1 flex-col items-center justify-center gap-4 rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] px-6 text-center shadow-[var(--shadow)]"
			>
				<div
					class="flex h-16 w-16 items-center justify-center rounded-2xl bg-[var(--secondary-bg)] text-[var(--secondary-text)]"
				>
					<Icon name="globe" size={30} />
				</div>
				<div>
					<h1 class="text-xl font-bold text-[var(--text)]">{$t('rest.title')}</h1>
					<p class="mt-2 max-w-md text-sm text-[var(--text-muted)]">{$t('rest.emptyState')}</p>
				</div>
				<button
					type="button"
					onclick={createRequest}
					class="inline-flex items-center gap-2 rounded-full bg-[var(--accent)] px-5 py-2.5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
				>
					<Icon name="plus" size={17} strokeWidth={2.4} />
					{$t('rest.createFirst')}
				</button>
			</div>
		{:else}
			<div
				class="grid min-h-0 flex-1"
				style="grid-template-columns: {requestWidth}px 10px minmax(320px, 1fr); grid-template-rows: minmax(0, 1fr);"
			>
				<div class="flex min-h-0 min-w-0 flex-col">
					<section
						class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
						style="height: {topHeight}px;"
					>
						<div class="flex h-11 shrink-0 items-center gap-1 border-b border-[var(--border)] px-2">
							<button
								type="button"
								onclick={() => (requestTab = 'headers')}
								class="flex items-center gap-1 rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {requestTab ===
								'headers'
									? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
									: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
							>
								{$t('rest.tabHeaders')}
								{#if headerIssues.length}
									<span
										class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
										title={validationMessage(headerIssues[0], $t)}
									></span>
								{/if}
							</button>
							<button
								type="button"
								onclick={() => (requestTab = 'query')}
								class="flex items-center gap-1 rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {requestTab ===
								'query'
									? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
									: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
							>
								{$t('rest.tabQuery')}
								{#if queryIssues.length}
									<span
										class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
										title={validationMessage(queryIssues[0], $t)}
									></span>
								{/if}
							</button>
							<button
								type="button"
								onclick={() => (requestTab = 'cookies')}
								class="flex items-center gap-1 rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {requestTab ===
								'cookies'
									? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
									: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
							>
								{$t('rest.tabCookies')}
								{#if cookieIssues.length}
									<span
										class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
										title={validationMessage(cookieIssues[0], $t)}
									></span>
								{/if}
							</button>
							<button
								type="button"
								onclick={() => (requestTab = 'raw')}
								title={$t('rest.rawRequestHint')}
								class="rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {requestTab ===
								'raw'
									? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
									: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
							>
								{$t('rest.rawRequest')}
							</button>
						</div>
						{#if requestTab === 'raw'}
							<div class="flex min-h-0 flex-1 flex-col gap-2 p-2">
								<div class="flex shrink-0 flex-wrap items-center gap-2">
									<button
										type="button"
										onclick={() => copyRawCurl('posix')}
										class="rounded-full bg-[var(--accent)] px-4 py-1.5 text-xs font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
									>
										{$t('rest.curlPosix')}
									</button>
									<button
										type="button"
										onclick={() => copyRawCurl('windows')}
										class="rounded-full bg-[var(--accent)] px-4 py-1.5 text-xs font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
									>
										{$t('rest.curlWindows')}
									</button>
								</div>
								<div class="min-h-0 flex-1">
									<CodeMirrorEditor
										value={rawRequestText}
										language="text"
										dark={dark}
										readonly={true}
										ariaLabel={$t('rest.rawRequestHint')}
									/>
								</div>
							</div>
						{:else}
							<div class="min-h-0 flex-1 p-2">
								{#if requestTab === 'headers'}
									<PropertyEditor
										rows={active.headers}
										onchange={onHeadersChange}
										fieldHint={hintFor('header')}
										nameLabel={$t('rest.headerName')}
										valueLabel={$t('rest.headerValue')}
										namePlaceholder={$t('rest.headerName')}
										valuePlaceholder={$t('rest.headerValue')}
										addLabel={$t('rest.addProperty')}
										removeLabel={$t('rest.removeProperty')}
										headerAutocomplete={true}
									/>
								{:else if requestTab === 'query'}
									<PropertyEditor
										rows={queryRows}
										onchange={onQueryChange}
										fieldHint={hintFor('query')}
										nameLabel={$t('rest.paramName')}
										valueLabel={$t('rest.paramValue')}
										namePlaceholder={$t('rest.paramName')}
										valuePlaceholder={$t('rest.paramValue')}
										addLabel={$t('rest.addProperty')}
										removeLabel={$t('rest.removeProperty')}
									/>
								{:else}
									<PropertyEditor
										rows={cookieRows}
										onchange={onCookiesChange}
										fieldHint={hintFor('cookie')}
										nameLabel={$t('rest.cookieName')}
										valueLabel={$t('rest.cookieValue')}
										namePlaceholder={$t('rest.cookieName')}
										valuePlaceholder={$t('rest.cookieValue')}
										addLabel={$t('rest.addProperty')}
										removeLabel={$t('rest.removeProperty')}
									/>
								{/if}
							</div>
						{/if}
					</section>

					<div
						class="resizer-h shrink-0 {resizingHeight ? 'active' : ''}"
						style="height: 12px;"
						role="separator"
						aria-orientation="horizontal"
						aria-label={$t('rest.resizeBody')}
						onpointerdown={startHeightResize}
					></div>

					<section
						class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
					>
						<div class="flex h-11 shrink-0 items-center gap-1 border-b border-[var(--border)] px-2">
							{#each BODY_TYPES as bodyType (bodyType)}
								<button
									type="button"
									onclick={() => setBodyType(bodyType)}
									class="flex items-center gap-1 rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {active.body_type ===
									bodyType
										? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
										: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
								>
									{$t(`rest.body.${bodyType}`)}
									{#if bodyIssues[bodyType].length}
										<span
											class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
											title={validationMessage(bodyIssues[bodyType][0], $t)}
										></span>
									{/if}
								</button>
							{/each}
							{#if active.body_type === 'json' || active.body_type === 'xml'}
								<button
									type="button"
									onclick={formatBody}
									class="ml-auto flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
									title={$t('common.format')}
									aria-label={$t('common.format')}
								>
									<Icon
										name={active.body_type === 'json' ? 'braces' : 'alignLeft'}
										size={18}
										strokeWidth={1.9}
									/>
								</button>
							{/if}
						</div>
						<div class="min-h-0 flex-1 overflow-hidden">
							{#if active.body_type === 'form'}
								<div class="h-full p-2">
									<PropertyEditor
										rows={active.form}
										onchange={onFormChange}
										fieldHint={hintFor('form')}
										nameLabel={$t('rest.paramName')}
										valueLabel={$t('rest.paramValue')}
										namePlaceholder={$t('rest.paramName')}
										valuePlaceholder={$t('rest.paramValue')}
										addLabel={$t('rest.addProperty')}
										removeLabel={$t('rest.removeProperty')}
									/>
								</div>
							{:else if active.body_type === 'multipart'}
								<div class="flex h-full flex-col">
									<div class="flex shrink-0 items-center gap-1 px-2 pt-2">
										{#each MULTIPART_TABS as tab (tab)}
											<button
												type="button"
												onclick={() => (multipartTab = tab)}
												class="flex items-center gap-1 rounded-md px-3 py-1 text-sm font-bold transition-colors duration-150 {multipartTab ===
												tab
													? 'bg-[var(--row-active)] text-[var(--text)]'
													: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
											>
												{$t(`rest.multipart.${tab}`)}
												{#if tab === 'fields' && multipartFieldIssues.length}
													<span
														class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
														title={validationMessage(multipartFieldIssues[0], $t)}
													></span>
												{:else if tab === 'files' && multipartFileIssues.length}
													<span
														class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
														title={validationMessage(multipartFileIssues[0], $t)}
													></span>
												{/if}
											</button>
										{/each}
									</div>
									<div class="min-h-0 flex-1 p-2">
										{#if multipartTab === 'fields'}
											<PropertyEditor
												rows={active.multipart}
												onchange={onMultipartFieldChange}
												fieldHint={hintFor('multipart')}
												nameLabel={$t('rest.multipart.fieldName')}
												valueLabel={$t('rest.multipart.fieldValue')}
												namePlaceholder={$t('rest.multipart.fieldName')}
												valuePlaceholder={$t('rest.multipart.fieldValue')}
												addLabel={$t('rest.addProperty')}
												removeLabel={$t('rest.removeProperty')}
											/>
										{:else}
											<PropertyFilesEditor
												rows={active.files}
												onchange={onMultipartFileChange}
												fieldHint={fileHint}
												nameLabel={$t('rest.multipart.partName')}
												fileLabel={$t('rest.multipart.file')}
												namePlaceholder={$t('rest.multipart.partName')}
												filePlaceholder={$t('rest.multipart.noFile')}
												addLabel={$t('rest.multipart.addFile')}
												removeLabel={$t('rest.multipart.removeFile')}
												pickLabel={$t('rest.multipart.pickFile')}
												pickTitle={$t('rest.multipart.pickTitle')}
											/>
										{/if}
									</div>
								</div>
							{:else}
								<CodeMirrorEditor
									value={active.body}
									language={active.body_type === 'json'
										? 'json'
										: active.body_type === 'xml'
											? 'xml'
											: 'text'}
									dark={dark}
									placeholder={$t('rest.bodyPlaceholder')}
									ariaLabel={$t('rest.body')}
									onchange={(next) => patchActive({ body: next })}
								/>
							{/if}
						</div>
					</section>
				</div>

				<div
					class="resizer-v h-full {resizingSplit ? 'active' : ''}"
					role="separator"
					aria-orientation="vertical"
					aria-label={$t('rest.resizeResponse')}
					onpointerdown={startSplitResize}
				></div>

				<section
					class="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
				>
					<div class="flex h-11 shrink-0 items-center gap-1 border-b border-[var(--border)] px-2">
						<button
							type="button"
							onclick={() => (responseTab = 'response')}
							class="rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {responseTab ===
							'response'
								? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
								: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
						>
							{$t('rest.response')}
						</button>
						<button
							type="button"
							onclick={() => (responseTab = 'headers')}
							class="rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {responseTab ===
							'headers'
								? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
								: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
						>
							{$t('rest.tabHeaders')}
						</button>
						<button
							type="button"
							onclick={() => (responseTab = 'raw')}
							title={$t('rest.rawResponseHint')}
							class="rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {responseTab ===
							'raw'
								? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
								: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
						>
							{$t('rest.rawResponse')}
						</button>
					</div>

					{#if responseTab === 'response'}
						<div
							class="flex min-h-11 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-[var(--border)] px-3 py-1.5"
						>
							<span class="text-xs font-semibold text-[var(--text-muted)]">
								{$t('rest.status')}:
								<span class="ml-1 text-sm font-bold text-[var(--text)]">
									{requestStatus ?? '—'}
								</span>
							</span>
							<span class="text-xs font-semibold text-[var(--text-muted)]">
								{$t('rest.size')}:
								<span class="ml-1 text-sm font-bold text-[var(--text)]">
									{formatRestSize(requestSize)}
								</span>
							</span>
							<span class="text-xs font-semibold text-[var(--text-muted)]">
								{$t('rest.time')}:
								<span class="ml-1 text-sm font-bold text-[var(--text)]">
									{requestTime === null ? '—' : formatRestTime(requestTime)}
								</span>
							</span>

							{#if showSaveCheckbox}
								<label class="flex cursor-pointer select-none items-center gap-1.5 text-xs font-semibold text-[var(--text)]">
									<input
										type="checkbox"
										checked={active.save_response}
										onchange={(event) =>
											toggleSaveResponse((event.currentTarget as HTMLInputElement).checked)}
										class="h-4 w-4 accent-[var(--accent)]"
									/>
									{$t('rest.save')}
								</label>
							{/if}

							<div class="ml-auto flex items-center gap-1">
								{#if canFormatResponse}
									<button
										type="button"
										onclick={() => transformResponse('format')}
										disabled={!hasResponseText}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
										title={$t('common.format')}
										aria-label={$t('common.format')}
									>
										<Icon name="alignLeft" size={18} strokeWidth={1.9} />
									</button>
									<button
										type="button"
										onclick={() => transformResponse('unescape')}
										disabled={!hasResponseText}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
										title={$t('formatter.unescape')}
										aria-label={$t('formatter.unescape')}
									>
										<Icon name="eraser" size={18} strokeWidth={1.9} />
									</button>
								{/if}
								{#if canPreview}
									<button
										type="button"
										onclick={() => (previewEnabled = !previewEnabled)}
										class="flex h-8 w-8 items-center justify-center rounded-md transition-all duration-150 hover:scale-110 hover:bg-white/5 active:scale-95 {previewEnabled
											? 'text-[var(--accent)]'
											: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
										title={$t('rest.preview')}
										aria-label={$t('rest.preview')}
									>
										<Icon name="eye" size={18} strokeWidth={1.9} />
									</button>
								{/if}
								{#if isAttachment || isBinary}
									<button
										type="button"
										onclick={downloadResponse}
										disabled={downloading}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
										title={$t('rest.download')}
										aria-label={$t('rest.download')}
									>
										<Icon name="download" size={18} strokeWidth={1.9} />
									</button>
								{/if}
								<button
									type="button"
									onclick={copyResponse}
									disabled={!hasResponseText}
									class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
									title={$t('common.copy')}
									aria-label={$t('common.copy')}
								>
									<Icon name="copy" size={18} strokeWidth={1.9} />
								</button>
							</div>
						</div>

						<div class="min-h-0 flex-1">
							{#if isAttachment || isBinary}
								<div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
									<div
										class="flex h-14 w-14 items-center justify-center rounded-2xl bg-[var(--secondary-bg)] text-[var(--secondary-text)]"
									>
										<Icon name="file" size={26} />
									</div>
									<p class="text-sm font-bold text-[var(--text)]">
										{isAttachment ? $t('rest.attachment') : $t('rest.binary')}
									</p>
									<p class="max-w-sm text-xs text-[var(--text-muted)]">
										{isAttachment ? $t('rest.attachmentHint') : $t('rest.binaryHint')}
									</p>
									{#if attachmentName}
										<p class="max-w-full truncate text-xs font-semibold text-[var(--text)]">
											{attachmentName}
										</p>
									{/if}
									<button
										type="button"
										onclick={downloadResponse}
										disabled={downloading}
										class="inline-flex items-center gap-2 rounded-full bg-[var(--accent)] px-5 py-2.5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95 disabled:opacity-50"
									>
										<Icon name="download" size={17} />
										{$t('rest.download')}
									</button>
								</div>
							{:else if !hasResponseText}
								<div class="flex h-full items-center justify-center px-6 text-center">
									<p class="text-sm font-semibold text-[var(--text-muted)]">
										{$t('rest.noResponse')}
									</p>
								</div>
							{:else if previewEnabled && canPreview}
								<iframe
									title={$t('rest.preview')}
									sandbox=""
									referrerpolicy="no-referrer"
									srcdoc={responseText}
									class="h-full w-full border-0 bg-white"
								></iframe>
							{:else}
								<CodeMirrorEditor
									value={responseText}
									language={responseLanguage ?? 'text'}
									dark={dark}
									readonly={true}
									ariaLabel={$t('rest.response')}
								/>
							{/if}
						</div>
					{:else if responseTab === 'headers'}
						<div class="min-h-0 flex-1 overflow-y-auto">
							{#if responseHeaders.length === 0}
								<p class="px-4 py-6 text-center text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.noResponseHeaders')}
								</p>
							{:else}
								<table class="w-full table-fixed text-left text-sm">
									<thead class="sticky top-0 bg-[var(--panel-bg)]">
										<tr class="border-b border-[var(--border)]">
											<th class="w-1/2 px-3 py-2 text-xs font-semibold text-[var(--text-muted)]">
												{$t('rest.headerName')}
											</th>
											<th class="w-1/2 px-3 py-2 text-xs font-semibold text-[var(--text-muted)]">
												{$t('rest.headerValue')}
											</th>
										</tr>
									</thead>
									<tbody>
										{#each responseHeaders as header, index (index)}
											<tr class="border-b border-[var(--border)]">
												<td class="break-words px-3 py-2 align-top font-semibold text-[var(--text)]">
													{header.name}
												</td>
												<td class="break-all px-3 py-2 align-top text-[var(--text)]">
													{header.value}
												</td>
											</tr>
										{/each}
									</tbody>
								</table>
							{/if}
						</div>
					{:else}
						<div class="flex min-h-0 flex-1 flex-col">
							<div class="flex shrink-0 items-center gap-2 px-3 py-2">
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.status')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{requestStatus ?? '—'}
									</span>
								</span>
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.size')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{formatRestSize(requestSize)}
									</span>
								</span>
								<button
									type="button"
									onclick={copyRawResponse}
									disabled={!rawResponseText}
									class="ml-auto flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
									title={$t('common.copy')}
									aria-label={$t('common.copy')}
								>
									<Icon name="copy" size={18} strokeWidth={1.9} />
								</button>
							</div>
							<div class="min-h-0 flex-1">
								<CodeMirrorEditor
									value={rawResponseText}
									language="text"
									dark={dark}
									readonly={true}
									ariaLabel={$t('rest.rawResponseHint')}
								/>
							</div>
						</div>
					{/if}
				</section>
			</div>
		{/if}
	</div>
</div>
