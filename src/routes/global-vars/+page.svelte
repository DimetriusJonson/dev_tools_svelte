<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { t } from '$lib/stores/i18n';
	import { notify, notifyError } from '$lib/stores/toast';
	import { copyText } from '$lib/utils/clipboard';
	import {
		createGlobalVar,
		deleteGlobalVar,
		listGlobalVars,
		revealGlobalVar,
		updateGlobalVar,
		isVarsSupported,
		type GlobalVar
	} from '$lib/services/vars';
	import {
		MAX_VAR_NAME_CHARS,
		REVEAL_MS,
		SECRET_DOTS,
		describeVarError,
		templateFor,
		validateVarName
	} from '$lib/utils/vars';

	// The same debounce the other editors use, so a fast rename is not one write per key.
	const NAME_DEBOUNCE_MS = 450;

	let desktop = $state(false);
	let loading = $state(true);
	let vars = $state<GlobalVar[]>([]);
	let draft = $state({ name: '', value: '', secure: false });
	let draftError = $state<string | null>(null);
	/** Localized reason for the last invalid name, keyed by row id. */
	let nameErrors = $state<Record<string, string>>({});
	/** A revealed secret lives only in page memory, never in storage. */
	let revealed = $state<Record<string, string>>({});
	let confirmingId = $state('');
	let confirmingSecureId = $state('');

	const revealTimers = new Map<string, ReturnType<typeof setTimeout>>();
	const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

	// `$t()` inside a template literal in an attribute renders the code verbatim.
	function copyLabel(name: string): string {
		return $t('vars.copyTemplate', { name });
	}

	function nameMessage(name: string, issue: string): string {
		if (issue === 'required') return $t('vars.nameRequired');
		if (issue === 'invalid') return $t('vars.nameInvalid', { name });
		return $t('vars.nameDuplicate', { name });
	}

	function clearReveal(id: string): void {
		const timer = revealTimers.get(id);
		if (timer) clearTimeout(timer);
		revealTimers.delete(id);
		delete revealed[id];
	}

	/**
	 * A revealed secret stays editable for `REVEAL_MS` and then hides again, so nothing
	 * that belongs to the credential store stays on screen longer than asked for.
	 */
	function scheduleReveal(id: string, value: string): void {
		const timer = revealTimers.get(id);
		if (timer) clearTimeout(timer);
		revealed[id] = value;
		revealTimers.set(
			id,
			setTimeout(() => {
				revealTimers.delete(id);
				delete revealed[id];
			}, REVEAL_MS)
		);
	}

	function queueSave(row: GlobalVar): void {
		const existing = saveTimers.get(row.id);
		if (existing) clearTimeout(existing);
		saveTimers.set(
			row.id,
			setTimeout(() => {
				saveTimers.delete(row.id);
				void commitRow(row);
			}, NAME_DEBOUNCE_MS)
		);
	}

	function replaceRow(updated: GlobalVar): void {
		const index = vars.findIndex((item) => item.id === updated.id);
		if (index >= 0) vars[index] = updated;
	}

	async function commitRow(row: GlobalVar): Promise<void> {
		const issue = validateVarName(row.name, vars, row.id);
		if (issue) {
			nameErrors[row.id] = nameMessage(row.name.trim(), issue);
			return;
		}
		delete nameErrors[row.id];
		try {
			// A safe row only accepts a new secret while it is revealed; otherwise the
			// credential-store entry stays exactly as it is.
			const value = row.secure ? (revealed[row.id] ?? null) : (row.value ?? '');
			const updated = await updateGlobalVar({
				id: row.id,
				name: row.name.trim(),
				secure: row.secure,
				value
			});
			replaceRow(updated);
			// The reveal timer keeps the secret on screen for the time it was asked for,
			// so a save does not hide the field the user is still typing in.
		} catch (error) {
			notifyError(describeVarError(error, $t) || $t('vars.saveFailed'));
		}
	}

	async function addVariable(): Promise<void> {
		const issue = validateVarName(draft.name, vars);
		if (issue) {
			draftError = nameMessage(draft.name.trim(), issue);
			return;
		}
		draftError = null;
		try {
			const created = await createGlobalVar({
				name: draft.name.trim(),
				secure: draft.secure,
				value: draft.value
			});
			vars = [...vars, created];
			draft = { name: '', value: '', secure: draft.secure };
			notify($t('vars.created'), 'success');
		} catch (error) {
			notifyError(describeVarError(error, $t) || $t('vars.saveFailed'));
		}
	}

	/**
	 * Turning a safe variable into a plain one reads the secret out of the credential
	 * store, so the first click only opens the confirmation. A plain variable becomes
	 * safe at once: nothing has to be read back.
	 */
	async function toggleSecure(row: GlobalVar): Promise<void> {
		if (row.secure) {
			if (confirmingSecureId !== row.id) confirmingSecureId = row.id;
			return;
		}

		try {
			const updated = await updateGlobalVar({
				id: row.id,
				name: row.name,
				secure: true,
				value: null
			});
			replaceRow(updated);
			notify($t('vars.updated'), 'success');
		} catch (error) {
			notifyError(describeVarError(error, $t) || $t('vars.saveFailed'));
		}
	}

	async function confirmSecureToPlain(row: GlobalVar): Promise<void> {
		confirmingSecureId = '';
		try {
			const updated = await updateGlobalVar({
				id: row.id,
				name: row.name,
				secure: false,
				value: null
			});
			replaceRow(updated);
			clearReveal(row.id);
			notify($t('vars.updated'), 'success');
		} catch (error) {
			notifyError(describeVarError(error, $t) || $t('vars.saveFailed'));
		}
	}

	async function showValue(row: GlobalVar): Promise<void> {
		if (revealed[row.id] !== undefined) {
			clearReveal(row.id);
			return;
		}
		try {
			scheduleReveal(row.id, await revealGlobalVar(row.id));
		} catch (error) {
			notifyError(describeVarError(error, $t));
		}
	}

	/** The first click asks, the second one deletes: no `window.confirm` here. */
	async function removeVariable(row: GlobalVar): Promise<void> {
		if (confirmingId !== row.id) {
			confirmingId = row.id;
			return;
		}
		confirmingId = '';
		try {
			await deleteGlobalVar(row.id);
			vars = vars.filter((item) => item.id !== row.id);
			clearReveal(row.id);
			delete nameErrors[row.id];
			notify($t('vars.deleted'), 'success');
		} catch (error) {
			notifyError(describeVarError(error, $t));
		}
	}

	async function copyTemplate(row: GlobalVar): Promise<void> {
		await copyText(templateFor(row.name));
		notify($t('vars.templateCopied'), 'success');
	}

	onMount(() => {
		desktop = isVarsSupported();
		if (!desktop) {
			loading = false;
		} else {
			void (async () => {
				try {
					vars = await listGlobalVars();
				} catch (error) {
					notifyError(describeVarError(error, $t) || $t('vars.loadFailed'));
				} finally {
					loading = false;
				}
			})();
		}

		return () => {
			for (const timer of revealTimers.values()) clearTimeout(timer);
			for (const timer of saveTimers.values()) clearTimeout(timer);
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
					<Icon name="variable" size={28} strokeWidth={2} />
				</div>
				<div>
					<h1 class="text-2xl font-bold text-[var(--text)]">{$t('vars.title')}</h1>
					<p class="mt-1 max-w-2xl text-sm leading-relaxed text-[var(--text-muted)]">
						{$t('vars.description')}
					</p>
				</div>
			</div>
			<span
				class="inline-flex shrink-0 items-center gap-2 rounded-full px-3 py-1.5 text-xs font-bold text-[var(--accent)]"
				style="background-color: color-mix(in srgb, var(--accent) 16%, transparent);"
			>
				<Icon name="lock" size={14} />
				{$t('vars.secure')}
			</span>
		</section>

		{#if !desktop}
			<section class="rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-6">
				<p class="text-sm font-bold text-[var(--text)]">{$t('vars.desktopOnly')}</p>
				<p class="mt-1 text-xs leading-relaxed text-[var(--text-muted)]">
					{$t('vars.desktopOnlyHint')}
				</p>
			</section>
		{:else}
			<section
				class="rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-5 shadow-[var(--shadow)]"
			>
				{#if loading}
					<p class="text-sm text-[var(--text-muted)]">{$t('vars.title')}…</p>
				{:else if vars.length === 0}
					<p class="text-sm text-[var(--text-muted)]">{$t('vars.empty')}</p>
				{:else}
					<ul class="flex flex-col gap-2">
						{#each vars as row (row.id)}
							<li
								class="grid grid-cols-[auto_minmax(0,1fr)_minmax(0,1.4fr)_auto] items-center gap-2 rounded-lg border border-[var(--border)] bg-[var(--app-bg)] p-2"
							>
								<button
									type="button"
									onclick={() => void toggleSecure(row)}
									aria-pressed={row.secure}
									class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md transition-all duration-150 hover:scale-105 active:scale-95 {row.secure
										? 'bg-[var(--accent)] text-[var(--accent-text)]'
										: 'bg-[var(--secondary-bg)] text-[var(--secondary-text)]'}"
									title={row.secure ? $t('vars.secureOn') : $t('vars.secureOff')}
									aria-label={row.secure ? $t('vars.secureOn') : $t('vars.secureOff')}
								>
									<Icon name={row.secure ? 'lock' : 'unlock'} size={18} />
								</button>

								<input
									type="text"
									value={row.name}
									aria-label={$t('vars.name')}
									aria-invalid={Boolean(nameErrors[row.id]) ? true : undefined}
									title={nameErrors[row.id] ?? $t('vars.name')}
									class="h-10 rounded-md border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)] {nameErrors[row.id]
										? 'border-[var(--danger)] bg-[var(--danger-bg)] text-[var(--danger-text)]'
										: ''}"
									oninput={(event) => {
										row.name = (event.currentTarget as HTMLInputElement).value;
										queueSave(row);
									}}
								/>

								{#if row.secure}
									{#if revealed[row.id] === undefined}
										<button
											type="button"
											onclick={() => void showValue(row)}
											class="flex h-10 items-center gap-2 rounded-md bg-[var(--panel-bg-alt)] px-3 text-sm font-semibold text-[var(--text-muted)] transition-all duration-150 hover:scale-105 active:scale-95"
											title={$t('vars.reveal')}
											aria-label={row.name}
										>
											<Icon name="eye" size={18} />
											<span>{SECRET_DOTS}</span>
										</button>
									{:else}
										<div class="flex min-w-0 items-center gap-1.5">
											<input
												type="text"
												value={revealed[row.id]}
												aria-label={$t('vars.value')}
												class="h-10 min-w-0 flex-1 rounded-md border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
												oninput={(event) => {
													// A secret typed here reaches the credential store through the same
													// debounce as a rename; hiding the field saves nothing.
													revealed[row.id] = (event.currentTarget as HTMLInputElement).value;
													queueSave(row);
												}}
											/>
											<button
												type="button"
												onclick={() => clearReveal(row.id)}
												class="h-8 w-8 shrink-0 rounded-md text-[var(--text-muted)] transition-all duration-150 hover:bg-white/5 hover:scale-110 active:scale-95"
												title={$t('vars.hide')}
												aria-label={$t('vars.hide')}
											>
											<Icon name="close" size={16} />
											</button>
										</div>
									{/if}
								{:else}
									<input
										type="text"
										value={row.value ?? ''}
										aria-label={$t('vars.value')}
										class="h-10 rounded-md border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
										oninput={(event) => {
											row.value = (event.currentTarget as HTMLInputElement).value;
											queueSave(row);
										}}
									/>
								{/if}

								<div class="flex shrink-0 items-center gap-1.5">
									<button
										type="button"
										onclick={() => void copyTemplate(row)}
										class="h-8 w-8 rounded-md text-[var(--text-muted)] transition-all duration-150 hover:bg-white/5 hover:scale-110 active:scale-95"
										title={copyLabel(row.name)}
										aria-label={copyLabel(row.name)}
									>
										<Icon name="copy" size={16} />
									</button>
									<button
										type="button"
										onclick={() => void removeVariable(row)}
										class="h-8 w-8 rounded-md text-[var(--text-muted)] transition-all duration-150 hover:bg-white/5 hover:scale-110 active:scale-95"
										title={$t('vars.delete')}
										aria-label={row.name}
									>
										<Icon name="trash" size={16} />
									</button>
								</div>
							</li>

							{#if confirmingId === row.id}
								<li
									class="flex flex-wrap items-center gap-2 rounded-lg border border-[var(--danger)] bg-[var(--danger-bg)] px-3 py-2"
									role="alert"
								>
									<span class="text-sm font-bold text-[var(--danger-text)]">
										{$t('vars.deleteConfirm', { name: row.name })}
									</span>
									<span class="text-xs text-[var(--danger-text)]">
										{$t('vars.deleteHint')}
									</span>
									<button
										type="button"
										onclick={() => void removeVariable(row)}
										class="h-9 rounded-full bg-[var(--danger)] px-4 text-xs font-bold text-white transition-all duration-150 hover:scale-105 active:scale-95"
									>
										{$t('vars.delete')}
									</button>
									<button
										type="button"
										onclick={() => (confirmingId = '')}
										class="h-9 rounded-full bg-[var(--secondary-bg)] px-4 text-xs font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
									>
										{$t('common.cancel')}
									</button>
								</li>
							{/if}

							{#if confirmingSecureId === row.id}
								<li
									class="flex flex-wrap items-center gap-2 rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg-alt)] px-3 py-2"
								>
									<span class="text-sm font-bold text-[var(--text)]">
										{$t('vars.secureToPlainConfirm', { name: row.name })}
									</span>
									<button
										type="button"
										onclick={() => void confirmSecureToPlain(row)}
										class="h-9 rounded-full bg-[var(--accent)] px-4 text-xs font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 active:scale-95"
									>
										{$t('vars.secureToPlain')}
									</button>
									<button
										type="button"
										onclick={() => (confirmingSecureId = '')}
										class="h-9 rounded-full bg-[var(--secondary-bg)] px-4 text-xs font-bold text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
									>
										{$t('common.cancel')}
									</button>
								</li>
							{/if}
						{/each}
					</ul>
				{/if}

				<div class="mt-4 grid grid-cols-[auto_minmax(0,1fr)_minmax(0,1.4fr)_auto] gap-2">
					<button
						type="button"
						onclick={() => (draft = { ...draft, secure: !draft.secure })}
						aria-pressed={draft.secure}
						class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md transition-all duration-150 hover:scale-105 active:scale-95 {draft.secure
							? 'bg-[var(--accent)] text-[var(--accent-text)]'
							: 'bg-[var(--secondary-bg)] text-[var(--secondary-text)]'}"
						title={draft.secure ? $t('vars.secureOn') : $t('vars.secureOff')}
						aria-label={draft.secure ? $t('vars.secureOn') : $t('vars.secureOff')}
					>
						<Icon name={draft.secure ? 'lock' : 'unlock'} size={18} />
					</button>
					<label class="sr-only" for="var-draft-name">{$t('vars.name')}</label>
					<input
						id="var-draft-name"
						type="text"
						bind:value={draft.name}
						placeholder={$t('vars.namePlaceholder')}
						aria-invalid={Boolean(draftError) ? true : undefined}
						title={draftError ?? $t('vars.name')}
						class="h-10 rounded-md border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)] {draftError
							? 'border-[var(--danger)] bg-[var(--danger-bg)] text-[var(--danger-text)]'
							: ''}"
					/>
					<label class="sr-only" for="var-draft-value">{$t('vars.value')}</label>
					<input
						id="var-draft-value"
						type="text"
						bind:value={draft.value}
						placeholder={$t('vars.valuePlaceholder')}
						class="h-10 rounded-md border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
					/>
					<button
						type="button"
						onclick={() => void addVariable()}
						class="flex h-10 shrink-0 items-center gap-2 rounded-full bg-[var(--accent)] px-4 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 active:scale-95"
						title={$t('vars.add')}
						aria-label={$t('vars.add')}
					>
						<Icon name="plus" size={16} />
						<span>{$t('vars.add')}</span>
					</button>
				</div>
				<p class="mt-3 text-xs leading-relaxed text-[var(--text-muted)]">
					{$t('vars.secureHint')} · {MAX_VAR_NAME_CHARS}
				</p>
			</section>
		{/if}
	</div>
</div>
