import { describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { t } from '$lib/stores/i18n';
import {
	MAX_VAR_NAME_CHARS,
	SECRET_MASK,
	containsTemplate,
	describeVarError,
	extractTemplateNames,
	requestHasTemplates,
	referencedVarNames,
	templateFor,
	validateVarName,
	type TemplateSource
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
	});

	it('shows a message it does not know verbatim, and the generic one for nothing', () => {
		const say = translate();
		expect(describeVarError('Could not reach the server', say)).toBe('Could not reach the server');
		expect(describeVarError('', say)).toBe(say('common.error'));
	});

	it('masks a safe value with the same text the dump uses', () => {
		expect(SECRET_MASK).toBe('********');
	});
});
