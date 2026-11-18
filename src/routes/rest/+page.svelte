<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { t } from '$lib/stores/i18n';
</script>

<div class="flex h-full items-center justify-center px-6">
	<div
		class="fade-in w-full max-w-xl rounded-2xl border border-[var(--border)] bg-[var(--panel-bg)] p-10 text-center shadow-[var(--shadow)]"
	>
		<div
			class="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-[var(--accent)] text-[var(--accent-text)]"
		>
			<Icon name="server" size={30} strokeWidth={2} />
		</div>
		<h1 class="mt-6 text-3xl font-bold text-[var(--text)]">{$t('rest.title')}</h1>
		<p class="mx-auto mt-3 max-w-md text-[15px] leading-relaxed text-[var(--text-muted)]">
			{$t('rest.description')}
		</p>
		<span
			class="mt-6 inline-flex items-center gap-2 rounded-full px-4 py-1.5 text-sm font-bold text-[var(--accent)]" style="background-color: color-mix(in srgb, var(--accent) 16%, transparent);"
		>
			<Icon name="sparkles" size={16} />
			{$t('rest.badge')}
		</span>
	</div>
</div>
