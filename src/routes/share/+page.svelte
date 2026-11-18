<script lang="ts">
	import { get } from 'svelte/store';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import { t } from '$lib/stores/i18n';
	import { isDark } from '$lib/stores/theme';
	import { draftShare } from '$lib/stores/share';
	import { copyText } from '$lib/utils/clipboard';
	import { isTauriRuntime } from '$lib/services/db';
	import { basename, pickShareFile } from '$lib/services/files';
	import {
		formatBytes,
		getShareServerStatus,
		listShareInterfaces,
		restartShareServer,
		uploadShareContent,
		uploadShareFile,
		type NetworkInterface,
		type ShareServerStatus,
		type ShareUploadResult,
		MAX_UPLOAD_BYTES
	} from '$lib/services/share';

	type ContentLanguage = 'xml' | 'json' | 'text';

	let desktop = $state(false);
	let loading = $state(true);
	let interfaces = $state<NetworkInterface[]>([]);
	let server = $state('');
	let serverStatus = $state<ShareServerStatus | null>(null);
	let mode = $state<'file' | 'content'>('file');
	let selectedPath = $state<string | null>(null);
	let selectedName = $state('');
	let content = $state('');
	let contentName = $state('snippet.json');
	let contentLanguage = $state<ContentLanguage>('json');
	let draftReadOnly = $state(false);
	let uploading = $state(false);
	let retrying = $state(false);
	let result = $state<ShareUploadResult | null>(null);
	let errorMessage = $state('');
	let dragging = $state(false);
	let copied = $state(false);

	let copyTimer: ReturnType<typeof setTimeout> | undefined;

	const isDarkTheme = $derived($isDark);
	const contentType = $derived(
		contentLanguage === 'xml'
			? 'application/xml'
			: contentLanguage === 'json'
				? 'application/json'
				: 'text/plain'
	);
	const canUpload = $derived(
		Boolean(server) && serverStatus !== null && serverStatus.running && !uploading
	);
	const serverFailed = $derived(serverStatus !== null && !serverStatus.running);

	function serverIsReady(): boolean {
		if (serverStatus !== null && !serverStatus.running) {
			flashError($t('share.serverUnavailable'));
			return false;
		}
		return true;
	}

	async function restartServer(): Promise<void> {
		retrying = true;
		errorMessage = '';
		try {
			serverStatus = await restartShareServer();
			if (serverStatus && !serverStatus.running) {
				flashError(serverStatus.error ?? $t('share.serverUnavailable'));
			}
		} catch (error) {
			flashError(String(error));
		} finally {
			retrying = false;
		}
	}

	function flashError(message: string): void {
		errorMessage = message;
	}

	function setLanguage(language: ContentLanguage): void {
		contentLanguage = language;
		const extension = language === 'xml' ? 'xml' : language === 'json' ? 'json' : 'txt';
		if (!contentName || /^snippet\.(xml|json|txt)$/.test(contentName)) {
			contentName = `snippet.${extension}`;
		}
	}

	async function selectFile(): Promise<void> {
		try {
			const path = await pickShareFile();
			if (!path) return;
			selectedPath = path;
			selectedName = basename(path);
			result = null;
			errorMessage = '';
		} catch (error) {
			flashError(String(error));
		}
	}

	async function shareFilePath(path: string): Promise<void> {
		if (!server) {
			flashError($t('share.serverRequired'));
			return;
		}
		if (!serverIsReady()) return;
		uploading = true;
		result = null;
		errorMessage = '';
		try {
			result = await uploadShareFile(path, server);
		} catch (error) {
			flashError(String(error));
		} finally {
			uploading = false;
		}
	}

	async function shareSelectedFile(): Promise<void> {
		if (!selectedPath) {
			await selectFile();
			if (!selectedPath) return;
		}
		await shareFilePath(selectedPath);
	}

	async function shareContent(): Promise<void> {
		if (!content.trim()) {
			flashError($t('share.emptyContent'));
			return;
		}
		const size = new TextEncoder().encode(content).length;
		if (size > MAX_UPLOAD_BYTES) {
			flashError($t('share.tooLarge', { size: formatBytes(MAX_UPLOAD_BYTES) }));
			return;
		}
		if (!server) {
			flashError($t('share.serverRequired'));
			return;
		}
		if (!serverIsReady()) return;

		uploading = true;
		result = null;
		errorMessage = '';
		try {
			const name = contentName.trim() || 'snippet.txt';
			result = await uploadShareContent(content, name, contentType, server);
		} catch (error) {
			flashError(String(error));
		} finally {
			uploading = false;
		}
	}

	async function copyLink(): Promise<void> {
		if (!result) return;
		await copyText(result.link);
		copied = true;
		if (copyTimer) clearTimeout(copyTimer);
		copyTimer = setTimeout(() => {
			copied = false;
		}, 1500);
	}

	onMount(() => {
		desktop = isTauriRuntime();
		let unlistenDragDrop: (() => void) | undefined;

		(async () => {
			if (!desktop) return;
			try {
				const [availableInterfaces, status] = await Promise.all([
					listShareInterfaces(),
					getShareServerStatus()
				]);
				interfaces = availableInterfaces;
				serverStatus = status;
				server = availableInterfaces[0]?.url ?? '';

				const draft = get(draftShare);
				if (draft) {
					mode = 'content';
					content = draft.content;
					contentName = draft.filename;
					contentLanguage = draft.language;
					draftReadOnly = draft.readOnly === true;
					draftShare.set(null);
				}

				const { getCurrentWebview } = await import('@tauri-apps/api/webview');
				unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
					if (event.payload.type === 'drop') {
						dragging = false;
						const path = event.payload.paths[0];
						if (path && !uploading) {
							mode = 'file';
							draftReadOnly = false;
							selectedPath = path;
							selectedName = basename(path);
							void shareFilePath(path);
						}
					} else if (event.payload.type === 'leave') {
						dragging = false;
					} else {
						dragging = true;
					}
				});
			} catch (error) {
				flashError(String(error));
			} finally {
				loading = false;
			}
		})();

		return () => {
			unlistenDragDrop?.();
			if (copyTimer) clearTimeout(copyTimer);
		};
	});
</script>

<div class="h-full overflow-y-auto px-4 py-5">
	<div class="mx-auto flex w-full max-w-4xl flex-col gap-4">
		<section
			class="fade-in flex flex-col gap-4 rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-6 shadow-[var(--shadow)] sm:flex-row sm:items-center sm:justify-between"
		>
			<div class="flex items-center gap-4">
				<div
					class="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl bg-[var(--accent)] text-[var(--accent-text)]"
				>
					<Icon name="share" size={28} strokeWidth={2} />
				</div>
				<div>
					<h1 class="text-2xl font-bold text-[var(--text)]">{$t('share.title')}</h1>
					<p class="mt-1 max-w-2xl text-sm leading-relaxed text-[var(--text-muted)]">
						{$t('share.description')}
					</p>
				</div>
			</div>
			<span
				class="inline-flex shrink-0 items-center gap-2 rounded-full px-3 py-1.5 text-xs font-bold text-[var(--accent)]"
				style="background-color: color-mix(in srgb, var(--accent) 16%, transparent);"
			>
				<Icon name="sparkles" size={14} />
				{$t('share.localOnly')}
			</span>
		</section>

		{#if !desktop}
			<section class="rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-6">
				<p class="text-sm font-semibold text-[var(--text)]">{$t('share.desktopOnly')}</p>
			</section>
		{:else}
			<section class="rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-5 shadow-[var(--shadow)]">
				<div class="mb-3 flex items-center gap-2 text-sm font-bold text-[var(--text)]">
					<Icon name="server" size={18} />
					{$t('share.server')}
				</div>
				<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
					<label class="sr-only" for="share-server">{$t('share.server')}</label>
					<select
						id="share-server"
						bind:value={server}
						class="h-11 w-full rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
					>
						{#each interfaces as item (item.url)}
							<option value={item.url}>
								{item.name} — {item.url}{item.is_loopback ? ` (${$t('share.loopback')})` : ''}
							</option>
						{/each}
					</select>
					<div class="flex flex-wrap items-center gap-2">
						<span
							class="inline-flex h-9 items-center gap-2 rounded-full px-3 text-xs font-bold {serverStatus?.running
								? 'text-emerald-500'
								: 'text-amber-500'}"
							style="background-color: color-mix(in srgb, currentColor 12%, transparent);"
						>
							<span class="h-2 w-2 rounded-full bg-current"></span>
							{serverStatus?.running ? $t('share.serverRunning') : $t('share.serverError')}
						</span>
						{#if serverFailed}
							<button
								type="button"
								onclick={restartServer}
								disabled={retrying}
								class="inline-flex h-9 items-center rounded-full bg-[var(--secondary-bg)] px-4 text-xs font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
							>
								{retrying ? $t('share.retrying') : $t('share.retry')}
							</button>
						{/if}
					</div>
				</div>
				{#if serverFailed}
					{#if serverStatus?.error}
						<p class="mt-2 text-xs font-medium text-amber-500">{serverStatus.error}</p>
					{/if}
					<p class="mt-2 text-xs font-medium text-amber-500">{$t('share.serverUnavailable')}</p>
				{:else if interfaces.find((item) => item.url === server)?.is_loopback}
					<p class="mt-2 text-xs text-[var(--text-muted)]">{$t('share.loopbackHint')}</p>
				{:else}
					<p class="mt-2 text-xs text-[var(--text-muted)]">{$t('share.serverHint')}</p>
				{/if}
			</section>

			<section class="rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-5 shadow-[var(--shadow)]">
				<div class="mb-4 flex gap-2">
					<button
						type="button"
						onclick={() => (mode = 'file')}
						class="rounded-full px-4 py-2 text-sm font-bold transition-all duration-150 active:scale-95 {mode === 'file'
							? 'bg-[var(--accent)] text-[var(--accent-text)]'
							: 'bg-[var(--secondary-bg)] text-[var(--secondary-text)]'}"
					>
						{$t('share.fileMode')}
					</button>
					<button
						type="button"
						onclick={() => (mode = 'content')}
						class="rounded-full px-4 py-2 text-sm font-bold transition-all duration-150 active:scale-95 {mode === 'content'
							? 'bg-[var(--accent)] text-[var(--accent-text)]'
							: 'bg-[var(--secondary-bg)] text-[var(--secondary-text)]'}"
					>
						{$t('share.contentMode')}
					</button>
				</div>

				{#if mode === 'file'}
					<div class="grid gap-3">
						<button
							type="button"
							onclick={selectFile}
							class="flex min-h-36 w-full flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed px-5 py-6 text-center transition-all duration-150 hover:scale-[1.01] active:scale-[0.99] {dragging
								? 'border-[var(--accent)] bg-[var(--accent)]/10'
								: 'border-[var(--border-strong)] bg-[var(--app-bg)]'}"
						>
							<span
								class="flex h-11 w-11 items-center justify-center rounded-xl bg-[var(--secondary-bg)] text-[var(--secondary-text)]"
							>
								<Icon name="file" size={22} />
							</span>
							<span class="text-sm font-bold text-[var(--text)]">
								{dragging ? $t('share.dropActive') : $t('share.dropHint')}
							</span>
							<span class="text-xs text-[var(--text-muted)]">
								{$t('share.limit', { size: formatBytes(MAX_UPLOAD_BYTES) })}
							</span>
						</button>

						<div class="flex flex-wrap items-center gap-3">
							<button
								type="button"
								onclick={selectFile}
								class="rounded-full bg-[var(--secondary-bg)] px-5 py-2.5 text-sm font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
							>
								{$t('common.selectFile')}
							</button>
							<span class="min-w-0 flex-1 truncate text-sm font-semibold text-[var(--text)]">
								{selectedName || $t('common.noFile')}
							</span>
							<button
								type="button"
								onclick={shareSelectedFile}
								disabled={!canUpload}
								class="rounded-full bg-[var(--accent)] px-5 py-2.5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
							>
								{uploading ? $t('share.uploading') : $t('share.uploadFile')}
							</button>
						</div>
					</div>
				{:else}
					<div class="grid gap-3">
						<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_160px]">
							<label class="flex flex-col gap-1.5 text-xs font-bold text-[var(--text-muted)]">
								{$t('share.filename')}
								<input
									bind:value={contentName}
									class="h-10 rounded-lg border border-[var(--border-strong)] bg-[var(--app-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
								/>
							</label>
							<label class="flex flex-col gap-1.5 text-xs font-bold text-[var(--text-muted)]">
								{$t('share.language')}
								<select
									value={contentLanguage}
									onchange={(event) =>
										setLanguage((event.currentTarget as HTMLSelectElement).value as ContentLanguage)}
									class="h-10 rounded-lg border border-[var(--border-strong)] bg-[var(--app-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
								>
									<option value="xml">XML</option>
									<option value="json">JSON</option>
									<option value="text">{$t('share.plainText')}</option>
								</select>
							</label>
						</div>
						<div
							class="h-64 overflow-hidden rounded-xl border border-[var(--border)] bg-[var(--app-bg)]"
						>
							<CodeMirrorEditor
								value={content}
								language={contentLanguage}
								readonly={draftReadOnly}
								dark={isDarkTheme}
								placeholder={draftReadOnly ? '' : $t('share.contentPlaceholder')}
								ariaLabel={$t('share.contentMode')}
								onchange={(next) => (content = next)}
							/>
						</div>
						<div class="flex flex-wrap items-center justify-between gap-3">
							<span class="text-xs text-[var(--text-muted)]">
								{draftReadOnly
									? $t('share.draftHint')
									: $t('share.limit', { size: formatBytes(MAX_UPLOAD_BYTES) })}
							</span>
							<button
								type="button"
								onclick={shareContent}
								disabled={!canUpload}
								class="rounded-full bg-[var(--accent)] px-5 py-2.5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
							>
								{uploading ? $t('share.uploading') : $t('share.uploadContent')}
							</button>
						</div>
					</div>
				{/if}
			</section>

			{#if result}
				<section
					class="fade-in rounded-2xl border border-[var(--accent)]/40 bg-[var(--panel-bg)] p-5 shadow-[var(--shadow)]"
				>
					<div class="mb-2 flex items-center gap-2 text-sm font-bold text-[var(--text)]">
						<Icon name="check" size={18} strokeWidth={3} class="text-emerald-500" />
						{$t('share.ready')}
					</div>
					<div class="flex flex-col gap-2 sm:flex-row">
						<input
							readonly
							value={result.link}
							class="h-11 min-w-0 flex-1 rounded-lg border border-[var(--border-strong)] bg-[var(--app-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
							aria-label={$t('share.link')}
							onfocus={(event) => (event.currentTarget as HTMLInputElement).select()}
						/>
						<button
							type="button"
							onclick={copyLink}
							class="inline-flex h-11 shrink-0 items-center justify-center gap-2 rounded-full bg-[var(--accent)] px-5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
						>
							<Icon name={copied ? 'check' : 'copy'} size={17} strokeWidth={copied ? 3 : 2} />
							{copied ? $t('common.copied') : $t('share.copyLink')}
						</button>
					</div>
					<p class="mt-2 text-xs text-[var(--text-muted)]">
						{$t('share.linkHint', { size: formatBytes(result.size) })}
					</p>
				</section>
			{/if}
		{/if}

		{#if errorMessage}
			<div
				class="fade-in rounded-xl border border-red-400/40 bg-red-500/95 px-4 py-3 text-sm font-semibold text-white"
			>
				{errorMessage}
			</div>
		{/if}

		{#if loading}
			<p class="px-1 text-sm text-[var(--text-muted)]">{$t('share.loading')}</p>
		{/if}
	</div>
</div>
