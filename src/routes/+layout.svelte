<script lang="ts">
	import { onMount } from 'svelte';
	import { version } from '$app/environment';
	import { goto } from '$app/navigation';
	import '../app.css';
	import Navbar from '$lib/components/Navbar.svelte';
	import Toaster from '$lib/components/Toaster.svelte';
	import { isTauriRuntime } from '$lib/services/db';
	import { t } from '$lib/stores/i18n';
	import { isDark } from '$lib/stores/theme';
	import { buildViewerUrl, consumePendingDeepLink, listenForDeepLinks } from '$lib/services/share';
	import { dismissSplash, setSplashTitle, setSplashVersion } from '$lib/utils/splash';

	let { children } = $props();

	onMount(() => {
		setSplashTitle($t('app.title'));
		setSplashVersion(version);
		dismissSplash();

		let cancelled = false;
		let unlisten: (() => void) | undefined;

		(async () => {
			unlisten = await listenForDeepLinks((target) => {
				if (!cancelled) void goto(buildViewerUrl(target));
			});
			if (cancelled) {
				unlisten();
				return;
			}
			const pending = await consumePendingDeepLink();
			if (pending && !cancelled) void goto(buildViewerUrl(pending));
		})();

		return () => {
			cancelled = true;
			unlisten?.();
		};
	});

	$effect(() => {
		const dark = $isDark;
		document.documentElement.classList.toggle('dark', dark);
		document.documentElement.dataset.theme = dark ? 'dark' : 'light';
	});

	$effect(() => {
		const title = `${$t('app.title')} ${version}`;
		document.title = title;
		if (isTauriRuntime()) {
			void import('@tauri-apps/api/window')
				.then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
				.catch((error) => console.warn('Failed to set window title', error));
		}
	});
</script>

<div class="flex h-screen flex-col overflow-hidden bg-[var(--app-bg)]">
	<Navbar />
	<main class="min-h-0 flex-1">
		{@render children()}
	</main>
	<Toaster />
</div>
