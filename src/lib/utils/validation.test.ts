import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { t } from '$lib/stores/i18n';
import { validateCookieValue, validateHeaderName } from './rest';
import { blockedMessage, propertyFieldHint, validationMessage, VALIDATION_KEYS } from './validation';

// `$t` is the locale store's translate function; tests read it once per case.
const translate = () => get(t);

describe('shared validation messages', () => {
	it('names every reason through the rest.validation keys', () => {
		const keys = Object.values(VALIDATION_KEYS);
		expect(keys).toContain('rest.validation.headerName');
		expect(keys).toContain('rest.validation.nameRequired');
		// A kind the table forgot to map would render its key instead of a sentence.
		expect(keys.every((key) => key.startsWith('rest.validation.'))).toBe(true);
	});

	it('interpolates the row name and the described character', () => {
		const issue = validateHeaderName('bad header');
		if (!issue) throw new Error('expected "bad header" to be an invalid header name');
		const message = validationMessage(issue, translate());
		expect(message).toContain('bad header');
		expect(message).toContain('␠');
	});

	it('renders a row with no name with a dash rather than an empty quote', () => {
		const message = validationMessage(
			{ kind: 'header-name', field: 'name', name: '', char: '␠' },
			translate()
		);
		expect(message).toContain('“—”');
	});

	it('wraps the first issue into the blocked message and answers null when valid', () => {
		expect(blockedMessage([], translate())).toBeNull();
		const issue = validateCookieValue('a"b');
		if (!issue) throw new Error('expected a quoted cookie value to be invalid');
		const message = blockedMessage([issue], translate());
		expect(message).toContain('Request not sent');
		expect(message).toContain('a"b');
	});
});

describe('cell-level property hints', () => {
	it('reports a missing name before the RFC rule of the cell', () => {
		// A typed name is checked by the RFC rule; the mandatory-name rule only fires
		// when the name is blank, and then it wins.
		expect(
			propertyFieldHint('header', 'name', { name: 'a b', value: 'x' }, -1, translate())
		).toContain('␠');
		const missing = propertyFieldHint('header', 'name', { name: '', value: 'x' }, 2, translate());
		expect(missing).not.toBeNull();
		expect(missing).toContain('name is required');
	});

	it('marks the value cell of a name-less row by its own text', () => {
		const hint = propertyFieldHint('cookie', 'value', { name: '', value: 'a;b' }, 0, translate());
		expect(hint).not.toBeNull();
		// The row has no name, so the message quotes the offending text instead.
		expect(hint).toContain('a;b');
	});

	it('stays silent for a cell that is valid or still empty', () => {
		expect(
			propertyFieldHint('header', 'name', { name: 'X-Token', value: '1' }, 0, translate())
		).toBeNull();
		// Typing a long value before the name must not be flagged while the row is a draft.
		expect(propertyFieldHint('header', 'name', { name: '', value: '' }, -1, translate())).toBeNull();
	});
});
