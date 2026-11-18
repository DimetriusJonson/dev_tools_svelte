import { derived, get, writable } from 'svelte/store';

export type ThemeMode = 'system' | 'dark' | 'light';

const STORAGE_KEY = 'devtools.theme';
const ORDER: ThemeMode[] = ['system', 'dark', 'light'];

function loadTheme(): ThemeMode {
	if (typeof window === 'undefined') return 'system';
	const stored = window.localStorage.getItem(STORAGE_KEY);
	if (stored === 'dark' || stored === 'light' || stored === 'system') return stored;
	return 'system';
}

export const themeMode = writable<ThemeMode>(loadTheme());

export const resolvedTheme = derived(themeMode, ($mode, set) => {
	if ($mode === 'dark' || $mode === 'light') {
		set($mode);
		return;
	}
	if (typeof window === 'undefined') {
		set('dark');
		return;
	}
	const query = window.matchMedia('(prefers-color-scheme: dark)');
	const update = () => set(query.matches ? 'dark' : 'light');
	update();
	query.addEventListener('change', update);
	return () => query.removeEventListener('change', update);
});

export const isDark = derived(resolvedTheme, ($resolved) => $resolved === 'dark');

export function setTheme(mode: ThemeMode): void {
	themeMode.set(mode);
	if (typeof window !== 'undefined') {
		window.localStorage.setItem(STORAGE_KEY, mode);
	}
}

export function cycleTheme(): void {
	const current = get(themeMode);
	const index = ORDER.indexOf(current);
	setTheme(ORDER[(index + 1) % ORDER.length]);
}

export function themeIcon(mode: ThemeMode): 'monitor' | 'moon' | 'sun' {
	if (mode === 'dark') return 'moon';
	if (mode === 'light') return 'sun';
	return 'monitor';
}
