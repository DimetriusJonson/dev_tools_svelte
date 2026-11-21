import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { notify, notifyError, toasts } from './toast';
import { en, ru, setLocale } from './i18n';

describe('notification stack', () => {
	it('keeps only the newest toasts on screen', () => {
		for (const message of ['one', 'two', 'three', 'four', 'five']) notify(message, 'info');
		const stack = get(toasts);
		expect(stack).toHaveLength(4);
		expect(stack.map((item) => item.message)).toEqual(['two', 'three', 'four', 'five']);
	});

	it('keeps a real failure message ahead of the fallback', () => {
		notifyError(new Error('CORS blocked'));
		expect(get(toasts).map((item) => item.message)).toContain('CORS blocked');
	});

	it('answers a textless failure in the active language', () => {
		setLocale('ru');
		notifyError(new Error(''));
		const russian = get(toasts);
		expect(russian[russian.length - 1].message).toBe(ru['common.error']);
		setLocale('en');
		notifyError('');
		const stack = get(toasts);
		expect(stack[stack.length - 1].message).toBe(en['common.error']);
	});
});
