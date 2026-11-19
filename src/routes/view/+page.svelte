<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import Icon from '$lib/components/Icon.svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import { t } from '$lib/stores/i18n';
	import { isDark } from '$lib/stores/theme';
	import { notifyError } from '$lib/stores/toast';
	import { copyText } from '$lib/utils/clipboard';
	import { isTauriRuntime } from '$lib/services/db';
	import { pickSaveFile } from '$lib/services/files';
	import {
		downloadSharedUpload,
		formatBytes,
		loadSharedUpload,
		type SharedUploadResponse
	} from '$lib/services/share';

	let upload = $state<SharedUploadResponse | null>(null);
	let loading = $state(true);
	let downloading = $state(false);
	let errorMessage = $state('');
	let copied = $state(false);
	let status = $state('');

	const isDarkTheme = $derived($isDark);
	const server = $derived($page.url.searchParams.get('server') ?? '');
	const id = $derived($page.url.searchParams.get('id') ?? '');
	const language = $derived(upload?.language ?? 'text');

	let copyTimer: ReturnType<typeof setTimeout> | undefined;

	async function load(targetServer = server, targetId = id): Promise<void> {
		loading = true;
		errorMessage = '';
		status = '';
		upload = null;

		if (!isTauriRuntime()) {
			errorMessage = $t('viewer.desktopOnly');
			notifyError(errorMessage);
			loading = false;
			return;
		}
		if (!targetServer || !targetId) {
			errorMessage = $t('viewer.invalidLink');
			notifyError(errorMessage);
			loading = false;
			return;
		}

		try {
			upload = await loadSharedUpload(targetServer, targetId);
		} catch (error) {
			errorMessage = String(error);
			notifyError(error);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const nextServer = server;
		const nextId = id;
		void load(nextServer, nextId);
	});

	async function copyContent(): Promise<void> {
		if (!upload?.content) return;
		await copyText(upload.content);
		copied = true;
		if (copyTimer) clearTimeout(copyTimer);
		copyTimer = setTimeout(() => {
			copied = false;
		}, 1500);
	}

	async function download(): Promise<void> {
		if (!upload) return;
		try {
			const path = await pickSaveFile(upload.filename);
			if (!path) return;
			downloading = true;
			status = '';
			await downloadSharedUpload(server, id, path);
			status = $t('viewer.downloaded', { path });
		} catch (error) {
			errorMessage = String(error);
			notifyError(error);
		} finally {
			downloading = false;
		}
	}

	onMount(() => {
		return () => {
			if (copyTimer) clearTimeout(copyTimer);
		};
	});
</script>

<div class="flex h-full min-h-0 flex-col gap-3 px-4 py-4">
	<div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
		<button
			type="button"
			onclick={() => goto('/share')}
			class="inline-flex items-center gap-2 rounded-full bg-[var(--secondary-bg)] px-4 py-2 text-sm font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
		>
			<Icon name="arrowRight" size={17} class="rotate-180" />
			{$t('viewer.back')}
		</button>
		{#if upload}
			<div class="flex min-w-0 flex-wrap items-center justify-end gap-2">
				<span class="truncate text-sm font-bold text-[var(--text)]" title={upload.filename}>
					{upload.filename}
				</span>
				<span class="text-xs text-[var(--text-muted)]">
					{formatBytes(upload.size)} · {upload.content_type}
				</span>
			</div>
		{/if}
	</div>

	{#if loading}
		<section
			class="flex flex-1 items-center justify-center rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)]"
		>
			<p class="text-sm font-semibold text-[var(--text-muted)]">{$t('viewer.loading')}</p>
		</section>
	{:else if errorMessage}
		<section
			class="flex flex-1 flex-col items-center justify-center gap-4 rounded-2xl border border-red-400/40 bg-[var(--panel-bg)] p-6 text-center"
		>
			<p class="max-w-xl text-sm font-semibold text-red-500">{errorMessage}</p>
			<div class="flex flex-wrap justify-center gap-2">
				<button
					type="button"
					onclick={() => load()}
					class="rounded-full bg-[var(--accent)] px-5 py-2.5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 active:scale-95"
				>
					{$t('viewer.retry')}
				</button>
				<button
					type="button"
					onclick={() => goto('/share')}
					class="rounded-full bg-[var(--secondary-bg)] px-5 py-2.5 text-sm font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
				>
					{$t('viewer.back')}
				</button>
			</div>
		</section>
	{:else if upload?.viewer === 'binary'}
		<section
			class="flex flex-1 flex-col items-center justify-center gap-5 rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-8 text-center shadow-[var(--shadow)]"
		>
			<div
				class="flex h-16 w-16 items-center justify-center rounded-2xl bg-[var(--secondary-bg)] text-[var(--secondary-text)]"
			>
				<Icon name="file" size={30} />
			</div>
			<div>
				<h1 class="text-xl font-bold text-[var(--text)]">{$t('viewer.binary')}</h1>
				<p class="mt-2 text-sm text-[var(--text-muted)]">
					{$t('viewer.binaryHint')}
				</p>
			</div>
			<dl class="grid w-full max-w-md grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-left text-sm">
				<dt class="font-semibold text-[var(--text-muted)]">{$t('viewer.filename')}</dt>
				<dd class="truncate font-semibold text-[var(--text)]">{upload.filename}</dd>
				<dt class="font-semibold text-[var(--text-muted)]">{$t('viewer.type')}</dt>
				<dd class="font-semibold text-[var(--text)]">{upload.content_type}</dd>
				<dt class="font-semibold text-[var(--text-muted)]">{$t('viewer.size')}</dt>
				<dd class="font-semibold text-[var(--text)]">{formatBytes(upload.size)}</dd>
				<dt class="font-semibold text-[var(--text-muted)]">{$t('viewer.created')}</dt>
				<dd class="font-semibold text-[var(--text)]">{upload.created_at}</dd>
			</dl>
			<button
				type="button"
				onclick={download}
				disabled={downloading}
				class="inline-flex items-center gap-2 rounded-full bg-[var(--accent)] px-5 py-2.5 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
			>
				<Icon name="download" size={17} />
				{downloading ? $t('viewer.downloading') : $t('viewer.download')}
			</button>
		</section>
	{:else if upload}
		<section
			class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
		>
			<div class="flex h-12 shrink-0 items-center justify-between gap-3 px-4">
				<div class="flex min-w-0 items-center gap-2 text-xs font-semibold text-[var(--text-muted)]">
					<span class="rounded-full bg-[var(--secondary-bg)] px-2.5 py-1 uppercase text-[var(--secondary-text)]">
						{upload.language ?? 'text'}
					</span>
					<span class="truncate">{upload.filename}</span>
					<span>·</span>
					<span>{formatBytes(upload.size)}</span>
				</div>
				<div class="flex items-center gap-1.5">
					<button
						type="button"
						onclick={copyContent}
						class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 {copied
							? 'text-[var(--accent)]'
							: ''}"
						title={copied ? $t('common.copied') : $t('common.copy')}
						aria-label={copied ? $t('common.copied') : $t('common.copy')}
					>
						<Icon name={copied ? 'check' : 'copy'} size={19} strokeWidth={copied ? 3 : 1.9} />
					</button>
					<button
						type="button"
						onclick={download}
						disabled={downloading}
						class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
						title={$t('viewer.download')}
						aria-label={$t('viewer.download')}
					>
						<Icon name="download" size={19} strokeWidth={1.9} />
					</button>
				</div>
			</div>
			<div class="min-h-0 flex-1">
				<CodeMirrorEditor
					value={upload.content ?? ''}
					language={language}
					dark={isDarkTheme}
					readonly={true}
					ariaLabel={$t('viewer.content')}
				/>
			</div>
		</section>
	{/if}

	{#if status}
		<p class="shrink-0 truncate px-1 text-xs font-semibold text-emerald-500" title={status}>{status}</p>
	{/if}
</div>
