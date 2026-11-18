<script lang="ts">
	import { page } from '$app/stores';
	import Icon from '$lib/components/Icon.svelte';
	import { locale, setLocale, t, type Locale } from '$lib/stores/i18n';
	import { cycleTheme, themeIcon, themeMode } from '$lib/stores/theme';

	const items = [
		{ path: '/xml', label: 'nav.xml' },
		{ path: '/json', label: 'nav.json' },
		{ path: '/url', label: 'nav.url' },
		{ path: '/compare', label: 'nav.compare' },
		{ path: '/share', label: 'nav.share' },
		{ path: '/rest', label: 'nav.rest' }
	];

	function isActive(path: string): boolean {
		return $page.url.pathname === path;
	}

	function onLocaleChange(event: Event): void {
		setLocale((event.currentTarget as HTMLSelectElement).value as Locale);
	}
</script>

<header
	class="relative z-20 flex h-[76px] shrink-0 items-center gap-4 bg-[var(--nav-bg)] px-5 shadow-[0_10px_30px_-20px_rgba(0,0,0,0.8)]"
>
	<nav
		class="flex min-w-0 flex-1 items-center gap-3 overflow-x-auto overflow-y-hidden py-1"
		aria-label="Main navigation"
	>
		{#each items as item (item.path)}
			<a
				href={item.path}
				class="shrink-0 rounded-full px-6 py-2.5 text-[15px] font-semibold transition-all duration-150 hover:scale-105 active:scale-95 {isActive(
					item.path
				)
					? 'bg-[var(--nav-active-bg)] text-[var(--nav-active-text)] shadow-[0_10px_24px_-12px_rgba(0,0,0,0.8)]'
					: 'bg-[var(--nav-inactive-bg)] text-[var(--nav-inactive-text)]'}"
				aria-current={isActive(item.path) ? 'page' : undefined}
			>
				{$t(item.label)}
			</a>
		{/each}
	</nav>

	<div class="flex shrink-0 items-center gap-2.5">
		<div class="relative">
			<label class="sr-only" for="locale-select">{$t('lang.label')}</label>
			<select
				id="locale-select"
				value={$locale}
				onchange={onLocaleChange}
				class="h-11 appearance-none rounded-full bg-[var(--nav-active-bg)] pl-5 pr-10 text-[15px] font-semibold text-white outline-none transition-all duration-150 hover:scale-105 focus-visible:ring-2 focus-visible:ring-white/70 active:scale-95"
			>
				<option value="en">en</option>
				<option value="ru">ru</option>
			</select>
			<Icon
				name="chevronDown"
				size={16}
				class="pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 text-white/80"
			/>
		</div>

		<button
			type="button"
			onclick={cycleTheme}
			class="flex h-11 w-11 items-center justify-center rounded-full bg-[var(--nav-active-bg)] text-white transition-all duration-150 hover:scale-105 active:scale-95"
			title="{$t('theme.switch')} · {$t(`theme.${$themeMode}`)}"
			aria-label="{$t('theme.switch')}"
		>
			<Icon name={themeIcon($themeMode)} size={19} />
		</button>
	</div>
</header>
