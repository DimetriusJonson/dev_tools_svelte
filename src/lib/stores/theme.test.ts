import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cycleTheme, setTheme, themeIcon, themeMode, resolvedTheme, isDark } from './theme';

const STORAGE_KEY = 'devtools.theme';

function stubMatchMedia(dark: boolean): void {
	vi.stubGlobal(
		'matchMedia',
		vi.fn((query: string) => ({
			matches: query === '(prefers-color-scheme: dark)' ? dark : false,
			addEventListener: vi.fn(),
			removeEventListener: vi.fn()
		}))
	);
}

beforeEach(() => {
	localStorage.clear();
	stubMatchMedia(false);
});

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('theme store', () => {
	it('falls back to system when storage is empty or holds garbage', () => {
		expect(get(themeMode)).toBe('system');
		localStorage.setItem(STORAGE_KEY, 'xkcd');
		expect(get(themeMode)).toBe('system');
	});

	it('remembers a chosen mode in localStorage and only that mode', () => {
		setTheme('dark');
		expect(get(themeMode)).toBe('dark');
		expect(localStorage.getItem(STORAGE_KEY)).toBe('dark');

		setTheme('light');
		expect(get(themeMode)).toBe('light');
		expect(localStorage.getItem(STORAGE_KEY)).toBe('light');
	});

	it('cycles system → dark → light → system', () => {
		setTheme('system');
		cycleTheme();
		expect(get(themeMode)).toBe('dark');
		cycleTheme();
		expect(get(themeMode)).toBe('light');
		cycleTheme();
		expect(get(themeMode)).toBe('system');
	});

	it('maps modes to the icon names the navbar renders', () => {
		expect(themeIcon('system')).toBe('monitor');
		expect(themeIcon('dark')).toBe('moon');
		expect(themeIcon('light')).toBe('sun');
	});

	it('resolves system to dark when the media query matches', () => {
		stubMatchMedia(true);
		setTheme('system');
		expect(get(resolvedTheme)).toBe('dark');
		expect(get(isDark)).toBe(true);
	});

	it('resolves system to light when the media query does not match', () => {
		stubMatchMedia(false);
		setTheme('system');
		expect(get(resolvedTheme)).toBe('light');
		expect(get(isDark)).toBe(false);
	});
});