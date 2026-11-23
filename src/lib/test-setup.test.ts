import { describe, expect, it } from 'vitest';
import { MemoryStorage } from './test-setup';

describe('test environment', () => {
	it('exposes a usable localStorage on any Node version', () => {
		expect(typeof window.localStorage?.setItem).toBe('function');
		window.localStorage.setItem('devtools.env.probe', 'value');
		expect(window.localStorage.getItem('devtools.env.probe')).toBe('value');
		window.localStorage.removeItem('devtools.env.probe');
		expect(window.localStorage.getItem('devtools.env.probe')).toBeNull();
	});
});

describe('MemoryStorage', () => {
	it('round trips keys and reports missing ones as null', () => {
		const storage = new MemoryStorage();
		storage.setItem('a', '1');
		expect(storage.getItem('a')).toBe('1');
		expect(storage.getItem('b')).toBeNull();
		expect(storage.length).toBe(1);
		storage.removeItem('a');
		expect(storage.getItem('a')).toBeNull();
		expect(storage.length).toBe(0);
	});

	it('indexes keys and clears everything', () => {
		const storage = new MemoryStorage();
		storage.setItem('a', '1');
		storage.setItem('b', '2');
		expect(storage.key(0)).toBe('a');
		expect(storage.key(1)).toBe('b');
		expect(storage.key(2)).toBeNull();
		storage.clear();
		expect(storage.length).toBe(0);
		expect(storage.key(0)).toBeNull();
	});

	it('coerces values to text like a browser Storage does', () => {
		const storage = new MemoryStorage();
		storage.setItem('n', 7 as unknown as string);
		expect(storage.getItem('n')).toBe('7');
	});
});
