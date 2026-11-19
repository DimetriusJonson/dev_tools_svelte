<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import Icon from '$lib/components/Icon.svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import { isDark } from '$lib/stores/theme';
	import { t } from '$lib/stores/i18n';
	import { draftShare } from '$lib/stores/share';
	import { notifyError } from '$lib/stores/toast';
	import { copyText } from '$lib/utils/clipboard';
	import {
		escapeContent,
		formatContent,
		type FormatOptions,
		type Mode
	} from '$lib/utils/formatters';
	import {
		isTauriRuntime,
		loadEditorState,
		processLargeFile,
		saveEditorState,
		saveTextFile
	} from '$lib/services/db';
	import {
		basename,
		formattedPath,
		pickInputFile,
		pickOutputFile
	} from '$lib/services/files';

	let { mode }: { mode: Mode } = $props();

	let source = $state('');
	let output = $state('');
	let indent = $state(4);
	let doFormat = $state(true);
	let doUnescape = $state(true);
	let selectedFile = $state<string | null>(null);
	let fileName = $state('');
	let status = $state('');
	let busy = $state(false);
	let copied = $state(false);
	let hydrated = $state(false);
	let leftWidth = $state(720);
	let resizing = $state(false);
	let fileInput = $state<HTMLInputElement | null>(null);

	let copyTimer: ReturnType<typeof setTimeout> | undefined;

	const isDarkTheme = $derived($isDark);

	function clampWidth(value: number): number {
		const max = typeof window === 'undefined' ? 1200 : Math.max(360, window.innerWidth - 430);
		return Math.min(Math.max(value, 320), max);
	}

	function currentOptions(): FormatOptions {
		return { indent, doFormat, doUnescape };
	}

	function flashError(message: string): void {
		notifyError(message);
	}

	function execute(): void {
		try {
			output = formatContent(mode, source, currentOptions());
			status = '';
		} catch {
			output = '';
			flashError($t('formatter.invalid'));
		}
	}

	function escapeSource(): void {
		try {
			output = escapeContent(mode, source);
			status = '';
		} catch {
			output = '';
			flashError($t('formatter.invalid'));
		}
	}

	async function copyOutput(): Promise<void> {
		if (!output) return;
		await copyText(output);
		copied = true;
		if (copyTimer) clearTimeout(copyTimer);
		copyTimer = setTimeout(() => {
			copied = false;
		}, 1500);
	}

	function shareOutput(): void {
		if (!output) return;
		draftShare.set({
			content: output,
			filename: `snippet.${mode}`,
			contentType: mode === 'xml' ? 'application/xml' : 'application/json',
			language: mode,
			readOnly: true
		});
		void goto('/share');
	}

	async function downloadOutput(): Promise<void> {
		if (!output) return;
		if (isTauriRuntime()) {
			try {
				const path = await pickOutputFile(`result.${mode}`, mode);
				if (!path) return;
				await saveTextFile(path, output);
				status = $t('formatter.saved', { path });
			} catch (error) {
				flashError(String(error));
			}
			return;
		}

		const blob = new Blob([output], {
			type: mode === 'xml' ? 'application/xml;charset=utf-8' : 'application/json;charset=utf-8'
		});
		const url = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = url;
		anchor.download = `result.${mode}`;
		anchor.click();
		URL.revokeObjectURL(url);
	}

	async function selectFile(): Promise<void> {
		if (!isTauriRuntime()) {
			fileInput?.click();
			return;
		}
		try {
			const path = await pickInputFile(mode);
			if (!path) return;
			selectedFile = path;
			fileName = basename(path);
			status = fileName;
		} catch (error) {
			flashError(String(error));
		}
	}

	async function onFilePicked(event: Event): Promise<void> {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		try {
			source = await file.text();
			fileName = file.name;
			selectedFile = null;
			status = file.name;
		} catch {
			flashError($t('common.error'));
		}
	}

	async function formatSelectedFile(): Promise<void> {
		if (!isTauriRuntime()) {
			if (!fileName) {
				fileInput?.click();
				return;
			}
			try {
				output = formatContent(mode, source, currentOptions());
				status = fileName;
			} catch {
				output = '';
				flashError($t('formatter.invalid'));
			}
			return;
		}

		if (!selectedFile) {
			await selectFile();
			return;
		}

		busy = true;
		try {
			const destination = await pickOutputFile(formattedPath(selectedFile, mode), mode);
			if (!destination) return;
			const result = await processLargeFile({
				input_path: selectedFile,
				output_path: destination,
				mode,
				indent,
				do_format: doFormat,
				do_unescape: doUnescape,
				do_escape: false
			});
			status = $t('formatter.fileDone', { path: result.output_path });
		} catch (error) {
			flashError($t('formatter.fileError', { message: String(error) }));
		} finally {
			busy = false;
		}
	}

	function startResize(event: PointerEvent): void {
		event.preventDefault();
		resizing = true;
		const startX = event.clientX;
		const startWidth = leftWidth;

		const move = (moveEvent: PointerEvent) => {
			leftWidth = clampWidth(startWidth + moveEvent.clientX - startX);
		};
		const stop = () => {
			resizing = false;
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', stop);
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', stop);
	}

	function onWindowResize(): void {
		leftWidth = clampWidth(leftWidth);
	}

	onMount(() => {
		let cancelled = false;
		(async () => {
			try {
				const saved = await loadEditorState(mode);
				if (!cancelled && saved) {
					source = saved.content;
					indent = saved.indent;
					doFormat = saved.opt_format;
					doUnescape = saved.opt_unescape;
				}
			} catch {
				/* keep defaults */
			} finally {
				if (!cancelled) hydrated = true;
			}
		})();

		leftWidth = clampWidth((window.innerWidth - 270) / 2);

		return () => {
			cancelled = true;
			if (copyTimer) clearTimeout(copyTimer);
		};
	});

	$effect(() => {
		const payload = {
			tool_key: mode,
			content: source,
			indent,
			opt_format: doFormat,
			opt_unescape: doUnescape
		};
		if (!hydrated) return;
		const timer = setTimeout(() => {
			void saveEditorState(payload);
		}, 450);
		return () => clearTimeout(timer);
	});

	const placeholderSource = $derived(
		mode === 'xml' ? $t('formatter.sourcePlaceholder') : $t('formatter.sourcePlaceholderJson')
	);
</script>

<svelte:window onresize={onWindowResize} />

<div class="relative flex h-full min-h-0 flex-col px-3 pb-3 pt-3">
	<div
		class="grid min-h-0 flex-1"
		style="grid-template-columns: {leftWidth}px 10px 232px 14px minmax(260px, 1fr); grid-template-rows: minmax(0, 1fr);"
	>
		<section
			class="relative flex min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
			style="grid-column: 1; grid-row: 1;"
			aria-label={$t('formatter.source')}
		>
			<div class="flex h-11 shrink-0 items-center gap-2 px-3">
				<input
					bind:this={fileInput}
					type="file"
					accept={mode === 'xml' ? '.xml,text/xml,application/xml' : '.json,application/json'}
					class="hidden"
					onchange={onFilePicked}
				/>
				<button
					type="button"
					onclick={selectFile}
					class="flex h-8 shrink-0 items-center rounded-full bg-[var(--secondary-bg)] px-4 text-[13px] font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
				>
					{$t('common.selectFile')}
				</button>
				<span
					class="min-w-0 flex-1 truncate text-[13px] font-semibold text-[var(--text)]"
					title={status}
				>
					{status || $t('common.noFile')}
				</span>
				<button
					type="button"
					onclick={formatSelectedFile}
					disabled={busy}
					class="flex h-8 shrink-0 items-center rounded-full bg-[var(--accent)] px-4 text-[13px] font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95 disabled:cursor-not-allowed disabled:opacity-60"
				>
					{busy ? $t('common.formatting') : $t('common.format')}
				</button>
			</div>
			<div class="min-h-0 flex-1">
				<CodeMirrorEditor
					value={source}
					language={mode}
					dark={isDarkTheme}
					placeholder={placeholderSource}
					ariaLabel={$t('formatter.source')}
					onchange={(next) => (source = next)}
				/>
			</div>
		</section>

		<div
			class="resizer-v {resizing ? 'active' : ''}"
			style="grid-column: 2; grid-row: 1;"
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize panes"
			onpointerdown={startResize}
		></div>

		<div
			class="flex min-h-0 flex-col items-center justify-center gap-5 px-1"
			style="grid-column: 3; grid-row: 1;"
		>
			<div class="relative w-full">
				<label class="sr-only" for={`indent-${mode}`}>{$t('formatter.indent', { n: indent })}</label>
				<select
					id={`indent-${mode}`}
					bind:value={indent}
					class="h-12 w-full appearance-none rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-4 pr-10 text-[15px] font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
				>
					<option value={2}>{$t('formatter.indent', { n: 2 })}</option>
					<option value={3}>{$t('formatter.indent', { n: 3 })}</option>
					<option value={4}>{$t('formatter.indent', { n: 4 })}</option>
				</select>
				<Icon
					name="chevronDown"
					size={18}
					class="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
				/>
			</div>

			<div class="flex w-full flex-col gap-4">
				<label class="group flex cursor-pointer select-none items-center gap-3">
					<input type="checkbox" bind:checked={doFormat} class="peer sr-only" />
					<span
						class="flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] border-2 transition-all duration-150 peer-focus-visible:ring-2 peer-focus-visible:ring-[var(--accent)] peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-[var(--app-bg)] {doFormat
							? 'border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-text)]'
							: 'border-[var(--border-strong)] bg-transparent text-transparent'}"
					>
						<Icon name="check" size={18} strokeWidth={3} />
					</span>
					<span class="text-[15px] font-semibold text-[var(--text)]">
						{$t('formatter.format')}
					</span>
				</label>

				<label class="group flex cursor-pointer select-none items-center gap-3">
					<input type="checkbox" bind:checked={doUnescape} class="peer sr-only" />
					<span
						class="flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] border-2 transition-all duration-150 peer-focus-visible:ring-2 peer-focus-visible:ring-[var(--accent)] peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-[var(--app-bg)] {doUnescape
							? 'border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-text)]'
							: 'border-[var(--border-strong)] bg-transparent text-transparent'}"
					>
						<Icon name="check" size={18} strokeWidth={3} />
					</span>
					<span class="text-[15px] font-semibold text-[var(--text)]">
						{$t('formatter.unescape')}
					</span>
				</label>
			</div>

			<button
				type="button"
				onclick={execute}
				class="flex h-12 w-full items-center justify-center rounded-full bg-[var(--accent)] text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
				aria-label={$t('formatter.format')}
				title={$t('formatter.format')}
			>
				<Icon name="arrowRight" size={24} strokeWidth={2.4} />
			</button>

			<button
				type="button"
				onclick={escapeSource}
				class="h-12 w-full rounded-full bg-[var(--accent)] px-3 text-[15px] font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
			>
				{$t('common.escape')}
			</button>
		</div>

		<div style="grid-column: 4; grid-row: 1;" aria-hidden="true"></div>

		<section
			class="relative flex min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
			style="grid-column: 5; grid-row: 1;"
			aria-label={$t('formatter.output')}
		>
			<div class="flex h-11 shrink-0 items-center justify-end gap-1.5 px-3">
				<button
					type="button"
					onclick={shareOutput}
					class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
					title={$t('share.shareResult')}
					aria-label={$t('share.shareResult')}
					disabled={!output}
				>
					<Icon name="share" size={19} strokeWidth={1.9} />
				</button>
				<button
					type="button"
					onclick={copyOutput}
					class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95 {copied
						? 'text-[var(--accent)]'
						: ''}"
					title={copied ? $t('common.copied') : $t('common.copy')}
					aria-label={copied ? $t('common.copied') : $t('common.copy')}
					disabled={!output}
				>
					<Icon name={copied ? 'check' : 'copy'} size={19} strokeWidth={copied ? 3 : 1.9} />
				</button>
				<button
					type="button"
					onclick={downloadOutput}
					class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:bg-white/5 hover:text-[var(--text)] active:scale-95"
					title={$t('common.download')}
					aria-label={$t('common.download')}
					disabled={!output}
				>
					<Icon name="download" size={19} strokeWidth={1.9} />
				</button>
			</div>
			<div class="min-h-0 flex-1">
				<CodeMirrorEditor
					value={output}
					language={mode}
					dark={isDarkTheme}
					readonly={true}
					placeholder={$t('formatter.outputPlaceholder')}
					ariaLabel={$t('formatter.output')}
				/>
			</div>
		</section>
	</div>

</div>
