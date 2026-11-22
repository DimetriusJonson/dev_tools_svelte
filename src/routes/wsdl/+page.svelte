<script lang="ts">
	import { onMount } from 'svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import WsdlExplorer from '$lib/components/WsdlExplorer.svelte';
	import { t } from '$lib/stores/i18n';
	import { isDark } from '$lib/stores/theme';
	import { notify, notifyError } from '$lib/stores/toast';
	import { cancelRestRequest, executeRestRequest, type RestResponse } from '$lib/services/rest';
	import {
		cancelWsdlFetch,
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
		saveWsdlSelection,
		type WsdlFileSummary,
		type WsdlOperation,
		type WsdlOperationDetail,
		type WsdlProject
	} from '$lib/services/wsdl';
	import { readClipboardText } from '$lib/utils/clipboard';
	import { formatRestSize, formatRestTime } from '$lib/utils/rest';
	import {
		SOAP_METHOD,
		buildEnvelope,
		describeErrorText,
		describeWarningText,
		editableForSave,
		lockedHeadersFor,
		mergeLockedHeaders,
		selectEndpoint
	} from '$lib/utils/wsdl';

	// The tree, the fetch and the SOAP call all live in the Rust backend.
	const desktop = isWsdlSupported();
	const dark = $derived($isDark);

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
	let endpointId = $state('');
	let dirty = $state(false);
	let response = $state<RestResponse | null>(null);

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
	const hasResponseText = $derived(
		!!response && !response.is_attachment && !response.is_binary && response.body !== ''
	);

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
			headers: editableForSave(target.soap_version, target.soap_action, target.state.headers),
			save_response: target.state.save_response
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
		dirty = false;
		response = null;
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
		endpointId = '';
		dirty = false;
		response = null;
	}

	function selectOperation(operationId: string): void {
		if (operationId === activeOperationId) return;
		activeOperationId = operationId;
		response = null;
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

		const userHeaders = editableForSave(
			loaded.soap_version,
			loaded.soap_action,
			loaded.state.headers
		);
		const body = current ? requestBody : loaded.state.request_body;

		running = true;
		aborting = false;
		try {
			const result = await executeRestRequest({
				method: SOAP_METHOD,
				url: target.url,
				headers: mergeLockedHeaders(
					lockedHeadersFor(loaded.soap_version, loaded.soap_action),
					userHeaders
				),
				body: buildEnvelope(body),
				body_type: 'xml',
				form: []
			});
			if (activeOperationId === operationId) response = result;
		} catch (error) {
			if (aborting || String(error).toLowerCase().includes('cancel')) {
				notify($t('rest.aborted'), 'info');
			} else {
				notifyError(error, $t('rest.requestFailed'));
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

<svelte:window onresize={onWindowResize} />

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
					<div class="flex min-w-0 flex-1 flex-col justify-center">
						<p class="truncate text-sm font-bold text-[var(--text)]">{detail.name}</p>
						<p class="truncate text-[11px] font-semibold text-[var(--text-muted)]">
							{detail.service_name} · SOAP {detail.soap_version === 'soap12' ? '1.2' : '1.1'}
						</p>
					</div>

					<div class="relative w-[280px] shrink-0">
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
						<div class="flex h-11 shrink-0 items-center gap-2 border-b border-[var(--border)] px-3">
							<span class="text-sm font-bold text-[var(--text)]">{$t('wsdl.request')}</span>
							<span class="text-xs font-semibold text-[var(--text-muted)]">
								{$t('wsdl.tabXml')}
							</span>
						</div>
						<div class="min-h-0 flex-1">
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
						<div class="flex h-11 shrink-0 items-center gap-2 border-b border-[var(--border)] px-3">
							<span class="text-sm font-bold text-[var(--text)]">{$t('wsdl.response')}</span>
							{#if response}
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.status')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">{response.status}</span>
								</span>
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.size')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{formatRestSize(response.size)}
									</span>
								</span>
								<span class="text-xs font-semibold text-[var(--text-muted)]">
									{$t('rest.time')}:
									<span class="ml-1 text-sm font-bold text-[var(--text)]">
										{formatRestTime(response.duration_ms)}
									</span>
								</span>
							{/if}
						</div>

						{#if response && !hasResponseText}
							<div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
								<Icon name="file" size={22} class="text-[var(--text-muted)]" />
								<p class="text-xs font-semibold text-[var(--text-muted)]">
									{response.content_type || $t('wsdl.noResponse')}
								</p>
							</div>
						{:else if hasResponseText}
							<div class="min-h-0 flex-1">
								<CodeMirrorEditor
									value={response?.body ?? ''}
									language={response?.language === 'html' ? 'text' : (response?.language ?? 'text')}
									dark={dark}
									readonly={true}
									ariaLabel={$t('wsdl.response')}
								/>
							</div>
						{:else}
							<div class="flex min-h-0 flex-1 items-center justify-center px-6 text-center">
								<p class="text-xs font-semibold text-[var(--text-muted)]">{$t('wsdl.noResponse')}</p>
							</div>
						{/if}
					</div>
				</div>
			{/if}
		</div>
	</div>
{/if}
