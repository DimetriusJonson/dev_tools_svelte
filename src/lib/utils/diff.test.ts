import { describe, expect, it } from 'vitest';
import { diffTexts, lineOffsets, splitLines } from './diff';

describe('diffTexts', () => {
	it('detects identical texts', () => {
		const result = diffTexts('a\nb\nc', 'a\nb\nc');
		expect(result.identical).toBe(true);
		expect(result.left).toHaveLength(0);
		expect(result.right).toHaveLength(0);
	});

	it('detects changed lines and inline ranges', () => {
		const left = '<airport>Анталья</airport>';
		const right = '<airport>Внуково</airport>';
		const result = diffTexts(left, right);
		expect(result.identical).toBe(false);
		expect(result.stats.changed).toBe(1);
		expect(result.left[0].inline[0]).toEqual({ start: 9, end: 16 });
		expect(result.right[0].inline[0]).toEqual({ start: 9, end: 16 });
	});

	it('detects added and removed lines', () => {
		const result = diffTexts('a\nb\nc', 'a\nc\nd');
		expect(result.stats.removed).toBeGreaterThanOrEqual(1);
		expect(result.stats.added).toBeGreaterThanOrEqual(1);
		expect(result.left.some((line) => line.line === 2)).toBe(true);
		expect(result.right.some((line) => line.line === 3)).toBe(true);
	});

	it('pairs a replaced block line by line', () => {
		const result = diffTexts('one\ntwo', 'ONE\nTWO');
		expect(result.stats.changed).toBe(2);
		expect(result.stats.added).toBe(0);
		expect(result.stats.removed).toBe(0);
	});

	it('handles CRLF input', () => {
		expect(splitLines('a\r\nb')).toEqual(['a', 'b']);
	});

	it('returns line offsets for CodeMirror ranges', () => {
		expect(lineOffsets('ab\ncd\nef')).toEqual([0, 3, 6]);
	});
});
