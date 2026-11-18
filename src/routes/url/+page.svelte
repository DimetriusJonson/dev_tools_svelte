<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { t } from '$lib/stores/i18n';
	import { copyText } from '$lib/utils/clipboard';
	import { decodeUrl, encodeUrl } from '$lib/utils/formatters';
	import { isTauriRuntime, loadEditorState, saveEditorState, saveTextFile } from '$lib/services/db';
	import { pickOutputFile } from '$lib/services/files';

	let source = $state('');
	let output = $state('');
	let topHeight = $state(430);
	let resizing = $state(false);
	let copied = $state(false);
	let hydrated = $state(false);
	let errorMessage = $state('');

	let copyTimer: ReturnType<typeof setTimeout> | undefined;

	function clampHeight(value: number): number {
		const max =
			typeof window === 'undefined' ? 900 : Math.max(180, window.innerHeight - 76 - 220);
		return Math.min(Math.max(value, 140), max);
	}

	function encode(): void {
		output = encodeUrl(source);
		errorMessage = '';
	}

	function decode(): void {
		output = decodeUrl(source);
		errorMessage = '';
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

	async function downloadOutput(): Promise<void> {
		if (!output) return;
		if (isTauriRuntime()) {
			try {
				const path = await pickOutputFile('result.txt', 'txt');
				if (!path) return;
				await saveTextFile(path, output);
			} catch (error) {
				errorMessage = String(error);
			}
			return;
		}
		const blob = new Blob([output], { type: 'text/plain;charset=utf-8' });
		const url = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = url;
		anchor.download = 'result.txt';
		anchor.click();
		URL.revokeObjectURL(url);
	}

	function startResize(event: PointerEvent): void {
		event.preventDefault();
		resizing = true;
		const startY = event.clientY;
		const startHeight = topHeight;

		const move = (moveEvent: PointerEvent) => {
			topHeight = clampHeight(startHeight + moveEvent.clientY - startY);
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
		topHeight = clampHeight(topHeight);
	}

	onMount(() => {
		let cancelled = false;
		(async () => {
			try {
				const saved = await loadEditorState('url');
				if (!cancelled && saved) source = saved.content;
			} catch {
				/* ignore */
			} finally {
				if (!cancelled) hydrated = true;
			}
		})();

		topHeight = clampHeight(Math.round((window.innerHeight - 76) * 0.46));

		return () => {
			cancelled = true;
			if (copyTimer) clearTimeout(copyTimer);
		};
	});

	$effect(() => {
		const content = source;
		if (!hydrated) return;
		const timer = setTimeout(() => {
			void saveEditorState({
				tool_key: 'url',
				content,
				indent: 4,
				opt_format: true,
				opt_unescape: true
			});
		}, 450);
		return () => clearTimeout(timer);
	});
</script>

<svelte:window onresize={onWindowResize} />

<div class="relative flex h-full min-h-0 flex-col px-3 pb-3 pt-3">
	<section
		class="min-h-0 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
		style="height: {topHeight}px;"
	>
		<textarea
			bind:value={source}
			class="h-full w-full resize-none bg-transparent px-4 py-3 font-mono text-[15px] leading-relaxed text-[var(--text)] outline-none placeholder:text-[var(--text-muted)]"
			placeholder={$t('url.sourcePlaceholder')}
			spellcheck="false"
			autocapitalize="off"
			autocorrect="off"
			aria-label={$t('url.sourcePlaceholder')}
		></textarea>
	</section>

	<div
		class="resizer-h {resizing ? 'active' : ''}"
		style="height: 12px;"
		role="separator"
		aria-orientation="horizontal"
		aria-label="Resize panes"
		onpointerdown={startResize}
	></div>

	<div class="flex shrink-0 items-center justify-center gap-4 pb-4 pt-1">
		<button
			type="button"
			onclick={encode}
			class="h-12 rounded-full bg-[var(--accent)] px-8 text-[15px] font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
		>
			{$t('url.encode')}
		</button>
		<button
			type="button"
			onclick={decode}
			class="h-12 rounded-full bg-[var(--accent)] px-8 text-[15px] font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
		>
			{$t('url.decode')}
		</button>
	</div>

	<section
		class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
	>
		<div class="flex h-11 shrink-0 items-center justify-end gap-1.5 px-3">
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
		<textarea
			readonly
			value={output}
			class="min-h-0 w-full flex-1 resize-none bg-transparent px-4 py-3 font-mono text-[15px] leading-relaxed text-[var(--text)] outline-none placeholder:text-[var(--text-muted)]"
			placeholder={$t('url.outputPlaceholder')}
			spellcheck="false"
			aria-label={$t('url.outputPlaceholder')}
		></textarea>
	</section>

	{#if errorMessage}
		<div
			class="fade-in pointer-events-none absolute bottom-24 left-1/2 z-30 max-w-[80%] -translate-x-1/2 rounded-full border border-red-400/40 bg-red-500/95 px-5 py-2.5 text-center text-sm font-semibold text-white shadow-lg"
		>
			{errorMessage}
		</div>
	{/if}
</div>
