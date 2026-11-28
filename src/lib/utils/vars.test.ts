import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { t } from '$lib/stores/i18n';
import {
	MAX_VAR_NAME_CHARS,
	SECRET_MASK,
	containsTemplate,
	describeVarError,
	extractTemplateNames,
	maskedValues,
	plainValues,
	referencedVarNames,
	requestHasTemplates,
	soapHasTemplates,
	substituteText,
	substituteUrl,
	templateFor,
	varValues,
	validateVarName,
	type TemplateSource,
	type VarRecord,
	type VarValues
} from './vars';

// `$t` is the locale store's translate function; tests read it once per case.
const translate = () => get(t);

function request(partial: Partial<TemplateSource> = {}): TemplateSource {
	return {
		url: 'https://example.com/p',
		headers: [],
		body: '',
		body_type: 'json',
		form: [],
		multipart: [],
		...partial
	};
}

describe('template grammar', () => {
	it('finds a name whatever spacing the user puts inside the braces', () => {
		expect(extractTemplateNames('{{token}}')).toEqual(['token']);
		expect(extractTemplateNames('{{ token }}')).toEqual(['token']);
		expect(extractTemplateNames('{{  TOKEN  }}')).toEqual(['token']);
		expect(containsTemplate('no braces here')).toBe(false);
	});

	it('keeps the order of appearance and lists a repeated name once', () => {
		expect(extractTemplateNames('{{b}} and {{a}} again {{B}}')).toEqual(['b', 'a']);
	});

	it('does not treat malformed braces as a variable', () => {
		// A name with a space, a digit-leading name and an empty name are not names,
		// so the text goes to the wire exactly as typed.
		expect(extractTemplateNames('{{ a b }}')).toEqual([]);
		expect(extractTemplateNames('{{1}}')).toEqual([]);
		expect(extractTemplateNames('{{}}')).toEqual([]);
		expect(extractTemplateNames('{{ unterminated')).toEqual([]);
	});

	it('reads a nested opening brace as literal text, like the Rust scanner', () => {
		// The closing `}}` is taken by the outer pair, whose inner text is not a name,
		// and the inner `{{b}}` is the one that resolves.
		expect(extractTemplateNames('{{a{{b}}}}')).toEqual(['b']);
	});

	it('builds the template the copy button puts on the clipboard', () => {
		expect(templateFor('Token')).toBe('{{ Token }}');
		expect(templateFor('  token ')).toBe('{{ token }}');
	});
});

describe('where a request can carry a template', () => {
	it('scans the URL, the headers, the body and the tables of the active tab', () => {
		expect(
			requestHasTemplates(request({ headers: [{ name: 'Authorization', value: 'Bearer {{token}}' }] }))
		).toBe(true);
		expect(requestHasTemplates(request({ body: '{"id": "{{id}}"}' }))).toBe(true);
		expect(
			requestHasTemplates(request({ body_type: 'form', form: [{ name: 'q', value: '{{term}}' }] }))
		).toBe(true);
		expect(
			requestHasTemplates(
				request({ body_type: 'multipart', multipart: [{ name: 'field', value: '{{term}}' }] })
			)
		).toBe(true);
		// The Form-encode table is only read when that tab is the one being sent.
		expect(
			requestHasTemplates(request({ body_type: 'json', form: [{ name: 'q', value: '{{term}}' }] }))
		).toBe(false);
		expect(requestHasTemplates(request())).toBe(false);
	});

	it('finds a template the Query table wrote into the URL percent-encoded', () => {
		// `buildQuery` turns `{{ token }}` into `%7B%7Btoken%7D%7D`; the backend reads the
		// decoded pair, so the client guard has to decode it too.
		const source = request({ url: 'https://example.com/p?q=%7B%7Btoken%7D%7D' });
		expect(referencedVarNames(source)).toEqual(['token']);
	});

	it('counts a template in a part name or a file row as a template', () => {
		// Neither a part name nor a file row is ever substituted, and the backend
		// refuses them, so the export guards have to see them too.
		expect(
			requestHasTemplates(
				request({ body_type: 'multipart', multipart: [{ name: '{{field}}', value: 'v' }] })
			)
		).toBe(true);
		expect(
			requestHasTemplates(
				request({
					body_type: 'multipart',
					files: [{ name: 'part', path: '{{dir}}/report.pdf', filename: 'report.pdf' }]
				})
			)
		).toBe(true);
		// File rows are only read when the Form tab is what goes on the wire.
		expect(
			requestHasTemplates(
				request({ body_type: 'json', files: [{ name: 'p', path: '{{dir}}', filename: 'x' }] })
			)
		).toBe(false);
	});

	it('reads the body buffer only when that tab is what goes on the wire', () => {
		// A JSON or XML buffer left over from another tab is never sent, so it must not
		// block a Form-encode request that references nothing.
		expect(
			requestHasTemplates(request({ body_type: 'form', body: '{"id": "{{id}}"}' }))
		).toBe(false);
		expect(
			requestHasTemplates(request({ body_type: 'multipart', body: '{"id": "{{id}}"}' }))
		).toBe(false);
		expect(requestHasTemplates(request({ body_type: 'xml', body: '<i>{{id}}</i>' }))).toBe(true);
		expect(requestHasTemplates(request({ body_type: 'text', body: '{{id}}' }))).toBe(true);
	});

	it('refuses a template in the URL path, a query name and the host, like the backend does', () => {
		// `Url::parse` percent-encodes the braces of a path or a host, and a query name is
		// never substituted, so all three reach `unresolved-template:` on the desktop.
		expect(requestHasTemplates(request({ url: 'https://example.com/{{seg}}' }))).toBe(true);
		expect(requestHasTemplates(request({ url: 'https://example.com/p?{{flag}}=1' }))).toBe(true);
		expect(requestHasTemplates(request({ url: 'https://{{host}}/p' }))).toBe(true);
		expect(
			requestHasTemplates(request({ url: 'https://example.com/p?%7B%7Bflag%7D%7D=1' }))
		).toBe(true);
		// A template in the port is not a name the resolver can apply either.
		const portUrl = request({ url: 'https://example.com:{{port}}/p' });
		expect(requestHasTemplates(portUrl)).toBe(true);
		expect(referencedVarNames(portUrl)).toEqual(['port']);
	});

	it('counts a template in a header name, which the backend refuses too', () => {
		const source = request({ headers: [{ name: '{{name}}', value: 'v' }] });
		expect(requestHasTemplates(source)).toBe(true);
	});

	it('scans a SOAP call over its envelope and the whole header table', () => {
		// A SOAP call has no URL table, query rows or Form tabs, so those places cannot
		// appear here; the header **names** are read like the REST table's.
		expect(soapHasTemplates({ body: '<i>plain</i>', headers: [] })).toBe(false);
		expect(soapHasTemplates({ body: '<i>{{id}}</i>', headers: [] })).toBe(true);
		expect(
			soapHasTemplates({ body: '<i>plain</i>', headers: [{ name: 'X-Token', value: '{{t}}' }] })
		).toBe(true);
		expect(
			soapHasTemplates({ body: '<i>plain</i>', headers: [{ name: '{{name}}', value: 'v' }] })
		).toBe(true);
	});

	it('names every variable once, across all the places at once', () => {
		const source = request({
			url: 'https://example.com/p?id={{id}}',
			headers: [{ name: 'X-Token', value: '{{token}}' }, { name: 'X-Id', value: '{{ID}}' }],
			body: '{"id": "{{id}}"}'
		});
		expect(referencedVarNames(source)).toEqual(['id', 'token']);
	});
});

describe('variable names', () => {
	it('accepts a C-style identifier and rejects everything else', () => {
		expect(validateVarName('Token_1', [])).toBeNull();
		expect(validateVarName('_private', [])).toBeNull();
		expect(validateVarName('1token', [])).toBe('invalid');
		expect(validateVarName('a token', [])).toBe('invalid');
		expect(validateVarName('a-token', [])).toBe('invalid');
		expect(validateVarName('{{token}}', [])).toBe('invalid');
		expect(validateVarName('токен', [])).toBe('invalid');
		expect(validateVarName('x'.repeat(MAX_VAR_NAME_CHARS), [])).toBeNull();
		expect(validateVarName('x'.repeat(MAX_VAR_NAME_CHARS + 1), [])).toBe('invalid');
		expect(validateVarName('   ', [])).toBe('required');
	});

	it('compares duplicates without case and never with the row being edited', () => {
		const rows = [{ id: 'a', name: 'Token', secure: false }];
		expect(validateVarName('token', rows)).toBe('duplicate');
		expect(validateVarName('token', rows, 'a')).toBeNull();
	});
});

describe('backend error texts', () => {
	it('localizes each code the resolver can answer', () => {
		const say = translate();
		expect(describeVarError('unknown-var:token', say)).toBe(say('vars.unknown', { name: 'token' }));
		expect(describeVarError('secret-missing:token', say)).toBe(
			say('vars.secretMissing', { name: 'token' })
		);
		expect(describeVarError('secret-store:locked', say)).toBe(
			say('vars.secretStore', { message: 'locked' })
		);
		expect(describeVarError('secret-too-large', say)).toBe(say('vars.secretTooLarge'));
		expect(describeVarError('duplicate-name:Token', say)).toBe(say('vars.nameDuplicate', { name: 'Token' }));
		expect(describeVarError('invalid-name:a b', say)).toBe(say('vars.nameInvalid', { name: 'a b' }));
		expect(describeVarError('unresolved-template:host', say)).toBe(
			say('vars.unsupportedTemplate', { name: 'host' })
		);
		// `<field>=<char>`: a control character a variable produced, which client
		// validation could not see because the cell holds the template text. The glyph is
		// the one `utils/rest.ts::describeCharacter` prints, so a bad value reads like a
		// bad cell.
		expect(describeVarError('invalid-value:Authorization=␍', say)).toBe(
			say('vars.invalidValue', { field: 'Authorization', char: '␍' })
		);
		expect(describeVarError('invalid-value:note=U+0000', say)).toBe(
			say('vars.invalidValue', { field: 'note', char: 'U+0000' })
		);
		// A multipart part name is not an RFC token in Rust, so it may itself contain `=`;
		// the glyph is still the text after the last one.
		expect(describeVarError('invalid-value:a=b=␊', say)).toBe(
			say('vars.invalidValue', { field: 'a=b', char: '␊' })
		);
	});

	it('shows a message it does not know verbatim, and nothing for an empty one', () => {
		const say = translate();
		expect(describeVarError('Could not reach the server', say)).toBe('Could not reach the server');
		// An empty failure answers `''` so the caller's fallback names the action that
		// failed rather than a generic error, and an `Error` is read through its `message`
		// the way `notifyError` reads it — a browser failure keeps its own wording.
		expect(describeVarError('', say)).toBe('');
		expect(describeVarError(undefined, say)).toBe('');
		expect(describeVarError(new Error('unknown-var:token'), say)).toBe(
			say('vars.unknown', { name: 'token' })
		);
		expect(describeVarError(new TypeError('Failed to fetch'), say)).toBe('Failed to fetch');
	});

	it('masks a safe value with the same text the dump uses', () => {
		expect(SECRET_MASK).toBe('********');
	});
});

describe('rendering templates for a dump or an export', () => {
	const rows: VarRecord[] = [
		{ id: 'a1', name: 'Host', secure: false, value: 'v1' },
		{ id: 'a2', name: 'Token', secure: true, value: null }
	];

	it('indexes by the lowercased name and starts a safe row as the mask', () => {
		const values = varValues(rows);
		expect(Object.keys(values)).toEqual(['host', 'token']);
		expect(values.host).toEqual({ id: 'a1', secure: false, value: 'v1' });
		// The list never carries a secret, so the mask is what a dump reads by default.
		expect(values.token.value).toBe(SECRET_MASK);
	});

	it('masks a safe value and keeps a plain one', () => {
		const revealed = { ...varValues(rows), token: { id: 'a2', secure: true, value: 's3cret' } };
		expect(maskedValues(revealed).token.value).toBe(SECRET_MASK);
		expect(maskedValues(revealed).host.value).toBe('v1');
	});

	it('drops safe entries so their template stays text', () => {
		const values = varValues(rows);
		expect(Object.keys(plainValues(values))).toEqual(['host']);
		expect(substituteText('Bearer {{ token }}', plainValues(values))).toBe(
			'Bearer {{ token }}'
		);
	});

	it('substitutes a known name and leaves an unknown one as typed', () => {
		const values = varValues(rows);
		expect(substituteText('Bearer {{ Token }}', values)).toBe(`Bearer ${SECRET_MASK}`);
		expect(substituteText('{{ host }}:8080', values)).toBe('v1:8080');
		expect(substituteText('{{ missing }}', values)).toBe('{{ missing }}');
		// Text without braces is returned untouched, so no scan runs over a big body.
		expect(substituteText('<i>plain</i>', values)).toBe('<i>plain</i>');
	});

	it('never rescans the value it wrote, like the Rust scanner', () => {
		const values: VarValues = {
			a: { id: 'a', secure: false, value: '{{b}}' },
			b: { id: 'b', secure: false, value: 'v' }
		};
		// One pass: `{{a}}` becomes the text `{{b}}`, which is not expanded again.
		expect(substituteText('{{a}}', values)).toBe('{{b}}');
		// The outer pair is not a name, so only the inner one resolves.
		expect(substituteText('{{a{{b}}}}', values)).toBe('{{av}}');
	});

	it('substitutes query values and keeps every other segment as typed', () => {
		const values = varValues([{ id: 'q', name: 'q', secure: false, value: 'v' }]);
		expect(
			substituteUrl('https://e.com/p?debug&a=1&&keep=a%20b&x=%7B%7Bq%7D%7D&y=%5E', values)
		).toBe('https://e.com/p?debug&a=1&&keep=a%20b&x=v&y=%5E');
		// A rewritten value is written the way the backend writes it: `+` for a space.
		expect(
			substituteUrl('https://e.com/p?q={{q}}', {
				q: { id: 'q', secure: false, value: 'a b&c' }
			})
		).toBe('https://e.com/p?q=a+b%26c');
		// A template in a parameter **name** is never substituted, and a URL without a
		// query is returned as it stands.
		expect(substituteUrl('https://e.com/p?{{q}}=1', values)).toBe('https://e.com/p?{{q}}=1');
		expect(substituteUrl('https://e.com/p', values)).toBe('https://e.com/p');
	});

	it('finds a template the Query table wrote, where the braces and spaces are encoded', () => {
		const values = varValues([{ id: 'q', name: 'q', secure: false, value: 'v' }]);
		// `buildQuery` writes `{{ q }}` as `%7B%7B%20q%20%7D%7D`, so scanning the raw
		// segment would never match it.
		expect(substituteUrl('https://e.com/p?id=%7B%7B%20q%20%7D%7D', values)).toBe(
			'https://e.com/p?id=v'
		);
		expect(substituteUrl('https://e.com/p?id=%7b%7b%20q%20%7d%7d', values)).toBe(
			'https://e.com/p?id=v'
		);
		// A safe value keeps the mask in that position too.
		expect(
			substituteUrl('https://e.com/p?t=%7B%7B%20q%20%7D%7D', {
				q: { id: 'q', secure: true, value: SECRET_MASK }
			})
		).toBe('https://e.com/p?t=********');
	});

	it('keeps the fragment outside the query it rewrites', () => {
		const values = varValues([{ id: 'q', name: 'q', secure: false, value: 'v' }]);
		expect(substituteUrl('https://e.com/p?q=%7B%7B%20q%20%7D%7D#section', values)).toBe(
			'https://e.com/p?q=v#section'
		);
		expect(substituteUrl('https://e.com/p?a=1#{{q}}', values)).toBe('https://e.com/p?a=1#{{q}}');
	});
});
