<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { restartApplication } from '$lib/services/updater';
	import { t } from '$lib/stores/i18n';
	import { clearUpdate, dismissUpdate, installUpdate, updateState } from '$lib/stores/update';
	import { isWindowsUserAgent } from '$lib/utils/update';

	const state = $derived($updateState);
	const version = $derived(state.version ?? '');
	const failed = $derived(state.phase === 'error');
	const available = $derived(state.phase === 'available');
	const installed = $derived(state.phase === 'ready');
	// Only Windows hands the install to an external installer that exits the app and relaunches it.
	const windowsInstaller = $derived(
		typeof navigator !== 'undefined' && isWindowsUserAgent(navigator.userAgent)
	);

	const label = $derived(
		failed
			? (state.message ?? $t('common.error'))
			: state.phase === 'downloading'
				? $t('update.downloading', { version, percent: state.percent })
				: state.phase === 'installing'
					? $t('update.installing', { version })
					: installed
						? $t('update.ready', { version })
						: $t('update.available', { version })
	);

	// "checking" is carried by the navbar spinner, so the banner appears once there is something to act on.
	const visible = $derived(state.phase !== 'idle' && state.phase !== 'checking');
</script>

{#if visible}
	<div
		class="fade-in shrink-0 border-b border-[var(--border)] px-5 py-2.5 {failed
			? 'bg-[var(--danger-bg)]'
			: 'bg-[var(--panel-bg)]'}"
		role={failed ? 'alert' : 'status'}
		aria-live={failed ? 'assertive' : 'polite'}
	>
		<div class="mx-auto flex w-full max-w-5xl flex-wrap items-center gap-3">
			<span
				class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg {failed
					? 'bg-[var(--danger-text)]/10 text-[var(--danger-text)]'
					: installed
						? 'bg-[var(--accent)] text-[var(--accent-text)]'
						: 'bg-[var(--panel-bg-alt)] text-[var(--text-muted)]'}"
			>
				{#if failed}
					<Icon name="close" size={16} strokeWidth={2.4} />
				{:else if installed}
					<Icon name="check" size={16} strokeWidth={2.4} />
				{:else if available}
					<Icon name="download" size={16} />
				{:else}
					<Icon name="refresh" size={16} class="animate-spin" />
				{/if}
			</span>

			<span
				class="min-w-0 flex-1 text-sm font-semibold {failed ? 'text-[var(--danger-text)]' : 'text-[var(--text)]'}"
			>
				{label}
			</span>

			{#if available && state.notes}
				<span class="w-full pl-11 text-xs leading-relaxed text-[var(--text-muted)] line-clamp-3">
					{state.notes}
				</span>
			{/if}

			{#if windowsInstaller && state.phase === 'installing'}
				<span class="w-full pl-11 text-xs text-[var(--text-muted)]">{$t('update.windowsHint')}</span>
			{/if}

			{#if installed && !windowsInstaller}
				<span class="w-full pl-11 text-xs text-[var(--text-muted)]">{$t('update.readyHint')}</span>
			{/if}

			{#if state.phase === 'downloading'}
				<div
					class="h-1 w-full overflow-hidden rounded-full bg-[var(--panel-bg-alt)]"
					role="progressbar"
					aria-valuenow={state.percent}
					aria-valuemin={0}
					aria-valuemax={100}
				>
					<div class="h-full rounded-full bg-[var(--accent)]" style="width: {state.percent}%"></div>
				</div>
			{/if}

			{#if available}
				<button
					type="button"
					onclick={() => void installUpdate()}
					class="h-8 rounded-lg bg-[var(--accent)] px-4 text-sm font-semibold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
				>
					{$t('update.install')}
				</button>
				<button
					type="button"
					onclick={() => void dismissUpdate()}
					class="h-8 rounded-lg px-3 text-sm font-semibold text-[var(--text-muted)] transition-all duration-150 hover:bg-[var(--row-hover)] hover:scale-105 hover:text-[var(--text)] active:scale-95"
				>
					{$t('update.dismiss')}
				</button>
			{:else if installed}
				<button
					type="button"
					onclick={() => void restartApplication()}
					class="h-8 rounded-lg bg-[var(--accent)] px-4 text-sm font-semibold text-[var(--accent-text)] transition-all duration-150 hover:scale-105 hover:bg-[var(--accent-hover)] active:scale-95"
				>
					{$t('update.restart')}
				</button>
				<button
					type="button"
					onclick={() => void clearUpdate()}
					class="h-8 rounded-lg px-3 text-sm font-semibold text-[var(--text-muted)] transition-all duration-150 hover:bg-[var(--row-hover)] hover:scale-105 hover:text-[var(--text)] active:scale-95"
				>
					{$t('update.later')}
				</button>
			{:else if failed}
				<button
					type="button"
					onclick={() => void clearUpdate()}
					class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[var(--danger-text)] transition-all duration-150 hover:scale-110 active:scale-95"
					title={$t('common.close')}
					aria-label={$t('common.close')}
				>
					<Icon name="close" size={15} strokeWidth={2.6} />
				</button>
			{/if}
		</div>
	</div>
{/if}
