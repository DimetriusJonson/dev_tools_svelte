<script lang="ts">
	import { onMount } from 'svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import PropertyEditor from '$lib/components/PropertyEditor.svelte';
	import WsdlExplorer from '$lib/components/WsdlExplorer.svelte';
	import { t } from '$lib/stores/i18n';
	import { isDark } from '$lib/stores/theme';
	import { notify, notifyError } from '$lib/stores/toast';
	import { isTauriRuntime } from '$lib/services/db';
	import { copyText, readClipboardText } from '$lib/utils/clipboard';
	import { formatContent } from '$lib/utils/formatters';
	import { pickSaveFile, sharedFileName } from '$lib/services/files';
	import {
		cancelRestRequest,
		downloadRestResponse,
		executeRestRequest,
		type RestExecutionInput,
		type RestResponse
	} from '$lib/services/rest';
	import {
		cancelWsdlFetch,
		clearWsdlResponse,
		createWsdlProject,
		deleteWsdlFile,
		deleteWsdlProject,
		fileOperations,
		firstOperation,
		getWsdlOperation,
		importWsdl,
		isWsdlSupported,
		listWsdlFiles,
		listWsdlProjects,
		loadWsdlSelection,
		renameWsdlProject,
		resolveWsdlSelection,
		saveWsdlOperationState,
		saveWsdlResponse,
		saveWsdlSelection,
		type WsdlEndpoint,
		type WsdlFileSummary,
		type WsdlOperation,
		type WsdlOperationDetail,
		type WsdlProject
	} from '$lib/services/wsdl';
	import {
		byteLength,
		buildCurl,
		buildRawResponse,
		detectLanguageFromHeaders,
		filenameFromDisposition,
		formatRestSize,
		formatRestTime,
		getHeader,
		isAttachmentDisposition,
		validateProperties,
		type CurlShell,
		type RestProperty
	} from '$lib/utils/rest';
	import {
		blockedMessage,
		propertyFieldHint,
		validationMessage
	} from '$lib/utils/validation';
	import { describeVarError, soapHasTemplates } from '$lib/utils/vars';
	import {
		containsTemplate,
		maskedValues,
		plainValues,
		referencedVarNames,
		requestHasTemplates,
		substituteText,
		varValues,
		type VarValues
	} from '$lib/utils/vars';
	import { listGlobalVars, revealGlobalVar } from '$lib/services/vars';
	import {
		SOAP_METHOD,
		buildEnvelope,
		buildSoapRawRequest,
		describeErrorText,
		describeWarningText,
		editableForSave,
		formatRawResponseDump,
		lockedHeadersFor,
		mergeLockedHeaders,
		normalizeHeaderRows,
		selectEndpoint,
		soapCurlRequest,
		soapCurlTemplatePlaces,
		type SoapCurlDraft,
		type SoapCurlRequest,
		type SoapVersion
	} from '$lib/utils/wsdl';

	// The tree, the fetch and the SOAP call all live in the Rust backend.
	const desktop = isWsdlSupported();
	const dark = $derived($isDark);

	type RequestTab = 'xml' | 'headers' | 'raw';
	type ResponseTab = 'xml' | 'headers' | 'raw';

	const REQUEST_TABS: RequestTab[] = ['xml', 'headers', 'raw'];
	const RESPONSE_TABS: ResponseTab[] = ['xml', 'headers', 'raw'];
	// Both panels offer the same three views, so one label table covers them.
	const TAB_LABELS: Record<RequestTab | ResponseTab, string> = {
		xml: 'wsdl.tabXml',
		headers: 'wsdl.tabHeaders',
		raw: 'wsdl.tabRaw'
	};

	let projects = $state<WsdlProject[]>([]);
	let files = $state<WsdlFileSummary[]>([]);
	let activeProjectId = $state<string | null>(null);
	let activeOperationId = $state<string | null>(null);
	let detail = $state<WsdlOperationDetail | null>(null);
	let loadingTree = $state(true);
	let loadingDetail = $state(false);
	let importing = $state(false);
	let running = $state(false);
	let aborting = $state(false);
	let hydrated = $state(false);

	let requestBody = $state('');
	let userHeaders = $state<RestProperty[]>([]);
	// The Raw preview substitutes plain variables and masks safe ones, exactly like
	// the dump the backend stores after a call.
	let vars = $state<VarValues>({});
	// A safe value is read only while the cURL dialog is open — the Raw preview keeps
	// the mask. The same rule the REST page keeps for its export.
	let secureDialog = $state(false);
	// The cURL export dropdown next to the copy button on the request `Raw` tab.
	let curlMenu = $state<{ x: number; y: number } | null>(null);
	// The export waiting for the dialog answer, so its buttons know what to copy.
	let pendingExport: (SoapCurlDraft & { shell: CurlShell }) | null = null;
	let endpointId = $state('');
	let dirty = $state(false);
	let saveResponse = $state(false);
	let response = $state<RestResponse | null>(null);
	let responseText = $state('');
	let requestTab = $state<RequestTab>('xml');
	let responseTab = $state<ResponseTab>('xml');
	let downloading = $state(false);

	let explorerWidth = $state(280);
	let topHeight = $state(320);
	let resizingExplorer = $state(false);
	let resizingHeight = $state(false);

	const visibleFiles = $derived(
		activeProjectId ? files.filter((file) => file.project_id === activeProjectId) : files
	);

	const activeFile = $derived(
		activeOperationId
			? (files.find((file) =>
					fileOperations(file).some((operation) => operation.id === activeOperationId)
				) ?? null)
			: null
	);
	const warnings = $derived(activeFile?.warnings ?? []);
	const endpoint = $derived(detail?.endpoints.find((item) => item.id === endpointId) ?? null);

	// A file can bind the same operation through two ports, so the version and the
	// action come from the endpoint the user picked rather than from the row loaded
	// before that choice.
	function versionOf(target: WsdlOperationDetail, picked: WsdlEndpoint | null): SoapVersion {
		return picked?.soap_version ?? target.soap_version;
	}
	function actionOf(target: WsdlOperationDetail, picked: WsdlEndpoint | null): string {
		return picked?.soap_action ?? target.soap_action;
	}
	const soapVersion = $derived(endpoint?.soap_version ?? detail?.soap_version ?? 'soap11');
	const soapAction = $derived(endpoint?.soap_action ?? detail?.soap_action ?? '');
	const lockedHeaders = $derived(lockedHeadersFor(soapVersion, soapAction));
	const lockedNames = $derived(lockedHeaders.map((row) => row.name.trim().toLowerCase()));
	/** What the Headers table shows: the mandatory rows the file writes, then the user's. */
	const headerRows = $derived(mergeLockedHeaders(lockedHeaders, userHeaders));
	const headerIssues = $derived(validateProperties('header', userHeaders));

	const savedResponseBody = $derived(detail?.state.response_body ?? '');
	const savedResponseHeaders = $derived(detail?.state.response_headers ?? []);
	const responseHeaders = $derived(response?.headers ?? savedResponseHeaders);
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
		Boolean(response?.is_attachment) || isAttachmentDisposition(attachmentHeader)
	);
	const isBinary = $derived(
		Boolean(response?.is_binary) ||
			(!responseLanguage && responseContentType.length > 0 && !isAttachment)
	);
	const hasResponseText = $derived(responseText.length > 0);
	const canFormatResponse = $derived(
		responseLanguage === 'json' || responseLanguage === 'xml'
	);
	const showSaveCheckbox = $derived(!isAttachment && !isBinary);
	const responseStatus = $derived(response?.status ?? null);
	const responseSize = $derived(
		response ? response.size : savedResponseBody ? byteLength(savedResponseBody) : 0
	);
	const responseTime = $derived(response?.duration_ms ?? null);
	/** Preview text with plain variables substituted and safe ones replaced by the mask. */
	function renderTemplates(text: string): string {
		return containsTemplate(text) ? substituteText(text, maskedValues(vars)) : text;
	}

	// The answer to a call is the dump the backend wrote; before any call the tab
	// shows the exchange the Send button would produce.
	const rawRequestText = $derived(
		response?.raw_request ||
			detail?.state.raw_request ||
			(endpoint
				? buildSoapRawRequest(
						endpoint.url,
						soapVersion,
						soapAction,
						editableForSave(soapVersion, soapAction, userHeaders).map((row) => ({
							name: row.name,
							value: renderTemplates(row.value)
						})),
						renderTemplates(requestBody)
					)
				: '')
	);

	// `{{ name }}` is expanded by the backend when the call is built, so the preview of
	// the exchange keeps showing the template itself. A header **name** is scanned too:
	// the backend refuses a template there, and this flag is what labels the `Raw` tab.
	const hasVarTemplates = $derived(
		soapHasTemplates({ body: requestBody, headers: userHeaders })
	);

	/** True when a row carries a name the binding already sends on its own. */
	function isLockedRow(row: RestProperty): boolean {
		return lockedNames.includes(row.name.trim().toLowerCase());
	}

	/** The payload the tabs read: the live answer, or the one stored for this operation. */
	const responseBody = $derived(response ? response.body : savedResponseBody);
	/** The first line of the exchange; a stored answer from an older database has none. */
	const responseStatusLine = $derived(
		response?.status_line ?? detail?.state.response_status_line ?? ''
	);
	// The whole exchange: the status line the server answered with, one header per line,
	// the blank line that separates them, and the body exactly as it arrived.
	const rawResponseDump = $derived(
		buildRawResponse(responseStatusLine, responseHeaders, responseBody)
	);

	/**
	 * A markup answer stays markup inside the dump, so the tab highlights it as XML and
	 * offers `Format`; anything else is plain text with no format action.
	 */
	const rawResponseLanguage = $derived(responseLanguage === 'xml' ? 'xml' : 'text');
	const canFormatRawResponse = $derived(responseLanguage === 'xml' && responseBody.length > 0);

	// A mirror of the dump: `Format` rewrites only what this tab shows, so neither the
	// stored bytes nor the `XML` tab's own mirror change. The XML tab shows the editable
	// mirror instead, and this one keeps the framing the exchange really had.
	let rawResponseText = $state('');

	$effect(() => {
		rawResponseText = rawResponseDump;
	});

	/** What Send and the download button put on the wire: the envelope plus all headers. */
	const soapInput = $derived<RestExecutionInput | null>(
		endpoint
			? {
					method: SOAP_METHOD,
					url: endpoint.url,
					headers: mergeLockedHeaders(lockedHeaders, userHeaders),
					body: buildEnvelope(requestBody),
					body_type: 'xml',
					form: []
				}
			: null
	);

	/** A locked row is written by the binding, so its cells are never validated. */
	function headerHint(field: 'name' | 'value', row: RestProperty, index: number): string | null {
		if (isLockedRow(row)) return null;
		return propertyFieldHint('header', field, row, index, $t);
	}

	function onHeadersChange(rows: RestProperty[]): void {
		// The merged table hands back the mandatory rows too; only the user's survive.
		userHeaders = editableForSave(soapVersion, soapAction, normalizeHeaderRows(rows));
		dirty = true;
	}

	function formatRequest(): void {
		try {
			requestBody = formatContent('xml', requestBody, {
				indent: 4,
				doFormat: true,
				doUnescape: false
			});
			dirty = true;
		} catch {
			notify($t('formatter.invalid'), 'error');
		}
	}

	function resetToTemplate(): void {
		if (!detail) return;
		requestBody = detail.input_template;
		dirty = true;
		requestTab = 'xml';
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

	async function copyRawRequest(): Promise<void> {
		if (rawRequestText) await copyText(rawRequestText);
	}

	function openCurlMenu(event: MouseEvent): void {
		event.stopPropagation();
		if (curlMenu) {
			curlMenu = null;
			return;
		}
		const target = event.currentTarget as HTMLElement;
		const rect = target.getBoundingClientRect();
		curlMenu = {
			x: Math.max(8, Math.min(rect.left, window.innerWidth - 228)),
			y: Math.min(rect.bottom + 4, window.innerHeight - 150)
		};
	}

	function closeCurlMenu(): void {
		curlMenu = null;
	}

	function runCurlMenuAction(action: () => void): void {
		action();
		curlMenu = null;
	}

	function onWindowClick(event: MouseEvent): void {
		const target = event.target as HTMLElement | null;
		if (target && !target.isConnected) return;
		if (target?.closest('[role="menu"], [role="dialog"]')) return;
		curlMenu = null;
	}

	function onWindowKeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape') curlMenu = null;
	}

	/** Same gate the download button uses: an endpoint must exist and the header
	 *  table must be clean. A request that cannot be sent is not exported either. */
	function soapExportBlocked(): string | null {
		if (!endpoint) return $t('wsdl.noEndpoint');
		const issues = headerIssues;
		if (issues.length) return blockedMessage(issues, $t) ?? $t('common.error');
		return null;
	}

	/** Re-reads the variable list before an export, so a value edited on
	 *  /global-vars cannot reach the command as a stale copy. */
	async function refreshVars(): Promise<void> {
		if (!isTauriRuntime()) return;
		try {
			vars = varValues(await listGlobalVars());
		} catch (error) {
			notify(describeVarError(error, $t) || $t('vars.loadFailed'), 'error');
		}
	}

	/** Reads the safe values this export mentions, one reveal per name. */
	async function revealSoapSecureValues(draft: SoapCurlDraft): Promise<VarValues> {
		const values: VarValues = { ...vars };
		for (const name of referencedVarNames(soapCurlTemplatePlaces(draft))) {
			const entry = values[name];
			if (!entry?.secure) continue;
			const secret = await revealGlobalVar(entry.id);
			values[name] = { ...entry, value: secret };
		}
		return values;
	}

	/** Exports the call the Send button would make as cURL. Plain variables are
	 *  always substituted; a safe variable the call mentions is asked about first.
	 */
	async function exportSoapCurl(shell: CurlShell): Promise<void> {
		const blocked = soapExportBlocked();
		if (blocked) {
			notify(blocked, 'error');
			return;
		}
		const draft: SoapCurlDraft & { shell: CurlShell } = {
			endpointUrl: endpoint?.url ?? '',
			soapVersion,
			soapAction,
			userHeaders: [...userHeaders],
			body: requestBody,
			shell
		};
		const places = soapCurlTemplatePlaces(draft);
		if (!isTauriRuntime()) {
			// The browser build cannot read the variable store, so a template would
			// be written literally.
			if (requestHasTemplates(places)) {
				notify($t('vars.curlUnavailable'), 'error');
				return;
			}
			const source = soapCurlRequest(draft, vars);
			if (source) await copyText(buildCurl(source, shell));
			return;
		}
		await refreshVars();
		if (!referencedVarNames(places).some((name) => vars[name]?.secure)) {
			const source = soapCurlRequest(draft, vars);
			if (source) await copyText(buildCurl(source, shell));
			return;
		}
		pendingExport = draft;
		secureDialog = true;
	}

	/** Answers the safe-variable question: "Replace" copies the real values. */
	async function resolveSecureExport(replace: boolean): Promise<void> {
		const target = pendingExport;
		pendingExport = null;
		secureDialog = false;
		if (!target) return;
		try {
			const values = replace ? await revealSoapSecureValues(target) : plainValues(vars);
			const source = soapCurlRequest(target, values);
			if (source) await copyText(buildCurl(source, target.shell));
		} catch (error) {
			notify(describeVarError(error, $t) || $t('vars.loadFailed'), 'error');
		}
	}

	async function copyRawResponse(): Promise<void> {
		if (rawResponseText) await copyText(rawResponseText);
	}

	/**
	 * `Format` on the `Raw` tab re-indents only the payload. The status line and the
	 * header block above it stay exactly as they arrived, and the stored bytes are
	 * not touched — this rewrites the tab's mirror alone.
	 */
	function formatRawResponse(): void {
		try {
			rawResponseText = formatRawResponseDump(responseStatusLine, responseHeaders, responseBody);
		} catch {
			notify($t('formatter.invalid'), 'error');
		}
	}

	/** A binary answer is only savable as a file; a SOAP fault with a body is text. */
	function isSavableResponse(result: RestResponse): boolean {
		return !result.is_attachment && !result.is_binary && result.language !== null;
	}

	async function persistResponse(operationId: string, result: RestResponse): Promise<void> {
		try {
			await saveWsdlResponse(
				operationId,
				result.body,
				result.headers,
				result.raw_request,
				result.status_line
			);
			if (detail?.id === operationId) {
				detail.state.response_body = result.body;
				detail.state.response_headers = result.headers;
				detail.state.raw_request = result.raw_request;
				detail.state.response_status_line = result.status_line;
			}
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function toggleSaveResponse(checked: boolean): Promise<void> {
		if (!detail) return;
		const target = detail;
		saveResponse = checked;
		// The flag itself belongs to the operation state, so it goes to SQLite too.
		dirty = true;
		if (checked) {
			if (response && isSavableResponse(response)) await persistResponse(target.id, response);
			return;
		}

		try {
			await clearWsdlResponse(target.id);
			if (detail === target) {
				target.state.response_body = '';
				target.state.response_headers = [];
				target.state.raw_request = '';
				target.state.response_status_line = '';
			}
			// An answer of the current call stays on screen; only the stored one goes away.
			if (!response) responseText = '';
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function downloadResponse(): Promise<void> {
		const input = soapInput;
		if (!input) {
			notify($t('wsdl.noEndpoint'), 'error');
			return;
		}
		if (headerIssues.length) {
			notify(blockedMessage(headerIssues, $t) ?? $t('common.error'), 'error');
			return;
		}

		// A name the server offered is reduced to a base name: it goes into the native
		// dialog and into the browser download attribute.
		const name = sharedFileName(attachmentName || 'response.bin');
		try {
			downloading = true;
			if (isTauriRuntime()) {
				const path = await pickSaveFile(name);
				if (!path) return;
				await downloadRestResponse(input, path, name);
			} else {
				await downloadRestResponse(input, '', name);
			}
			notify($t('rest.downloaded', { name }), 'success');
		} catch (error) {
			notifyError(describeVarError(error, $t), $t('rest.downloadFailed'));
		} finally {
			downloading = false;
		}
	}

	function describeWarning(warning: string): string {
		const described = describeWarningText(warning);
		return described.detail ? $t(described.key, { detail: described.detail }) : $t(described.key);
	}

	// Non-reactive: it records what is already persisted so the effect below does not
	// rewrite the same selection after the initial restore.
	let savedSelection = '';

	function selectionKey(projectId: string | null, operationId: string | null): string {
		return `${projectId ?? ''}|${operationId ?? ''}`;
	}

	// The explorer selection outlives the session: it goes to SQLite so the app reopens
	// on the same project and operation.
	$effect(() => {
		if (!hydrated || !desktop) return;
		const key = selectionKey(activeProjectId, activeOperationId);
		if (key === savedSelection) return;
		savedSelection = key;
		void saveWsdlSelection({ project_id: activeProjectId, operation_id: activeOperationId });
	});

	/** Writes the editable part of the operation state; the locked rows are recomputed in Rust. */
	async function flushState(target: WsdlOperationDetail): Promise<void> {
		await saveWsdlOperationState({
			operation_id: target.id,
			endpoint_id: endpointId,
			request_body: requestBody,
			// The version of the picked endpoint decides which names are mandatory,
			// exactly like `persist_state` in Rust recomputes them on write.
			headers: editableForSave(soapVersion, soapAction, userHeaders),
			save_response: saveResponse
		});
		if (detail === target) dirty = false;
	}

	// Body and endpoint edits are persisted a moment after typing, like the other editors.
	$effect(() => {
		if (!hydrated || !detail || !dirty) return;
		const target = detail;
		const timer = setTimeout(() => {
			void flushState(target).catch((error) => notifyError(error, $t('common.error')));
		}, 450);
		return () => clearTimeout(timer);
	});

	// Sequential ids so a slow read of an earlier operation cannot overwrite a newer one.
	let detailToken = 0;

	function applyDetail(loaded: WsdlOperationDetail): void {
		detail = loaded;
		activeOperationId = loaded.id;
		requestBody = loaded.state.request_body;
		endpointId = selectEndpoint(loaded.endpoints, loaded.state.endpoint_id)?.id ?? '';
		// The picked endpoint decides which names the binding writes, so the split of
		// the stored list happens after it is known.
		const target = endpointId ? selectEndpoint(loaded.endpoints, endpointId) : null;
		userHeaders = editableForSave(
			versionOf(loaded, target),
			actionOf(loaded, target),
			loaded.state.headers
		);
		saveResponse = loaded.state.save_response;
		dirty = false;
		response = null;
		// The answer saved for this operation is shown again as it arrived.
		responseText = loaded.state.response_body;
		responseTab = 'xml';
		requestTab = 'xml';
	}

	async function openOperation(operationId: string): Promise<void> {
		const token = ++detailToken;
		loadingDetail = true;
		try {
			const loaded = await getWsdlOperation(operationId);
			if (token === detailToken) applyDetail(loaded);
		} catch (error) {
			if (token === detailToken) notifyError(error, $t('common.error'));
		} finally {
			if (token === detailToken) loadingDetail = false;
		}
	}

	function clearDetail(): void {
		detailToken += 1;
		loadingDetail = false;
		detail = null;
		requestBody = '';
		userHeaders = [];
		endpointId = '';
		dirty = false;
		saveResponse = false;
		response = null;
		responseText = '';
		requestTab = 'xml';
		responseTab = 'xml';
	}

	function selectOperation(operationId: string): void {
		if (operationId === activeOperationId) return;
		activeOperationId = operationId;
		// The next detail carries its own saved answer; until it arrives the panel is empty.
		response = null;
		responseText = '';
		responseTab = 'xml';
		void openOperation(operationId);
	}

	/** The first operation of the visible tree, so a cleared selection never stays empty. */
	function firstVisibleOperation(): WsdlOperation | null {
		for (const file of visibleFiles) {
			const operation = firstOperation(file);
			if (operation) return operation;
		}
		return null;
	}

	function isOperationVisible(operationId: string | null): boolean {
		if (!operationId) return false;
		return visibleFiles.some((file) =>
			fileOperations(file).some((operation) => operation.id === operationId)
		);
	}

	function resyncSelection(): void {
		if (isOperationVisible(activeOperationId)) return;
		const operation = firstVisibleOperation();
		if (operation) selectOperation(operation.id);
		else clearDetail();
	}

	function selectProject(projectId: string | null): void {
		activeProjectId = projectId;
		resyncSelection();
	}

	async function createProject(name: string): Promise<void> {
		try {
			const project = await createWsdlProject(name);
			projects = [...projects, project];
			// The new project is empty, so the tree follows it and the operation of the
			// previous view is replaced by the first one the new view holds.
			selectProject(project.id);
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function renameProject(id: string, name: string): Promise<void> {
		try {
			await renameWsdlProject(id, name);
			projects = projects.map((project) => (project.id === id ? { ...project, name } : project));
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function deleteProject(id: string): Promise<void> {
		try {
			await deleteWsdlProject(id);
			projects = projects.filter((project) => project.id !== id);
			files = files.filter((file) => file.project_id !== id);
			if (activeProjectId === id) activeProjectId = null;
			resyncSelection();
			notify($t('wsdl.projectDeleted'), 'info');
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function importFromClipboard(): Promise<void> {
		if (importing) return;
		let text = '';
		try {
			text = await readClipboardText();
		} catch (error) {
			notifyError(error, $t('wsdl.clipboardUnavailable'));
			return;
		}
		const url = text.trim();
		if (!url) {
			notify($t('wsdl.clipboardEmpty'), 'error');
			return;
		}

		importing = true;
		try {
			const result = await importWsdl(activeProjectId ?? '', url);
			files = await listWsdlFiles();
			notify(
				$t(result.reimported ? 'wsdl.reimported' : 'wsdl.imported', {
					name: result.summary.name
				}),
				'success'
			);
			// A file that lands in another project has to be shown, so the view follows it.
			if (activeProjectId !== null && result.summary.project_id !== activeProjectId) {
				activeProjectId = result.summary.project_id || null;
			}
			const operation = firstOperation(result.summary);
			if (operation) selectOperation(operation.id);
		} catch (error) {
			const described = describeErrorText(String(error));
			const message = described.detail
				? $t(described.key, { detail: described.detail })
				: $t(described.key);
			notify(message, 'error');
		} finally {
			importing = false;
		}
	}

	async function cancelImport(): Promise<void> {
		try {
			await cancelWsdlFetch();
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	async function deleteFile(fileId: string): Promise<void> {
		try {
			await deleteWsdlFile(fileId);
			files = files.filter((file) => file.id !== fileId);
			resyncSelection();
			notify($t('wsdl.fileDeleted'), 'info');
		} catch (error) {
			notifyError(error, $t('common.error'));
		}
	}

	/**
	 * Sends the envelope for one operation. The HTTP call is the REST backend's, so the
	 * cancellation path is `cancel_rest_request` too, and a SOAP fault arrives as a
	 * response rather than an error.
	 */
	async function runOperation(operationId: string): Promise<void> {
		if (running) return;
		const current = detail !== null && detail.id === operationId;
		let loaded: WsdlOperationDetail | null = current ? detail : null;
		if (!loaded) {
			try {
				loaded = await getWsdlOperation(operationId);
			} catch (error) {
				notifyError(error, $t('common.error'));
				return;
			}
		}
		if (current && dirty) {
			// A failed save must not stop the call: the stored state still goes out.
			await flushState(loaded).catch(() => null);
		}

		const savedId = current ? endpointId : loaded.state.endpoint_id;
		const target = selectEndpoint(loaded.endpoints, savedId);
		if (!target) {
			notify($t('wsdl.noEndpoint'), 'error');
			return;
		}

		// The picked endpoint decides the SOAP version, so its locked names are the ones
		// a user row must not repeat on this call.
		const outgoingHeaders = current
			? userHeaders
			: editableForSave(target.soap_version, actionOf(loaded, target), loaded.state.headers);
		const issues = validateProperties('header', outgoingHeaders);
		if (issues.length) {
			notify(blockedMessage(issues, $t) ?? $t('common.error'), 'error');
			return;
		}

		const body = current ? requestBody : loaded.state.request_body;
		// An operation whose schema produced a template must not go out empty; an
		// operation with no input element sends an empty Body on purpose.
		if (body.trim() === '' && loaded.input_template.trim() !== '') {
			notify($t('wsdl.emptyBody'), 'error');
			return;
		}
		const remember = current ? saveResponse : loaded.state.save_response;

		running = true;
		aborting = false;
		try {
			const result = await executeRestRequest({
				method: SOAP_METHOD,
				url: target.url,
				headers: mergeLockedHeaders(
					lockedHeadersFor(target.soap_version, target.soap_action),
					outgoingHeaders
				),
				body: buildEnvelope(body),
				body_type: 'xml',
				form: []
			});
			if (activeOperationId === operationId) {
				response = result;
				responseText = result.body;
				responseTab = 'xml';
			}
			if (remember && isSavableResponse(result)) {
				await persistResponse(operationId, result);
			}
		} catch (error) {
			if (aborting || String(error).toLowerCase().includes('cancel')) {
				notify($t('rest.aborted'), 'info');
			} else {
				// The SOAP call goes through the REST executor, so a substitution failure
				// arrives as a code and is localized rather than shown as backend text.
				notifyError(describeVarError(error, $t), $t('rest.requestFailed'));
			}
		} finally {
			running = false;
			aborting = false;
		}
	}

	async function abortRequest(): Promise<void> {
		if (!running || aborting) return;
		aborting = true;
		try {
			await cancelRestRequest();
		} catch (error) {
			aborting = false;
			notifyError(error, $t('common.error'));
		}
	}

	function executeSelected(): void {
		if (running) {
			void abortRequest();
			return;
		}
		if (!detail) {
			notify($t('wsdl.selectOperation'), 'error');
			return;
		}
		void runOperation(detail.id);
	}

	/** The file menu's Execute: the operation the user has open in that file, else its first. */
	async function executeFile(fileId: string): Promise<void> {
		const file = files.find((item) => item.id === fileId);
		if (!file) return;
		const operations = fileOperations(file);
		const selected =
			operations.find((operation) => operation.id === activeOperationId) ?? operations[0];
		if (!selected) return;
		if (selected.id !== activeOperationId) {
			activeOperationId = selected.id;
			response = null;
			responseText = '';
		}
		await openOperation(selected.id);
		await runOperation(selected.id);
	}

	function clampExplorer(value: number): number {
		const max = typeof window === 'undefined' ? 480 : Math.min(480, window.innerWidth - 640);
		return Math.min(Math.max(value, 220), Math.max(240, max));
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
		topHeight = clampTopHeight(topHeight);
	}

	function onEndpointChange(event: Event): void {
		endpointId = (event.currentTarget as HTMLSelectElement).value;
		dirty = true;
	}

	onMount(() => {
		if (!desktop) return;
		let cancelled = false;

		(async () => {
			try {
				const [loadedProjects, loadedFiles, stored] = await Promise.all([
					listWsdlProjects(),
					listWsdlFiles(),
					loadWsdlSelection()
				]);
				if (cancelled) return;
				projects = loadedProjects;
				files = loadedFiles;
				// Reopen where the user left off; ids of deleted data are dropped and the
				// first visible operation is selected instead.
				const restored = resolveWsdlSelection(loadedProjects, loadedFiles, stored);
				// Seeded with the stored value, so the effect below rewrites the keys only
				// when the restore had to repair a selection pointing at deleted data.
				savedSelection = selectionKey(stored.project_id, stored.operation_id);
				activeProjectId = restored.project_id;
				if (restored.operation_id) await openOperation(restored.operation_id);
				// The `Raw` tab substitutes plain variables, so their values are read once.
				try {
					vars = varValues(await listGlobalVars());
				} catch {
					vars = {};
				}
			} catch (error) {
				notifyError(error, $t('common.error'));
			} finally {
				if (!cancelled) {
					hydrated = true;
					loadingTree = false;
				}
			}
		})();

		explorerWidth = clampExplorer(280);
		topHeight = clampTopHeight(Math.round((window.innerHeight - 220) * 0.5));

		return () => {
			cancelled = true;
		};
	});
</script>

<svelte:window onresize={onWindowResize} onclick={onWindowClick} onkeydown={onWindowKeydown} />

{#if !desktop}
	<div class="flex h-full min-h-0 items-center justify-center px-6">
		<section
			class="max-w-lg rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-6 text-center shadow-[var(--shadow)]"
		>
			<div class="mb-3 flex items-center justify-center gap-2 text-[var(--accent)]">
				<Icon name="server" size={20} />
			</div>
			<p class="text-sm font-bold text-[var(--text)]">{$t('wsdl.desktopOnly')}</p>
			<p class="mt-2 text-xs font-semibold leading-relaxed text-[var(--text-muted)]">
				{$t('wsdl.desktopOnlyHint')}
			</p>
		</section>
	</div>
{:else}
	<div class="flex h-full min-h-0 gap-0 px-3 pb-3 pt-3">
		<div class="h-full min-h-0 shrink-0" style="width: {explorerWidth}px;">
			<WsdlExplorer
				{projects}
				files={visibleFiles}
				{activeProjectId}
				activeOperationId={activeOperationId}
				loading={loadingTree}
				{importing}
				onSelectProject={selectProject}
				onCreateProject={createProject}
				onRenameProject={renameProject}
				onDeleteProject={deleteProject}
				onImport={importFromClipboard}
				onCancelImport={cancelImport}
				onSelectOperation={selectOperation}
				onExecuteFile={(fileId) => void executeFile(fileId)}
				onDeleteFile={(fileId) => void deleteFile(fileId)}
			/>
		</div>

		<div
			class="resizer-v h-full shrink-0 {resizingExplorer ? 'active' : ''}"
			style="width: 10px;"
			role="separator"
			aria-orientation="vertical"
			aria-label={$t('wsdl.resizeExplorer')}
			onpointerdown={startExplorerResize}
		></div>

		<div class="flex min-h-0 min-w-0 flex-1 flex-col gap-2">
			{#if warnings.length > 0}
				<div
					role="status"
					class="flex shrink-0 items-start gap-2 rounded-lg border border-[var(--border)] px-3 py-2 shadow-[var(--shadow)]"
					style="background-color: color-mix(in srgb, var(--accent) 10%, var(--panel-bg));"
				>
					<Icon name="warning" size={16} class="mt-0.5 shrink-0 text-[var(--accent)]" />
					<div class="min-w-0 flex-1">
						<p class="text-xs font-bold text-[var(--text)]">{$t('wsdl.warnings')}</p>
						<ul class="mt-1 flex flex-col gap-0.5">
							{#each warnings as warning (warning)}
								<li class="text-[11px] leading-snug text-[var(--text-muted)]">
									{describeWarning(warning)}
								</li>
							{/each}
						</ul>
					</div>
				</div>
			{/if}

			{#if loadingDetail && !detail}
				<div
					class="flex min-h-0 flex-1 items-center justify-center rounded-lg border border-[var(--border)] bg-[var(--panel-bg)]"
				>
					<p class="text-xs font-semibold text-[var(--text-muted)]">{$t('wsdl.loading')}</p>
				</div>
			{:else if !detail}
				<div
					class="fade-in flex min-h-0 flex-1 flex-col items-center justify-center gap-3 rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] px-6 text-center shadow-[var(--shadow)]"
				>
					<Icon name="braces" size={30} class="text-[var(--text-muted)]" />
					<p class="text-sm font-semibold text-[var(--text-muted)]">{$t('wsdl.selectOperation')}</p>
				</div>
			{:else}
				<div class="flex h-12 shrink-0 items-center gap-2">
					<div class="flex min-w-[96px] max-w-[220px] shrink flex-col justify-center">
						<p class="truncate text-sm font-bold text-[var(--text)]">{detail.name}</p>
						<p class="truncate text-[11px] font-semibold text-[var(--text-muted)]">
							{detail.service_name} · SOAP {soapVersion === 'soap12' ? '1.2' : '1.1'}
						</p>
					</div>

					<div class="relative min-w-0 flex-1">
						<select
							value={endpointId}
							onchange={onEndpointChange}
							disabled={detail.endpoints.length === 0}
							class="h-10 w-full appearance-none rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 pr-9 text-xs font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)] disabled:opacity-50"
							aria-label={$t('wsdl.endpoint')}
							title={endpoint?.url ?? $t('wsdl.noEndpoint')}
						>
							{#if detail.endpoints.length === 0}
								<option value="">{$t('wsdl.noEndpoint')}</option>
							{/if}
							{#each detail.endpoints as item (item.id)}
								<option value={item.id}>{item.url}</option>
							{/each}
						</select>
						<Icon
							name="chevronDown"
							size={16}
							class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
						/>
					</div>

					<button
						type="button"
						onclick={executeSelected}
						class="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-[var(--accent)] text-[var(--accent-text)] transition-all duration-150 hover:scale-105 active:scale-95"
						title={running ? $t('wsdl.abort') : $t('wsdl.execute')}
						aria-label={running ? $t('wsdl.abort') : $t('wsdl.execute')}
						aria-busy={running}
					>
						<Icon name="globe" size={20} class={running ? 'animate-spin' : ''} />
					</button>
				</div>

				<div
					class="grid min-h-0 flex-1"
					style="grid-template-columns: minmax(0, 1fr); grid-template-rows: {topHeight}px 10px minmax(180px, 1fr);"
				>
					<div
						class="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
					>
						<div class="flex h-11 shrink-0 items-center gap-1 border-b border-[var(--border)] px-2">
							<span class="mr-1 text-sm font-bold text-[var(--text)]">{$t('wsdl.request')}</span>
							{#each REQUEST_TABS as tab (tab)}
								<button
									type="button"
									onclick={() => (requestTab = tab)}
									class="flex items-center gap-1 rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {requestTab ===
									tab
										? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
										: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
								>
									{$t(TAB_LABELS[tab])}
									{#if tab === 'headers' && headerIssues.length}
										<span
											class="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--danger)]"
											title={validationMessage(headerIssues[0], $t)}
										></span>
									{/if}
								</button>
							{/each}

							<div class="ml-auto flex items-center gap-1">
								{#if requestTab === 'xml'}
									<button
										type="button"
										onclick={formatRequest}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
										title={$t('common.format')}
										aria-label={$t('common.format')}
									>
										<Icon name="alignLeft" size={18} strokeWidth={1.9} />
									</button>
									<button
										type="button"
										onclick={resetToTemplate}
										disabled={!detail.input_template}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
										title={$t('wsdl.resetTemplate')}
										aria-label={$t('wsdl.resetTemplate')}
									>
										<Icon name="refresh" size={18} strokeWidth={1.9} />
									</button>
								{:else if requestTab === 'raw'}
									<button
										type="button"
										onclick={copyRawRequest}
										disabled={!rawRequestText}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
										title={$t('common.copy')}
										aria-label={$t('common.copy')}
									>
										<Icon name="copy" size={18} strokeWidth={1.9} />
									</button>
									<button
										type="button"
										onclick={openCurlMenu}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
										title={$t('rest.curlMenu')}
										aria-label={$t('rest.curlMenu')}
									>
										<Icon name="more" size={18} strokeWidth={3} />
									</button>
									{#if curlMenu}
										<div
											class="fixed z-50 w-40 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] py-1 shadow-2xl"
											style="left: {curlMenu.x}px; top: {curlMenu.y}px;"
											role="menu"
										>
											<button
												type="button"
												onclick={() => runCurlMenuAction(() => void exportSoapCurl('posix'))}
												class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
												role="menuitem"
											>
												{$t('rest.copyCurlPosix')}
											</button>
											<button
												type="button"
												onclick={() => runCurlMenuAction(() => void exportSoapCurl('windows'))}
												class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
												role="menuitem"
											>
												{$t('rest.copyCurlWindows')}
											</button>
										</div>
									{/if}
								{/if}
							</div>
						</div>

						<div class="min-h-0 flex-1 overflow-hidden">
							{#if requestTab === 'xml'}
								<CodeMirrorEditor
									value={requestBody}
									language="xml"
									dark={dark}
									placeholder={$t('rest.bodyPlaceholder')}
									ariaLabel={$t('wsdl.request')}
									onchange={(next) => {
										requestBody = next;
										dirty = true;
									}}
								/>
							{:else if requestTab === 'headers'}
								<div class="h-full p-2">
									<PropertyEditor
										rows={headerRows}
										onchange={onHeadersChange}
										fieldHint={headerHint}
										locked={(row) => isLockedRow(row)}
										lockedTitle={$t('wsdl.lockedHeader')}
										nameLabel={$t('rest.headerName')}
										valueLabel={$t('rest.headerValue')}
										namePlaceholder={$t('rest.headerName')}
										valuePlaceholder={$t('rest.headerValue')}
										addLabel={$t('rest.addProperty')}
										removeLabel={$t('rest.removeProperty')}
									/>
								</div>
							{:else}
								<div class="flex min-h-0 flex-1 flex-col gap-1">
									{#if hasVarTemplates}
										<p class="shrink-0 px-2 text-xs leading-relaxed text-[var(--text-muted)]">
											{$t('vars.rawHint')}
										</p>
									{/if}
									<div class="min-h-0 flex-1">
										<CodeMirrorEditor
											value={rawRequestText}
											language="text"
											dark={dark}
											readonly={true}
											ariaLabel={$t('wsdl.tabRaw')}
											/>
									</div>
								</div>
							{/if}
						</div>
					</div>

					<div
						class="resizer-h shrink-0 {resizingHeight ? 'active' : ''}"
						role="separator"
						aria-orientation="horizontal"
						aria-label={$t('wsdl.resizeBody')}
						onpointerdown={startHeightResize}
					></div>

					<div
						class="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
					>
						<div class="flex h-11 shrink-0 items-center gap-1 border-b border-[var(--border)] px-2">
							<span class="mr-1 text-sm font-bold text-[var(--text)]">{$t('wsdl.response')}</span>
							{#each RESPONSE_TABS as tab (tab)}
								<button
									type="button"
									onclick={() => (responseTab = tab)}
									class="rounded-md px-3 py-1.5 text-sm font-bold transition-colors duration-150 {responseTab ===
									tab
										? 'bg-[var(--panel-bg-alt)] text-[var(--text)]'
										: 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
								>
									{$t(TAB_LABELS[tab])}
								</button>
							{/each}
						</div>

						{#if responseTab === 'xml'}
							<div
								class="flex min-h-11 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-[var(--border)] px-3 py-1.5"
							>
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.status')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{responseStatus ?? '—'}
									</span>
								</span>
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.size')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{formatRestSize(responseSize)}
									</span>
								</span>
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.time')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{responseTime === null ? '—' : formatRestTime(responseTime)}
									</span>
								</span>

								{#if showSaveCheckbox}
									<label
										class="flex cursor-pointer select-none items-center gap-1.5 text-xs font-semibold text-[var(--text)]"
									>
										<input
											type="checkbox"
											checked={saveResponse}
											onchange={(event) =>
												void toggleSaveResponse((event.currentTarget as HTMLInputElement).checked)}
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
											{$t('wsdl.noResponse')}
										</p>
									</div>
								{:else}
									<CodeMirrorEditor
										value={responseText}
										language={responseLanguage ?? 'text'}
										dark={dark}
										readonly={true}
										ariaLabel={$t('wsdl.response')}
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
									{#if canFormatRawResponse}
										<button
											type="button"
											onclick={formatRawResponse}
											class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
											title={$t('common.format')}
											aria-label={$t('common.format')}
										>
											<Icon name="alignLeft" size={18} strokeWidth={1.9} />
										</button>
									{/if}
									<button
										type="button"
										onclick={copyRawResponse}
										disabled={!rawResponseText}
										class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 disabled:opacity-40"
										title={$t('common.copy')}
										aria-label={$t('common.copy')}
									>
										<Icon name="copy" size={18} strokeWidth={1.9} />
									</button>
								</div>
								<div class="min-h-0 flex-1">
									<CodeMirrorEditor
										value={rawResponseText}
										language={rawResponseLanguage}
										dark={dark}
										readonly={true}
										ariaLabel={$t('wsdl.tabRaw')}
									/>
								</div>
							</div>
						{/if}
					</div>
				</div>
			{/if}
		</div>
	</div>
{#if secureDialog}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
		role="dialog"
		aria-modal="true"
		aria-labelledby="soap-secure-templates-heading"
	>
		<div
			class="w-[460px] max-w-full rounded-xl border border-[var(--border-strong)] bg-[var(--panel-bg)] p-4 shadow-2xl"
		>
			<div class="flex items-start gap-3">
				<Icon
					name="warning"
					size={20}
					strokeWidth={1.9}
					class="mt-0.5 shrink-0 text-[var(--danger-text)]"
				/>
				<p id="soap-secure-templates-heading" class="text-sm leading-relaxed text-[var(--text)]">
					{$t('vars.secureWarning')}
				</p>
			</div>
			<div class="mt-4 flex flex-wrap items-center gap-2">
				<button
					type="button"
					onclick={() => void resolveSecureExport(true)}
					class="rounded-full bg-[var(--accent)] px-4 py-1.5 text-xs font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
				>
					{$t('vars.replace')}
				</button>
				<button
					type="button"
					onclick={() => void resolveSecureExport(false)}
					class="rounded-full bg-[var(--accent)] px-4 py-1.5 text-xs font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
				>
					{$t('vars.keepTemplates')}
				</button>
				<button
					type="button"
					onclick={() => {
						pendingExport = null;
						secureDialog = false;
					}}
					class="rounded-full px-4 py-1.5 text-xs font-bold text-[var(--text-muted)] transition-all duration-150 hover:scale-105 hover:bg-[var(--panel-bg-alt)] active:scale-95"
				>
					{$t('common.cancel')}
				</button>
			</div>
		</div>
	</div>
{/if}
{/if}
