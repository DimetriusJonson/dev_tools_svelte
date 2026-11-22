<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import type { RestProperty } from '$lib/utils/rest';

	let {
		rows = [],
		onchange,
		nameLabel = 'Name',
		valueLabel = 'Value',
		namePlaceholder = '',
		valuePlaceholder = '',
		addLabel = 'Add property',
		removeLabel = 'Remove property',
		headerAutocomplete = false,
		lockedTitle = '',
		fieldHint,
		locked
	}: {
		rows?: RestProperty[];
		onchange: (rows: RestProperty[]) => void;
		nameLabel?: string;
		valueLabel?: string;
		namePlaceholder?: string;
		valuePlaceholder?: string;
		addLabel?: string;
		removeLabel?: string;
		headerAutocomplete?: boolean;
		/** Why a read-only row is on screen: the tool writes it, not the user. */
		lockedTitle?: string;
		/**
		 * Localized reason a cell is invalid, or `null` when it follows the standard.
		 * `index` is the row index, or `-1` for the add-row draft.
		 */
		fieldHint?: (field: 'name' | 'value', row: RestProperty, index: number) => string | null;
		/** A row the component shows without letting the user edit or remove it. */
		locked?: (row: RestProperty, index: number) => boolean;
	} = $props();

	const listId = `property-${Math.random().toString(36).slice(2)}`;

	const HEADER_NAMES = [
		'Accept',
		'Accept-Encoding',
		'Accept-Language',
		'Authorization',
		'Cache-Control',
		'Content-Type',
		'Cookie',
		'Origin',
		'Pragma',
		'Referer',
		'User-Agent',
		'X-Requested-With'
	];

	const HEADER_VALUES = [
		'application/json',
		'application/xml',
		'application/x-www-form-urlencoded',
		'text/plain',
		'text/html',
		'*/*',
		'gzip, deflate',
		'no-cache',
		'ru',
		'en'
	];

	let draftName = $state('');
	let draftValue = $state('');
	let draftNameInput = $state<HTMLInputElement | null>(null);

	const nameOptions = $derived(
		Array.from(new Set([...HEADER_NAMES, ...rows.map((row) => row.name)].filter(Boolean)))
	);
	const valueOptions = $derived(
		Array.from(new Set([...HEADER_VALUES, ...rows.map((row) => row.value)].filter(Boolean)))
	);

	// The draft is checked before it can be stored, so an invalid name or value
	// never becomes a row and never reaches the request. The name cell is checked
	// even while it is empty: a value without a name would be an unusable row.
	const draftNameHint = $derived(
		draftName || draftValue
			? (fieldHint?.('name', { name: draftName, value: draftValue }, -1) ?? null)
			: null
	);
	const draftValueHint = $derived(
		draftValue ? (fieldHint?.('value', { name: draftName, value: draftValue }, -1) ?? null) : null
	);

	function hint(field: 'name' | 'value', row: RestProperty, index: number): string | null {
		return fieldHint?.(field, row, index) ?? null;
	}

	function cellClass(
		field: 'name' | 'value',
		problem: string | null,
		draft = false,
		lockedRow = false
	): string {
		// A locked row has no remove button, so its value cell keeps the draft padding.
		const padding = field === 'name' || draft || lockedRow ? 'px-3' : 'pl-3 pr-10';
		const side = field === 'name' ? 'border-r border-[var(--border)] font-semibold' : '';
		const palette = problem
			? 'bg-[var(--danger-bg)] text-[var(--danger-text)]'
			: lockedRow
				? 'bg-[var(--panel-bg-alt)] text-[var(--text-muted)]'
				: 'bg-transparent text-[var(--text)] focus-visible:bg-[var(--panel-bg-alt)]';
		return `h-10 min-w-0 ${padding} text-sm outline-none placeholder:text-[var(--text-muted)] ${side} ${palette}`;
	}

	function update(index: number, field: 'name' | 'value', value: string): void {
		onchange(rows.map((row, rowIndex) => (rowIndex === index ? { ...row, [field]: value } : row)));
	}

	function remove(index: number): void {
		onchange(rows.filter((_, rowIndex) => rowIndex !== index));
	}

	function commitDraft(): void {
		const name = draftName.trim();
		if (!name) {
			// A value without a name stays in the draft: dropping it would throw away
			// what was typed, and the name cell already explains what is missing.
			if (draftValue.trim()) return;
			draftName = '';
			draftValue = '';
			return;
		}
		// A broken draft stays on screen, highlighted, so it can be corrected.
		if (draftNameHint || draftValueHint) return;
		onchange([...rows, { name, value: draftValue }]);
		draftName = '';
		draftValue = '';
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
				list={headerAutocomplete ? `${listId}-names` : undefined}
				class={cellClass('name', draftNameHint, true)}
				aria-label={`${nameLabel} (${addLabel})`}
				aria-invalid={Boolean(draftNameHint)}
				title={draftNameHint ?? undefined}
			/>
			<input
				bind:value={draftValue}
				onkeydown={onDraftKeydown}
				placeholder={valuePlaceholder}
				list={headerAutocomplete ? `${listId}-values` : undefined}
				class={cellClass('value', draftValueHint, true)}
				aria-label={`${valueLabel} (${addLabel})`}
				aria-invalid={Boolean(draftValueHint)}
				title={draftValueHint ?? undefined}
			/>
		</div>

		{#each rows as row, index (index)}
			{@const nameHint = hint('name', row, index)}
			{@const valueHint = hint('value', row, index)}
			{@const lockedRow = locked?.(row, index) ?? false}
			<div class="grid grid-cols-2 border-b border-[var(--border)] {lockedRow ? '' : 'group relative'}">
				<input
					value={row.name}
					oninput={(event) => update(index, 'name', (event.currentTarget as HTMLInputElement).value)}
					placeholder={namePlaceholder}
					list={headerAutocomplete ? `${listId}-names` : undefined}
					readonly={lockedRow}
					tabindex={lockedRow ? -1 : undefined}
					class={cellClass('name', nameHint, false, lockedRow)}
					aria-label={`${nameLabel} ${index + 1}`}
					aria-invalid={Boolean(nameHint)}
					title={lockedRow ? lockedTitle || undefined : nameHint ?? undefined}
				/>
				<input
					value={row.value}
					oninput={(event) => update(index, 'value', (event.currentTarget as HTMLInputElement).value)}
					placeholder={valuePlaceholder}
					list={headerAutocomplete ? `${listId}-values` : undefined}
					readonly={lockedRow}
					tabindex={lockedRow ? -1 : undefined}
					class={cellClass('value', valueHint, false, lockedRow)}
					aria-label={`${valueLabel} ${index + 1}`}
					aria-invalid={Boolean(valueHint)}
					title={lockedRow ? lockedTitle || undefined : valueHint ?? undefined}
				/>
				{#if !lockedRow}
					<button
						type="button"
						onclick={() => remove(index)}
						class="absolute right-1.5 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md bg-[var(--panel-bg)] text-[var(--text-muted)] opacity-0 transition-all duration-150 hover:scale-110 hover:text-[var(--danger)] focus-visible:opacity-100 group-hover:opacity-100"
						title={removeLabel}
						aria-label={removeLabel}
					>
						<Icon name="trash" size={15} />
					</button>
				{/if}
			</div>
		{/each}
	</div>

	{#if headerAutocomplete}
		<datalist id={`${listId}-names`}>
			{#each nameOptions as name (name)}
				<option value={name}></option>
			{/each}
		</datalist>
		<datalist id={`${listId}-values`}>
			{#each valueOptions as value (value)}
				<option value={value}></option>
			{/each}
		</datalist>
	{/if}
</div>
