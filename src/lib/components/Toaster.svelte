<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { t } from '$lib/stores/i18n';
	import { toasts, dismissToast } from '$lib/stores/toast';

	const styles: Record<string, string> = {
		success: 'border-emerald-400/50 bg-emerald-600 text-white',
		error: 'border-red-500/60 bg-red-800 text-white',
		info: 'border-white/20 bg-[var(--toast-bg)] text-[var(--toast-text)]'
	};
</script>

<div
	class="pointer-events-none fixed bottom-5 left-1/2 z-[100] flex w-full max-w-md -translate-x-1/2 flex-col items-center gap-2 px-4"
	aria-live="polite"
	aria-atomic="false"
>
	{#each $toasts as toast (toast.id)}
		<div
			class="fade-in pointer-events-auto relative flex w-full items-center rounded-xl border px-4 py-3 text-sm font-semibold shadow-xl {styles[
				toast.kind
			]}"
		>
			<span class="min-w-0 flex-1 break-words px-7 text-center">{toast.message}</span>
			<button
				type="button"
				onclick={() => dismissToast(toast.id)}
				class="absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-md opacity-70 transition-all duration-150 hover:scale-110 hover:opacity-100 active:scale-95"
				title={$t('common.close')}
				aria-label={$t('common.close')}
			>
				<Icon name="close" size={15} strokeWidth={2.6} />
			</button>
		</div>
	{/each}
</div>
