<script lang="ts">
	import { onMount } from 'svelte';
	import { basicSetup } from 'codemirror';
	import {
		Compartment,
		EditorState,
		StateEffect,
		StateField,
		type Extension
	} from '@codemirror/state';
	import {
		Decoration,
		EditorView,
		placeholder as cmPlaceholder,
		type DecorationSet
	} from '@codemirror/view';
	import { json as jsonLang } from '@codemirror/lang-json';
	import { xml as xmlLang } from '@codemirror/lang-xml';
	import {
		defaultHighlightStyle,
		syntaxHighlighting,
		type HighlightStyle
	} from '@codemirror/language';
	import { oneDarkHighlightStyle } from '@codemirror/theme-one-dark';

	export type EditorLanguage = 'xml' | 'json' | 'text';

	export interface Highlight {
		from: number;
		to: number;
		class: string;
		line?: boolean;
	}

	let {
		value = '',
		language = 'text',
		readonly = false,
		dark = true,
		placeholder = '',
		ariaLabel = '',
		onchange,
		onfocus
	}: {
		value?: string;
		language?: EditorLanguage;
		readonly?: boolean;
		dark?: boolean;
		placeholder?: string;
		ariaLabel?: string;
		onchange?: (value: string) => void;
		onfocus?: () => void;
	} = $props();

	const setHighlightsEffect = StateEffect.define<Highlight[]>();

	const highlightField = StateField.define<DecorationSet>({
		create: () => Decoration.none,
		update(decorations, transaction) {
			decorations = decorations.map(transaction.changes);
			for (const effect of transaction.effects) {
				if (effect.is(setHighlightsEffect)) {
					const ranges = [];
					for (const highlight of effect.value) {
						const length = transaction.state.doc.length;
						const from = Math.max(0, Math.min(highlight.from, length));
						const to = Math.max(from, Math.min(highlight.to, length));
						if (highlight.line) {
							const line = transaction.state.doc.lineAt(from);
							ranges.push(Decoration.line({ class: highlight.class }).range(line.from));
						} else if (to > from) {
							ranges.push(Decoration.mark({ class: highlight.class }).range(from, to));
						}
					}
					decorations = Decoration.set(ranges, true);
				}
			}
			return decorations;
		},
		provide: (field) => EditorView.decorations.from(field)
	});

	function languageExtension(lang: EditorLanguage): Extension {
		if (lang === 'json') return jsonLang();
		if (lang === 'xml') return xmlLang();
		return [];
	}

	function editorTheme(darkMode: boolean): Extension {
		return EditorView.theme(
			{
				'&': {
					height: '100%',
					fontSize: '14px',
					backgroundColor: darkMode ? '#1b1b1b' : '#ffffff',
					color: darkMode ? '#e5e5e5' : '#111827'
				},
				'.cm-scroller': {
					fontFamily: '"Cascadia Code", "Fira Code", Consolas, "Courier New", monospace',
					lineHeight: '1.65'
				},
				'.cm-content': {
					caretColor: darkMode ? '#f0a500' : '#d97706',
					padding: '8px 0'
				},
				'.cm-gutters': {
					backgroundColor: darkMode ? '#1b1b1b' : '#ffffff',
					color: darkMode ? '#6b7280' : '#9ca3af',
					border: 'none'
				},
				'.cm-activeLine': {
					backgroundColor: darkMode ? 'rgba(255,255,255,0.035)' : 'rgba(0,0,0,0.035)'
				},
				'.cm-activeLineGutter': {
					backgroundColor: darkMode ? 'rgba(255,255,255,0.045)' : 'rgba(0,0,0,0.045)',
					color: darkMode ? '#d1d5db' : '#374151'
				},
				'&.cm-focused': { outline: 'none' },
				'.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': {
					backgroundColor: darkMode ? 'rgba(229,160,13,0.30)' : 'rgba(217,119,6,0.20)'
				},
				'.cm-cursor, .cm-dropCursor': {
					borderLeftColor: darkMode ? '#f0a500' : '#d97706'
				},
				'.cm-placeholder': { color: darkMode ? '#6b7280' : '#9ca3af' },
				'.cm-foldGutter span': { color: darkMode ? '#6b7280' : '#9ca3af' }
			},
			{ dark: darkMode }
		);
	}

	function themeExtension(darkMode: boolean): Extension {
		const style: HighlightStyle = darkMode ? oneDarkHighlightStyle : defaultHighlightStyle;
		return [editorTheme(darkMode), syntaxHighlighting(style)];
	}

	const languageCompartment = new Compartment();
	const readonlyCompartment = new Compartment();
	const placeholderCompartment = new Compartment();
	const themeCompartment = new Compartment();

	let container: HTMLDivElement;
	let view = $state.raw<EditorView | null>(null);

	onMount(() => {
		const state = EditorState.create({
			doc: value,
			extensions: [
				basicSetup,
				EditorState.tabSize.of(4),
				EditorView.contentAttributes.of({
					'aria-label': ariaLabel,
					'aria-multiline': 'true'
				}),
				highlightField,
				languageCompartment.of(languageExtension(language)),
				readonlyCompartment.of([
					EditorState.readOnly.of(readonly),
					EditorView.editable.of(!readonly)
				]),
				placeholderCompartment.of(cmPlaceholder(placeholder)),
				themeCompartment.of(themeExtension(dark)),
				EditorView.updateListener.of((update) => {
					if (update.docChanged) onchange?.(update.state.doc.toString());
				}),
				EditorView.domEventHandlers({
					focus: () => {
						onfocus?.();
						return false;
					}
				})
			]
		});

		view = new EditorView({ state, parent: container });
		return () => {
			view?.destroy();
			view = null;
		};
	});

	$effect(() => {
		const nextValue = value;
		if (!view) return;
		if (nextValue !== view.state.doc.toString()) {
			view.dispatch({
				changes: { from: 0, to: view.state.doc.length, insert: nextValue }
			});
		}
	});

	$effect(() => {
		const lang = language;
		const isReadonly = readonly;
		const isDark = dark;
		const hint = placeholder;
		if (!view) return;
		view.dispatch({
			effects: [
				languageCompartment.reconfigure(languageExtension(lang)),
				readonlyCompartment.reconfigure([
					EditorState.readOnly.of(isReadonly),
					EditorView.editable.of(!isReadonly)
				]),
				placeholderCompartment.reconfigure(cmPlaceholder(hint)),
				themeCompartment.reconfigure(themeExtension(isDark))
			]
		});
	});

	export function setHighlights(highlights: Highlight[]): void {
		view?.dispatch({ effects: setHighlightsEffect.of(highlights) });
	}

	export function clearHighlights(): void {
		view?.dispatch({ effects: setHighlightsEffect.of([]) });
	}

	export function focusEditor(): void {
		view?.focus();
	}

	export function getEditorValue(): string {
		return view?.state.doc.toString() ?? value;
	}
</script>

<div class="h-full w-full overflow-hidden" bind:this={container}></div>
