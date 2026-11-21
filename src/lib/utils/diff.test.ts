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

	it('ignores line-ending style when comparing', () => {
		const mixed = 'a\r\nb\rc';
		const unix = 'a\nb\nc';
		const result = diffTexts(mixed, unix);
		expect(result.identical).toBe(true);
		expect(result.left).toEqual([]);
		expect(result.right).toEqual([]);
	});

	it('reports two empty texts as identical', () => {
		const empty = diffTexts('', '');
		expect(empty.identical).toBe(true);
		expect(empty.left).toEqual([]);
		expect(empty.right).toEqual([]);
	});

	it('marks an empty line against a filled one as changed', () => {
		const againstEmpty = diffTexts('', 'a');
		expect(againstEmpty.stats.changed).toBe(1);
		expect(againstEmpty.left[0].inline).toEqual([]);
		expect(againstEmpty.right[0].inline).toEqual([{ start: 0, end: 1 }]);
	});

	it('adds an empty line with no inline range', () => {
		const trailing = diffTexts('a', 'a\n');
		expect(trailing.stats.added).toBe(1);
		expect(trailing.right[0].line).toBe(2);
		expect(trailing.right[0].kind).toBe('added');
		expect(trailing.right[0].inline).toEqual([]);
	});

	it('marks a removed empty line with no inline range', () => {
		const removedBlank = diffTexts('a\n', 'a');
		expect(removedBlank.stats.removed).toBe(1);
		expect(removedBlank.left[0].kind).toBe('removed');
		expect(removedBlank.left[0].inline).toEqual([]);
	});

	it('diffs a code line inline', () => {
		const code = diffTexts('const value = 1;', 'const value = 22;');
		expect(code.stats.changed).toBe(1);
		expect(code.left[0].inline).toEqual([{ start: 14, end: 16 }]);
		expect(code.right[0].inline).toEqual([{ start: 14, end: 17 }]);
	});

	it('merges adjacent inline ranges', () => {
		const merged = diffTexts('foo,bar', 'foo,BAZ');
		expect(merged.left[0].inline).toEqual([{ start: 4, end: 7 }]);
		expect(merged.right[0].inline).toEqual([{ start: 4, end: 7 }]);
	});
});

describe('diffTexts caps and fallbacks', () => {
	it('marks the whole line when a pair has too many tokens', () => {
		const wide = Array.from({ length: 501 }, (_, index) => 'word' + index).join(' ');
		const wideAlt = Array.from({ length: 501 }, (_, index) => 'other' + index).join(' ');
		const result = diffTexts(wide, wideAlt);
		expect(result.stats.changed).toBe(1);
		expect(result.left[0].inline).toEqual([{ start: 0, end: wide.length }]);
		expect(result.right[0].inline).toEqual([{ start: 0, end: wideAlt.length }]);
	});

	it('keeps common prefix and suffix lines on the cheap path', () => {
		const breakCode = String.fromCharCode(10);
		const prefix = Array.from({ length: 1000 }, (_, index) => 'prefix' + index).join(breakCode);
		const suffix = Array.from({ length: 999 }, (_, index) => 'suffix' + index).join(breakCode);
		const tallLeft = [prefix, 'x0', 'x1', suffix].join(breakCode);
		const tallRight = [prefix, 'y0', 'y1', suffix].join(breakCode);
		const result = diffTexts(tallLeft, tallRight);
		expect(result.stats.changed).toBe(2);
		expect(result.stats.added).toBe(0);
		expect(result.stats.removed).toBe(0);
		expect(result.left).toHaveLength(2);
		expect(result.left[0].line).toBe(1001);
		expect(result.left[1].line).toBe(1002);
		expect(result.right).toHaveLength(2);
		expect(result.right[0].line).toBe(1001);
		expect(result.right[1].line).toBe(1002);
	});

	it('uses one tokenizer for both sides of a changed line', () => {
		const result = diffTexts('x > y', 'x > z');
		expect(result.left[0].inline).toEqual([{ start: 4, end: 5 }]);
		expect(result.right[0].inline).toEqual([{ start: 4, end: 5 }]);
	});

	it('falls back to whole-line marks when tokens match but lines do not', () => {
		const result = diffTexts('"abc', 'abc');
		expect(result.stats.changed).toBe(1);
		expect(result.left[0].inline).toEqual([{ start: 0, end: 4 }]);
		expect(result.right[0].inline).toEqual([{ start: 0, end: 3 }]);
	});

	it('counts code units of a surrogate pair when marking inline', () => {
		const lamp = '\u{1f4a1}';
		const result = diffTexts('a ' + lamp + ' b', 'a ' + lamp + ' c');
		expect(result.left[0].inline).toEqual([{ start: 5, end: 6 }]);
		expect(result.right[0].inline).toEqual([{ start: 5, end: 6 }]);
	});

	it('reports the extra removals of an unequal block', () => {
		const result = diffTexts('x\ny\nz', 'z');
		expect(result.stats.removed).toBe(2);
		expect(result.stats.added).toBe(0);
		expect(result.stats.changed).toBe(0);
		expect(result.left).toHaveLength(2);
		expect(result.right).toEqual([]);
	});

	it('pairs a block line by line and leaves the rest removed', () => {
		const result = diffTexts('a\nb\nc\nd', 'a\nX\nd');
		expect(result.stats.changed).toBe(1);
		expect(result.stats.removed).toBe(1);
		expect(result.left[0].line).toBe(2);
		expect(result.left[0].kind).toBe('changed');
		expect(result.left[1].line).toBe(3);
		expect(result.left[1].kind).toBe('removed');
		expect(result.right).toHaveLength(1);
	});

	it('points only at lines that exist after normalization', () => {
		const left = 'a\r\nb\rc\n\nlast';
		const right = 'x\ny';
		const result = diffTexts(left, right);
		const leftCount = splitLines(left).length;
		const rightCount = splitLines(right).length;
		expect(result.left.every((entry) => entry.line >= 1 && entry.line <= leftCount)).toBe(true);
		expect(result.right.every((entry) => entry.line >= 1 && entry.line <= rightCount)).toBe(true);
	});

	it('never emits an inverted or out-of-line inline range', () => {
		const left = 'a\r\nb\rc\n\nlast';
		const right = 'x\ny';
		const result = diffTexts(left, right);
		const leftFits = result.left.every((entry) => entry.inline.every((range) => range.end >= range.start && range.end <= (splitLines(left)[entry.line - 1] ?? '').length));
		const rightFits = result.right.every((entry) => entry.inline.every((range) => range.end >= range.start && range.end <= (splitLines(right)[entry.line - 1] ?? '').length));
		expect(leftFits).toBe(true);
		expect(rightFits).toBe(true);
	});
});

describe('splitLines line endings', () => {
	it('splits an empty text as one empty line', () => {
		const empty = splitLines('');
		expect(empty).toEqual(['']);
	});

	it('keeps an empty line after a trailing break', () => {
		const trailing = splitLines('a\n');
		expect(trailing).toEqual(['a', '']);
	});

	it('normalizes a lone CR as a line break', () => {
		const crOnly = splitLines('a\rb');
		expect(crOnly).toEqual(['a', 'b']);
	});

	it('normalizes mixed line endings', () => {
		const mixed = splitLines('a\r\nb\rc\nd');
		expect(mixed).toEqual(['a', 'b', 'c', 'd']);
	});
});

describe('lineOffsets matches splitLines', () => {
	it('counts an empty text as one line at zero', () => {
		const offsets = lineOffsets('');
		expect(offsets).toEqual([0]);
	});

	it('treats CRLF as one break two characters wide', () => {
		const offsets = lineOffsets('a\r\nb');
		expect(offsets).toEqual([0, 3]);
	});

	it('treats a lone CR as a one-character break', () => {
		const offsets = lineOffsets('a\rb');
		expect(offsets).toEqual([0, 2]);
	});

	it('reports a start for the empty line after a trailing break', () => {
		const offsets = lineOffsets('a\n');
		expect(offsets).toEqual([0, 2]);
	});

	it('gives splitLines a start position for every line', () => {
		const texts = ['', 'a', 'a\n', 'a\r\n', 'a\r', 'a\rb', 'a\r\nb\rc\nd', 'a\n\nb'];
		const countsMatch = texts.every((text) => lineOffsets(text).length === splitLines(text).length);
		expect(countsMatch).toBe(true);
	});

	it('places CR-only breaks at the character that follows them', () => {
		const text = 'a\rb\rc';
		const offsets = lineOffsets(text);
		const starts = offsets.map((offset) => text.charAt(offset));
		expect(starts).toEqual(['a', 'b', 'c']);
	});
});

describe('diffTexts stats and highlights agree', () => {
	it('counts each highlighted line exactly once in stats', () => {
		const shift = diffTexts('a\nb\nc', 'a\nc\nd');
		const unequalBlock = diffTexts('a\nb\nc\nd', 'a\nX\nd');
		const allRemoved = diffTexts('x\ny\nz', 'z');
		const trailingBreak = diffTexts('a', 'a\n');
		const cases = [shift, unequalBlock, allRemoved, trailingBreak];
		const agreed = cases.every((entry) => entry.stats.changed + entry.stats.removed === entry.left.length && entry.stats.changed + entry.stats.added === entry.right.length);
		expect(agreed).toBe(true);
	});

	it('reports identical only when nothing is highlighted', () => {
		const same = diffTexts('a\nb', 'a\nb');
		expect(same.identical).toBe(true);
		const other = diffTexts('a\nb', 'a\nB');
		expect(other.identical).toBe(false);
	});
});
