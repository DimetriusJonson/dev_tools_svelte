<script lang="ts">
	import { onMount } from 'svelte';
	import CodeMirrorEditor from '$lib/components/CodeMirrorEditor.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { isDark } from '$lib/stores/theme';
	import { t } from '$lib/stores/i18n';
	import { diffTexts, lineOffsets, splitLines, type LineHighlight } from '$lib/utils/diff';
	import { loadEditorState, saveEditorState } from '$lib/services/db';

	type Language = 'xml' | 'json' | 'text';

	interface Highlight {
		from: number;
		to: number;
		class: string;
		line?: boolean;
	}

	let leftText = $state('');
	let rightText = $state('');
	let summary = $state('');
	let hydrated = $state(false);
	let leftEditor = $state<any>(null);
	let rightEditor = $state<any>(null);

	const dark = $derived($isDark);

	function detectLanguage(text: string): Language {
		const trimmed = text.trimStart();
		if (trimmed.startsWith('<')) return 'xml';
		if (trimmed.startsWith('{') || trimmed.startsWith('[')) return 'json';
		return 'text';
	}

	const leftLanguage = $derived(detectLanguage(leftText));
	const rightLanguage = $derived(detectLanguage(rightText));

	function buildHighlights(text: string, highlights: LineHighlight[], side: 'left' | 'right'): Highlight[] {
		const offsets = lineOffsets(text);
		const lines = splitLines(text);
		const output: Highlight[] = [];
		const lineClass = side === 'left' ? 'cm-diff-line-removed' : 'cm-diff-line-added';
		const markClass = side === 'left' ? 'cm-diff-mark-removed' : 'cm-diff-mark-added';

		for (const highlight of highlights) {
			const start = offsets[highlight.line - 1] ?? 0;
			const lineLength = (lines[highlight.line - 1] ?? '').length;
			output.push({ from: start, to: start + lineLength, class: lineClass, line: true });
			for (const range of highlight.inline) {
				if (range.end > range.start) {
					output.push({
						from: start + range.start,
						to: start + range.end,
						class: markClass
					});
				}
			}
		}
		return output;
	}

	function clearDiff(): void {
		leftEditor?.clearHighlights();
		rightEditor?.clearHighlights();
		summary = '';
	}

	function onLeftChange(next: string): void {
		leftText = next;
		clearDiff();
	}

	function onRightChange(next: string): void {
		rightText = next;
		clearDiff();
	}

	function compare(): void {
		const result = diffTexts(leftText, rightText);
		leftEditor?.setHighlights(buildHighlights(leftText, result.left, 'left'));
		rightEditor?.setHighlights(buildHighlights(rightText, result.right, 'right'));
		summary = result.identical
			? $t('compare.identical')
			: $t('compare.summary', {
					added: result.stats.added,
					removed: result.stats.removed,
					changed: result.stats.changed
				});
	}

	function swap(): void {
		const previousLeft = leftText;
		leftText = rightText;
		rightText = previousLeft;
		clearDiff();
	}

	onMount(() => {
		let cancelled = false;
		(async () => {
			try {
				const [left, right] = await Promise.all([
					loadEditorState('compare-left'),
					loadEditorState('compare-right')
				]);
				if (!cancelled) {
					if (left) leftText = left.content;
					if (right) rightText = right.content;
				}
			} catch {
				/* ignore */
			} finally {
				if (!cancelled) hydrated = true;
			}
		})();
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		const left = leftText;
		const right = rightText;
		if (!hydrated) return;
		const timer = setTimeout(() => {
			void saveEditorState({
				tool_key: 'compare-left',
				content: left,
				indent: 4,
				opt_format: true,
				opt_unescape: true
			});
			void saveEditorState({
				tool_key: 'compare-right',
				content: right,
				indent: 4,
				opt_format: true,
				opt_unescape: true
			});
		}, 500);
		return () => clearTimeout(timer);
	});
</script>

<div
	class="grid h-full min-h-0 gap-3 px-4 py-4"
	style="grid-template-columns: minmax(0, 1fr) 160px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr);"
>
	<section
		class="min-h-0 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
		style="grid-column: 1; grid-row: 1;"
	>
		<CodeMirrorEditor
			bind:this={leftEditor}
			value={leftText}
			language={leftLanguage}
			dark={dark}
			placeholder={$t('compare.placeholderLeft')}
			ariaLabel={$t('compare.source')}
			onchange={onLeftChange}
		/>
	</section>

	<div
		class="flex flex-col items-center justify-center gap-4"
		style="grid-column: 2; grid-row: 1;"
	>
		<button
			type="button"
			onclick={compare}
			class="h-12 w-full rounded-full bg-[var(--accent)] text-[15px] font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
		>
			{$t('compare.compare')}
		</button>

		<button
			type="button"
			onclick={swap}
			class="flex h-12 w-12 items-center justify-center rounded-full bg-[var(--accent)] text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
			title={$t('compare.swap')}
			aria-label={$t('compare.swap')}
		>
			<Icon name="swap" size={20} strokeWidth={2.2} />
		</button>

		{#if summary}
			<p
				class="fade-in max-w-[150px] text-center text-xs font-semibold leading-relaxed text-[var(--text-muted)]"
			>
				{summary}
			</p>
		{/if}
	</div>

	<section
		class="min-h-0 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] shadow-[var(--shadow)]"
		style="grid-column: 3; grid-row: 1;"
	>
		<CodeMirrorEditor
			bind:this={rightEditor}
			value={rightText}
			language={rightLanguage}
			dark={dark}
			placeholder={$t('compare.placeholderRight')}
			ariaLabel={$t('compare.result')}
			onchange={onRightChange}
		/>
	</section>
</div>
