<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { pickPartFile } from '$lib/services/files';
	import type { RestFileRow } from '$lib/utils/rest';

	let {
		rows = [],
		onchange,
		nameLabel = 'Part name',
		fileLabel = 'File',
		namePlaceholder = '',
		filePlaceholder = 'No file',
		addLabel = 'Add file part',
		removeLabel = 'Remove file part',
		pickLabel = 'Select file',
		pickTitle = 'Select file to send',
		fieldHint
	}: {
		rows?: RestFileRow[];
		onchange: (rows: RestFileRow[]) => void;
		nameLabel?: string;
		fileLabel?: string;
		namePlaceholder?: string;
		filePlaceholder?: string;
		addLabel?: string;
		removeLabel?: string;
		pickLabel?: string;
		pickTitle?: string;
		/**
		 * Localized reason a cell is invalid, or `null` when it can be sent.
		 * `index` is the row index, or `-1` for the add-row draft.
		 */
		fieldHint?: (field: 'name' | 'value', row: RestFileRow, index: number) => string | null;
	} = $props();

	let draftName = $state('');
	let draftPath = $state('');
	let draftFilename = $state('');
	let draftFile = $state<File | null>(null);
	let draftNameInput = $state<HTMLInputElement | null>(null);
	/** Set when a draft was committed without a file, so the picker explains itself. */
	let draftNeedsFile = $state(false);
	let picking = $state<number | null>(null);

	function draftRow(): RestFileRow {
		return { name: draftName, path: draftPath, filename: draftFilename, file: draftFile };
	}

	// Same rules as the property table: the draft is checked before it can become a row,
	// and the name cell is checked even while empty because a file without a part name
	// would never reach the request.
	const draftNameHint = $derived(
		draftName || draftFilename ? (fieldHint?.('name', draftRow(), -1) ?? null) : null
	);
	const draftFileHint = $derived(
		draftFilename || draftNeedsFile ? (fieldHint?.('value', draftRow(), -1) ?? null) : null
	);

	function hint(field: 'name' | 'value', row: RestFileRow, index: number): string | null {
		return fieldHint?.(field, row, index) ?? null;
	}

	function nameClasses(problem: string | null): string {
		const palette = problem
			? 'bg-[var(--danger-bg)] text-[var(--danger-text)]'
			: 'bg-transparent text-[var(--text)] focus-visible:bg-[var(--panel-bg-alt)]';
		return `h-10 min-w-0 px-3 text-sm outline-none placeholder:text-[var(--text-muted)] border-r border-[var(--border)] font-semibold ${palette}`;
	}

	function fileClasses(problem: string | null, draft: boolean): string {
		const palette = problem
			? 'bg-[var(--danger-bg)] text-[var(--danger-text)]'
			: 'bg-transparent text-[var(--text)] hover:bg-[var(--panel-bg-alt)]';
		return `h-10 min-w-0 ${draft ? 'px-3' : 'pl-3 pr-10'} text-left text-sm outline-none ${palette}`;
	}

	function fileText(row: RestFileRow): string {
		return row.filename || row.path || filePlaceholder;
	}

	function fileTitle(row: RestFileRow, problem: string | null): string | undefined {
		if (problem) return problem;
		return row.path || row.filename || pickLabel;
	}

	function updateName(index: number, value: string): void {
		onchange(rows.map((row, rowIndex) => (rowIndex === index ? { ...row, name: value } : row)));
	}

	function remove(index: number): void {
		onchange(rows.filter((_, rowIndex) => rowIndex !== index));
	}

	async function choose(index: number): Promise<void> {
		picking = index;
		try {
			const picked = await pickPartFile(pickTitle);
			if (!picked) return;
			if (index < 0) {
				draftPath = picked.path;
				draftFilename = picked.filename;
				draftFile = picked.file ?? null;
				draftNeedsFile = false;
				return;
			}
			onchange(
				rows.map((row, rowIndex) =>
					rowIndex === index
						? { ...row, path: picked.path, filename: picked.filename, file: picked.file ?? null }
						: row
				)
			);
		} finally {
			picking = null;
		}
	}

	function commitDraft(): void {
		const name = draftName.trim();
		if (!name) {
			// A picked file with no part name stays in the draft: dropping it would throw
			// away the pick, and the name cell already explains what is missing.
			if (draftFilename) return;
			draftName = '';
			draftPath = '';
			draftFilename = '';
			draftFile = null;
			draftNeedsFile = false;
			return;
		}
		if (draftNameHint) return;
		// A part with no file has nothing to send: the picker takes over the explanation.
		if (!draftFilename && !draftFile) {
			draftNeedsFile = true;
			return;
		}
		if (draftFileHint) return;
		onchange([
			...rows,
			{
				name,
				path: draftPath,
				filename: draftFilename,
				file: draftFile
			}
		]);
		draftName = '';
		draftPath = '';
		draftFilename = '';
		draftFile = null;
		draftNeedsFile = false;
	}

	function onDraftKeydown(event: KeyboardEvent): void {
		if (event.key === 'Enter') {
			event.preventDefault();
			commitDraft();
			draftNameInput?.focus();
		}
	}
</script>

<div class="flex min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--border)]">
	<div
		class="min-h-0 flex-1 overflow-y-auto"
		onfocusout={(event) => {
			const next = event.relatedTarget as Node | null;
			if (!(event.currentTarget as HTMLElement).contains(next)) commitDraft();
		}}
	>
		<div class="grid grid-cols-2 border-b border-[var(--border)]">
			<input
				bind:this={draftNameInput}
				bind:value={draftName}
				onkeydown={onDraftKeydown}
				placeholder={namePlaceholder}
				class={nameClasses(draftNameHint)}
				aria-label={`${nameLabel} (${addLabel})`}
				aria-invalid={Boolean(draftNameHint)}
				title={draftNameHint ?? undefined}
			/>
			<button
				type="button"
				onclick={() => choose(-1)}
				class={fileClasses(draftFileHint, true)}
				aria-label={`${fileLabel} (${addLabel})`}
				title={fileTitle(
					{ name: draftName, path: draftPath, filename: draftFilename, file: draftFile },
					draftFileHint
				)}
			>
				<span class="flex items-center gap-2 truncate">
					<Icon name="file" size={15} />
					<span class="truncate">{draftFilename || filePlaceholder}</span>
					{#if picking === -1}
						<span class="text-[var(--text-muted)]">…</span>
					{/if}
				</span>
			</button>
		</div>

		{#each rows as row, index (index)}
			{@const nameHint = hint('name', row, index)}
			{@const fileHint = hint('value', row, index)}
			<div class="group relative grid grid-cols-2 border-b border-[var(--border)]">
				<input
					value={row.name}
					oninput={(event) => updateName(index, (event.currentTarget as HTMLInputElement).value)}
					placeholder={namePlaceholder}
					class={nameClasses(nameHint)}
					aria-label={`${nameLabel} ${index + 1}`}
					aria-invalid={Boolean(nameHint)}
					title={nameHint ?? undefined}
				/>
				<button
					type="button"
					onclick={() => choose(index)}
					class={fileClasses(fileHint, false)}
					aria-label={`${fileLabel} ${index + 1}`}
					title={fileTitle(row, fileHint)}
				>
					<span class="flex items-center gap-2 truncate">
						<Icon name="file" size={15} />
						<span class="truncate">{fileText(row)}</span>
						{#if picking === index}
							<span class="text-[var(--text-muted)]">…</span>
						{/if}
					</span>
				</button>
				<button
					type="button"
					onclick={() => remove(index)}
					class="absolute right-1.5 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md bg-[var(--panel-bg)] text-[var(--text-muted)] opacity-0 transition-all duration-150 hover:scale-110 hover:text-[var(--danger)] focus-visible:opacity-100 group-hover:opacity-100"
					title={removeLabel}
					aria-label={removeLabel}
				>
					<Icon name="trash" size={15} />
				</button>
			</div>
		{/each}
	</div>
</div>
