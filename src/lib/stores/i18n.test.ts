import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { en, locale, ru, setLocale, t } from './i18n';

type Dictionary = Record<string, string>;

function keysOf(dictionary: Dictionary): string[] {
	return Object.keys(dictionary);
}

function sortedIn(text: string): string[] {
	const names = text.match(/\{[A-Za-z][A-Za-z0-9_]*(?=\})/g) ?? [];

	return names.map((name) => name.slice(1)).sort();
}

// Keys the file dialogs, the navbar landmark and the resizer handle read, plus the
// multipart picker title. None of them appears as a literal in a page, so a
// translation added to one dictionary only stays invisible at runtime.
const SHARED_KEYS = [
	'nav.main',
	'common.resizePanes',
	'dialog.selectXml',
	'dialog.selectJson',
	'dialog.selectShare',
	'dialog.saveShared',
	'dialog.saveFormatted',
	'fileFilter.xml',
	'fileFilter.json',
	'fileFilter.text',
	'fileFilter.all',
	'rest.multipart.pickTitle'
];

const LATIN_LABELS = ['fileFilter.xml', 'fileFilter.json', 'fileFilter.text'];
const CYRILLIC = /[\u0400-\u04FF]/;

describe('locale dictionaries', () => {
	const englishKeys = keysOf(en).sort();
	const russianKeys = keysOf(ru).sort();

	it('keeps the same key set in both languages', () => {
		expect(englishKeys).toEqual(russianKeys);
		expect(englishKeys.length).toBe(russianKeys.length);
		expect(englishKeys).toContain('nav.main');
	});

	it('carries every key the layout, navbar and file dialogs read', () => {
		for (const key of SHARED_KEYS) {
			expect(typeof en[key]).toBe('string');
			expect(typeof ru[key]).toBe('string');
			const pattern = LATIN_LABELS.includes(key) ? /^[A-Za-z ]+$/ : CYRILLIC;

			expect(ru[key]).toMatch(pattern);
		}
	});

	it('interpolates the same parameters in both languages', () => {
		for (const key of keysOf(en)) {
			expect(sortedIn(en[key])).toEqual(sortedIn(ru[key]));
		}
	});

	it('answers an unknown key as the key itself', () => {
		const translate = get(t);
		expect(translate('nothing.that.does.not.exist')).toBe('nothing.that.does.not.exist');
		// A placeholder without a matching parameter stays on screen, not empty.
		expect(translate('update.downloading', { version: '0.1.0' })).toContain('{percent}');
	});

	it('switches the whole UI when the locale changes', () => {
		setLocale('ru');
		expect(get(locale)).toBe('ru');
		const russian = get(t);
		expect(russian('nav.main')).toMatch(CYRILLIC);
		setLocale('en');
		const english = get(t);
		expect(english('nav.main')).toBe(en['nav.main']);
	});
});
