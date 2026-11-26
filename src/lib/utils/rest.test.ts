import { describe, expect, it } from 'vitest';
import {
	buildCookieHeader,
	buildCurl,
	buildMultipartFormData,
	buildMultipartPreview,
	buildQuery,
	buildRawRequestPreview,
	buildRawResponse,
	byteLength,
	describeCharacter,
	detectLanguageFromContentType,
	detectLanguageFromHeaders,
	filenameFromDisposition,
	isAttachmentDisposition,
	encodeForm,
	formatRestSize,
	formatRestTime,
	parseCookies,
	parseCurl,
	parseForm,
	parseQuery,
	resolveRequestBody,
	setCookieRows,
	updateUrlQuery,
	validateCookieName,
	validateCookieValue,
	validateFileRow,
	validateHeaderName,
	validateHeaderValue,
	validateProperties,
	validatePropertyField,
	validateQueryUrl,
	validateRequest,
	validateRowName,
	validateFileRows,
	basenameOf,
	contentTypeForFilename,
	MULTIPART_BOUNDARY_PLACEHOLDER,
	defaultContentType,
	getHeader,
	bodyTypeFromContentType,
	multipartBoundary
} from './rest';

describe('query parameters', () => {
	it('parses and decodes query parameters while keeping the fragment', () => {
		const rows = parseQuery('https://example.com/path?email=a%40b.c&activateUrl=https%3A%2F%2Fevil.example%2F#part');
		expect(rows).toEqual([
			{ name: 'email', value: 'a@b.c' },
			{ name: 'activateUrl', value: 'https://evil.example/' }
		]);
		expect(buildQuery(rows)).toBe(
			'email=a%40b.c&activateUrl=https%3A%2F%2Fevil.example%2F'
		);
		expect(updateUrlQuery('https://example.com/path?old=1#part', rows)).toBe(
			'https://example.com/path?email=a%40b.c&activateUrl=https%3A%2F%2Fevil.example%2F#part'
		);
	});

	it('removes the query string when every row is empty', () => {
		expect(updateUrlQuery('https://example.com/a?x=1#top', [])).toBe('https://example.com/a#top');
	});

	it('decodes plus signs as spaces', () => {
		expect(parseQuery('https://example.com/?q=hello+world')).toEqual([
			{ name: 'q', value: 'hello world' }
		]);
	});
});

describe('cookies', () => {
	it('reads and replaces the Cookie header', () => {
		const headers = [
			{ name: 'Accept', value: 'application/json' },
			{ name: 'Cookie', value: 'aid_dev=abc; theme=dark' }
		];
		const rows = parseCookies(headers);
		expect(rows).toEqual([
			{ name: 'aid_dev', value: 'abc' },
			{ name: 'theme', value: 'dark' }
		]);
		expect(buildCookieHeader(rows)).toBe('aid_dev=abc; theme=dark');

		const updated = setCookieRows(headers, [{ name: 'sid', value: '42' }]);
		expect(updated).toEqual([
			{ name: 'Accept', value: 'application/json' },
			{ name: 'Cookie', value: 'sid=42' }
		]);
		expect(setCookieRows(headers, [])).toEqual([{ name: 'Accept', value: 'application/json' }]);
	});

	it('adds the Cookie header when it is missing', () => {
		expect(setCookieRows([], [{ name: 'a', value: '1' }])).toEqual([
			{ name: 'Cookie', value: 'a=1' }
		]);
	});
});

describe('form encoding', () => {
	it('round-trips form rows', () => {
		const rows = [
			{ name: 'name', value: 'John Doe' },
			{ name: 'tag', value: 'a&b' }
		];
		expect(encodeForm(rows)).toBe('name=John+Doe&tag=a%26b');
		expect(parseForm('name=John+Doe&tag=a%26b')).toEqual(rows);
	});

	it('gives the Form-encode table priority over the text buffer', () => {
		const rows = [{ name: 'user', value: 'a' }];
		expect(resolveRequestBody({ body: '{"a":1}', body_type: 'form', form: rows })).toBe(
			'user=a'
		);
		expect(resolveRequestBody({ body: '{"a":1}', body_type: 'json', form: rows })).toBe(
			'{"a":1}'
		);
		expect(resolveRequestBody({ body: 'plain', body_type: 'text', form: rows })).toBe('plain');
		expect(resolveRequestBody({ body: '{"a":1}', body_type: 'form', form: [] })).toBe('');
	});
});

describe('cURL import', () => {
	it('parses a POSIX cURL command with headers and JSON body', () => {
		const parsed = parseCurl(
			`curl 'https://example.com/api?x=1' \\\n  -X POST \\\n  -H 'Accept: application/json' \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{"name":"John"}'`
		);
		expect(parsed).not.toBeNull();
		expect(parsed?.method).toBe('POST');
		expect(parsed?.url).toBe('https://example.com/api?x=1');
		expect(parsed?.body).toBe('{"name":"John"}');
		expect(parsed?.bodyType).toBe('json');
		expect(parsed?.headers).toEqual([
			{ name: 'Accept', value: 'application/json' },
			{ name: 'Content-Type', value: 'application/json' }
		]);
	});

	it('parses a Windows cURL.exe command with escaped quotes', () => {
		const parsed = parseCurl(
			`curl.exe "https://example.com/login" -X POST -H "Content-Type: application/x-www-form-urlencoded" --data "user=a&pass=b"`
		);
		expect(parsed?.url).toBe('https://example.com/login');
		expect(parsed?.bodyType).toBe('form');
		// Form rows are stored separately, so the text buffer stays empty.
		expect(parsed?.body).toBe('');
		expect(parsed?.form).toEqual([
			{ name: 'user', value: 'a' },
			{ name: 'pass', value: 'b' }
		]);
	});

	it('reads caret-quoted Windows commands with batch percent signs', () => {
		const parsed = parseCurl(
			`curl.exe ^"http://example.com/x?a=1&b=2^" ^\r\n-H ^"X Pct: 100%%^" ^\r\n-H ^"Accept: application/json^"`
		);
		expect(parsed?.url).toBe('http://example.com/x?a=1&b=2');
		expect(parsed?.headers).toEqual([
			{ name: 'X Pct', value: '100%' },
			{ name: 'Accept', value: 'application/json' }
		]);
	});

	it('decodes bash ANSI-C quoting produced by a browser export', () => {
		const parsed = parseCurl(
			`curl $'https://example.com/api?q=1&x=2' --request POST -H $'Content-Type: application/json' --data-raw $'{"a":1}\\n{"b":2}\\tend'`
		);
		expect(parsed?.url).toBe('https://example.com/api?q=1&x=2');
		expect(parsed?.method).toBe('POST');
		expect(parsed?.headers).toEqual([{ name: 'Content-Type', value: 'application/json' }]);
		expect(parsed?.body).toBe('{"a":1}\n{"b":2}\tend');
	});

	it('unescapes an XML payload escaped inside plain single quotes', () => {
		const parsed = parseCurl(
			`curl 'https://xml.example.com/order/book?debugNemo=true' \\
-X POST \\
-H 'Content-Type: application/xml' \\
-H 'Accept: application/xml' \\
-H 'cookie: aid_dev=867E21D71396E11B.dev' \\
--data-raw '<?xml version=\\"1.0\\" encoding=\\"UTF-8\\"?>\\n<order xmlns:xsi=\\"http://www.w3.org/2001/XMLSchema-instance\\">\\n    <OrderType>\\n        <type>Touristic</type>\\n    </OrderType>\\n    <spo>TARIFF N 8483/0Dub UAE\\n      16.01-20.12.26</spo>\\n</order>\\n'`
		);
		expect(parsed?.method).toBe('POST');
		expect(parsed?.url).toBe('https://xml.example.com/order/book?debugNemo=true');
		expect(parsed?.bodyType).toBe('xml');
		expect(parsed?.body).toBe(
			'<?xml version="1.0" encoding="UTF-8"?>\n' +
				'<order xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">\n' +
				'    <OrderType>\n' +
				'        <type>Touristic</type>\n' +
				'    </OrderType>\n' +
				'    <spo>TARIFF N 8483/0Dub UAE\n      16.01-20.12.26</spo>\n' +
				'</order>\n'
		);
		expect(parseCookies(parsed?.headers ?? [])).toEqual([
			{ name: 'aid_dev', value: '867E21D71396E11B.dev' }
		]);
	});

	it('unescapes a JSON payload escaped inside plain single quotes', () => {
		const parsed = parseCurl(
			`curl 'https://example.com/api' -X POST -H 'Content-Type: application/json' --data-raw '{\\"name\\":\\"John\\",\\n\\"age\\":30}'`
		);
		expect(parsed?.bodyType).toBe('json');
		expect(parsed?.body).toBe('{"name":"John",\n"age":30}');
	});

	it('keeps escape sequences that are genuine JSON string content', () => {
		const parsed = parseCurl(
			`curl 'https://example.com/api' -X POST -H 'Content-Type: application/json' --data-raw '{"note":"line1\\nline2","quote":"say \\"hi\\""}'`
		);
		expect(parsed?.bodyType).toBe('json');
		expect(parsed?.body).toBe('{"note":"line1\\nline2","quote":"say \\"hi\\""}');
	});

	it('keeps a multi-line body with real line breaks untouched', () => {
		const parsed = parseCurl(
			`curl 'https://example.com/api' --data-raw 'SELECT\\nfirst\nsecond'`
		);
		expect(parsed?.bodyType).toBe('text');
		expect(parsed?.body).toBe('SELECT\\nfirst\nsecond');
	});

	it('keeps empty quoted arguments as their own tokens', () => {
		const parsed = parseCurl(`curl -H '' --data-raw '' 'https://example.com/api'`);
		expect(parsed?.url).toBe('https://example.com/api');
		expect(parsed?.method).toBe('GET');
		expect(parsed?.headers).toEqual([]);
		expect(parsed?.body).toBe('');
	});

	it('keeps the form table independent from the text buffer when importing', () => {
		const parsed = parseCurl(
			`curl 'https://example.com/login' --data 'user=a&pass=b' -H 'Content-Type: application/x-www-form-urlencoded'`
		);
		expect(parsed?.method).toBe('POST');
		expect(parsed?.bodyType).toBe('form');
		expect(parsed?.body).toBe('');
		expect(parsed?.form).toEqual([
			{ name: 'user', value: 'a' },
			{ name: 'pass', value: 'b' }
		]);
	});

	it('accepts a bare URL and defaults to GET', () => {
		const parsed = parseCurl('curl https://example.com/health');
		expect(parsed?.method).toBe('GET');
		expect(parsed?.url).toBe('https://example.com/health');
	});

	it('rejects commands that are not cURL', () => {
		expect(parseCurl('wget https://example.com')).toBeNull();
	});
});

describe('cURL export', () => {
	const request = {
		method: 'POST',
		url: 'https://example.com/api',
		headers: [{ name: 'Accept', value: 'application/json' }],
		body: '{"a":"it\'s"}'
	};

	it('generates POSIX commands on one line', () => {
		expect(buildCurl(request, 'posix')).toBe(
			`curl --request POST -H 'Accept: application/json' --data-raw '{"a":"it'\\''s"}' 'https://example.com/api'`
		);
	});

	it('escapes Windows commands for cmd.exe', () => {
		const command = buildCurl(
			{
				method: 'GET',
				url: 'http://127.0.0.1:3005/test_json',
				headers: [
					{ name: 'Accept', value: 'application/json' },
					{ name: 'Accept-Language', value: 'ru' }
				],
				body: ''
			},
			'windows'
		);
		expect(command).toBe(
			'curl.exe "http://127.0.0.1:3005/test_json" ^\r\n' +
				'--request GET ^\r\n' +
				'-H "Accept: application/json" ^\r\n' +
				'-H "Accept-Language: ru"'
		);
	});

	it('escapes quotes and keeps percent signs literal in Windows arguments', () => {
		const command = buildCurl(
			{
				method: 'POST',
				url: 'https://example.com/api?q=100%',
				headers: [{ name: 'X Note', value: 'say "hi"' }],
				body: '{"a":1}'
			},
			'windows'
		);
		expect(command).toContain('curl.exe "https://example.com/api?q=100%"');
		expect(command).toContain('-H "X Note: say \\"hi\\""');
		expect(command).toContain('--data-raw "{\\"a\\":1}"');
	});

	it('reads back its own POSIX command', () => {
		const command = buildCurl(request, 'posix');
		const parsed = parseCurl(command);
		expect(parsed?.method).toBe('POST');
		expect(parsed?.url).toBe('https://example.com/api');
		expect(parsed?.headers).toEqual([{ name: 'Accept', value: 'application/json' }]);
		expect(parsed?.bodyType).toBe('json');
		expect(parsed?.body).toBe(`{"a":"it's"}`);
	});

	it('reads back its own Windows command', () => {
		const command = buildCurl(request, 'windows');
		const parsed = parseCurl(command);
		expect(parsed?.method).toBe('POST');
		expect(parsed?.url).toBe('https://example.com/api');
		expect(parsed?.headers).toEqual([{ name: 'Accept', value: 'application/json' }]);
		expect(parsed?.bodyType).toBe('json');
		expect(parsed?.body).toBe(`{"a":"it's"}`);
	});
});

describe('response helpers', () => {
	it('formats sizes and times', () => {
		expect(formatRestSize(24)).toBe('24 bytes');
		expect(formatRestSize(7074)).toBe('6.91 KiB');
		expect(formatRestTime(0)).toBe('0 ms');
		expect(formatRestTime(54)).toBe('54 ms');
		expect(formatRestTime(1234)).toBe('1.23 sec');
	});

	it('detects response languages from headers', () => {
		expect(detectLanguageFromHeaders([{ name: 'content-type', value: 'application/json' }], '{}')).toBe(
			'json'
		);
		expect(detectLanguageFromHeaders([{ name: 'content-type', value: 'text/html' }], '<html>')).toBe(
			'html'
		);
		expect(detectLanguageFromHeaders([], '{"a":1}')).toBe('json');
		expect(detectLanguageFromHeaders([], '<root/>')).toBe('xml');
	});

	it('reads every HTML-flavoured content type as HTML', () => {
		for (const type of [
			'text/html; charset=utf-8',
			'application/xhtml+xml',
			'application/xhtml',
			'text/x-html'
		]) {
			expect(detectLanguageFromContentType(type)).toBe('html');
		}
		// XHTML never falls through to the XML test, and markup that is not HTML does.
		expect(detectLanguageFromContentType('application/atom+xml')).toBe('xml');
	});

	it('sniffs an HTML document from its tag or its doctype', () => {
		expect(detectLanguageFromHeaders([], '<!DOCTYPE html><html><body>hi</body></html>')).toBe(
			'html'
		);
		expect(detectLanguageFromHeaders([], '<!doctype html>')).toBe('html');
		expect(detectLanguageFromHeaders([], '<!DOCTYPE html>')).toBe('html');
		expect(detectLanguageFromHeaders([], '<soap:Envelope/>')).toBe('xml');
	});

	it('builds a raw request preview with default headers', () => {
		const raw = buildRawRequestPreview('GET', 'https://example.com/test?x=1', [], '', 'json');
		expect(raw.startsWith('GET /test?x=1 HTTP/1.1\r\n')).toBe(true);
		expect(raw).toContain('host: example.com');
		expect(raw).toContain('user-agent: WebDevUsefulTools Client');
	});

	it('adds default content headers for a request body', () => {
		const raw = buildRawRequestPreview('POST', 'https://example.com/api', [], '{"a":1}', 'json');
		expect(raw.startsWith('POST /api HTTP/1.1\r\n')).toBe(true);
		expect(raw).toContain('content-type: application/json');
		expect(raw).toContain('content-length: 7');
		expect(raw.endsWith('\r\n\r\n{"a":1}')).toBe(true);
	});

	it('frames the raw response dump like the wire format', () => {
		expect(
			buildRawResponse(
				'HTTP/1.1 200 OK',
				[
					{ name: 'Date', value: 'Mon, 23 Nov 2026 09:00:00 GMT' },
					{ name: 'Content-Type', value: 'application/json' },
					{ name: 'Content-Length', value: '11' }
				],
				'{"ok":true}'
			)
		).toBe(
			'HTTP/1.1 200 OK\r\nDate: Mon, 23 Nov 2026 09:00:00 GMT\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{"ok":true}'
		);
	});

	it('shows the response body as it arrived, not the formatted mirror', () => {
		expect(buildRawResponse('HTTP/1.1 200 OK', [], '{"a":1,\n"b":2}')).toBe(
			'HTTP/1.1 200 OK\r\n\r\n{"a":1,\n"b":2}'
		);
	});

	it('drops a nameless header row and a status line the store never kept', () => {
		// A database written before the status line was persisted rebuilds without one.
		expect(buildRawResponse('  ', [{ name: ' ', value: 'x' }, { name: 'a', value: '1' }], 'body')).toBe(
			'a: 1\r\n\r\nbody'
		);
		expect(buildRawResponse('', [], 'body')).toBe('body');
		expect(buildRawResponse('', [], '')).toBe('');
	});
});

describe('header and parameter validation', () => {
	it('accepts RFC 9110 token names and rejects separators', () => {
		expect(validateHeaderName('X-Custom-Header_1')).toBeNull();
		expect(validateHeaderName('Content-Type')).toBeNull();
		expect(validateHeaderName('')).toBeNull();
		expect(validateHeaderName('Con tent')).toMatchObject({ kind: 'header-name', char: '␠' });
		expect(validateHeaderName('X-Forwarded::For')).toMatchObject({ kind: 'header-name', char: ':' });
		expect(validateHeaderName('Host\nName')).toMatchObject({ kind: 'header-name', char: '␊' });
	});

	it('rejects CR, LF and control characters in header values and keeps HTAB', () => {
		expect(validateHeaderValue('application/json; charset=utf-8')).toBeNull();
		expect(validateHeaderValue('a\tb')).toBeNull();
		expect(validateHeaderValue('a\r\nX-Injected: 1')).toMatchObject({
			kind: 'header-value',
			char: '␍'
		});
		expect(validateHeaderValue('a\u0000b')).toMatchObject({ kind: 'header-value', char: 'U+0000' });
	});

	it('describes the offending character for the message', () => {
		expect(describeCharacter('\r')).toBe('␍');
		expect(describeCharacter(' ')).toBe('␠');
		expect(describeCharacter('\u001f')).toBe('U+001F');
		expect(describeCharacter('x')).toBe('x');
	});

	it('checks cookie names and values against RFC 6265', () => {
		expect(validateCookieName('sid')).toBeNull();
		expect(validateCookieName('sid one')).toMatchObject({ kind: 'cookie-name', char: '␠' });
		expect(validateCookieValue('a=b%20c')).toBeNull();
		expect(validateCookieValue('a,b')).toMatchObject({ kind: 'cookie-value', char: ',' });
		expect(validateCookieValue('a b')).toMatchObject({ kind: 'cookie-value', char: '␠' });
		expect(validateCookieValue('a"b')).toMatchObject({ kind: 'cookie-value', char: '"' });
		expect(
			validateProperties('cookie', parseCookies([{ name: 'Cookie', value: 'my cookie=1' }]))
		).toEqual([expect.objectContaining({ kind: 'cookie-name', row: 0 })]);
	});

	it('validates the raw query string, not the decoded rows', () => {
		expect(validateQueryUrl('https://example.com/?a=1&b=2')).toEqual([]);
		expect(validateQueryUrl('https://example.com/?q=hello%20world')).toEqual([]);
		expect(validateQueryUrl('https://example.com/?q=hello world')).toEqual([
			{ kind: 'query-param', field: 'value', name: 'q', char: '␠', row: 0 }
		]);
		expect(validateQueryUrl('https://example.com/?a b=1')).toEqual([
			{ kind: 'query-param', field: 'name', name: 'a b', char: '␠', row: 0 }
		]);
		expect(validateQueryUrl('https://example.com/?a=1&b=%zz')).toEqual([
			{ kind: 'query-param', field: 'value', name: 'b', char: '%', row: 1 }
		]);
		expect(validateQueryUrl('https://example.com/?a=1#frag ment')).toEqual([]);
	});

	it('only rejects form fields that cannot survive the wire', () => {
		expect(validatePropertyField('form', 'name', 'user name')).toBeNull();
		expect(validatePropertyField('form', 'value', 'a b & c')).toBeNull();
		expect(validatePropertyField('form', 'value', 'a\nb')).toMatchObject({
			kind: 'form-param',
			char: '␊'
		});
		expect(validatePropertyField('query', 'name', 'a b')).toMatchObject({
			kind: 'query-param',
			char: '␠'
		});
	});

	it('collects every problem that blocks a request', () => {
		const issues = validateRequest({
			url: 'https://example.com/?a b=1',
			headers: [
				{ name: 'Content Type', value: 'ok' },
				{ name: 'Cookie', value: 'sid=1; other=2 3' }
			],
			form: [{ name: 'x', value: 'a\nb' }],
			body_type: 'json'
		});
		expect(issues.map((issue) => issue.kind)).toEqual([
			'header-name',
			'cookie-value',
			'query-param'
		]);
	});

	it('validates form rows only when the Form-encode tab is active', () => {
		const request = {
			url: 'https://example.com/',
			headers: [],
			form: [{ name: 'x', value: 'a\nb' }],
			body_type: 'json' as const
		};
		expect(validateRequest(request)).toEqual([]);
		expect(validateRequest({ ...request, body_type: 'form' })).toEqual([
			{ kind: 'form-param', field: 'value', name: 'x', char: '␊', row: 0 }
		]);
	});
});

describe('required parameter names', () => {
	it('flags a value that has no name and ignores a fully empty row', () => {
		expect(validateProperties('header', [{ name: '', value: 'application/json' }])).toEqual([
			{ kind: 'name-required', field: 'name', name: '', char: '', row: 0 }
		]);
		expect(validateProperties('form', [{ name: '   ', value: 'x' }])).toEqual([
			{ kind: 'name-required', field: 'name', name: '', char: '', row: 0 }
		]);
		expect(validateProperties('header', [{ name: '', value: '' }])).toEqual([]);
		// A name with an empty value is a legal header field.
		expect(validateProperties('header', [{ name: 'X-Note', value: '' }])).toEqual([]);
		// Cell-level checks stay lenient so the add-row draft is never flagged.
		expect(validatePropertyField('header', 'name', '')).toBeNull();
	});

	it('keeps a value when the query name is cleared and reports it', () => {
		expect(buildQuery([{ name: '', value: '1' }])).toBe('=1');
		expect(buildQuery([{ name: '', value: '  ' }, { name: '', value: '' }])).toBe('');
		expect(updateUrlQuery('https://example.com/a?x=1', [{ name: '', value: '1' }])).toBe(
			'https://example.com/a?=1'
		);
		expect(validateQueryUrl('https://example.com/a?=1')).toEqual([
			{ kind: 'name-required', field: 'name', name: '', char: '', row: 0 }
		]);
		// A bare flag with no `=` is a legal name-less value pair, not a missing name.
		expect(validateQueryUrl('https://example.com/a?debug')).toEqual([]);
		expect(validateQueryUrl('https://example.com/a?=')).toEqual([]);
	});

	it('keeps a value when the cookie name is cleared and reports it', () => {
		expect(buildCookieHeader([{ name: '', value: 'abc' }])).toBe('=abc');
		expect(buildCookieHeader([{ name: '', value: '' }])).toBe('');
		expect(parseCookies([{ name: 'Cookie', value: 'aid=1; =2' }])).toEqual([
			{ name: 'aid', value: '1' },
			{ name: '', value: '2' }
		]);
		expect(validateProperties('cookie', parseCookies([{ name: 'Cookie', value: 'aid=1; =2' }]))).toEqual(
			[{ kind: 'name-required', field: 'name', name: '', char: '', row: 1 }]
		);
		expect(setCookieRows([{ name: 'Accept', value: '*/*' }], [{ name: '', value: 'abc' }])).toEqual([
			{ name: 'Accept', value: '*/*' },
			{ name: 'Cookie', value: '=abc' }
		]);
	});

	it('blocks the request when a name is missing', () => {
		const base = {
			url: 'https://example.com/',
			headers: [{ name: '', value: 'application/json' }],
			form: [],
			body_type: 'json' as const
		};
		expect(validateRequest(base).map((issue) => issue.kind)).toEqual(['name-required']);
		expect(
			validateRequest({ ...base, headers: [], url: 'https://example.com/?=1' }).map(
				(issue) => issue.kind
			)
		).toEqual(['name-required']);
		expect(
			validateRequest({
				...base,
				headers: [],
				form: [{ name: '', value: 'x' }],
				body_type: 'form'
			}).map((issue) => issue.kind)
		).toEqual(['name-required']);
	});
});

/** A `multipart/form-data` payload as a browser export writes it into `--data-raw`. */
function multipartDump(boundary: string): string {
	return [
		`--${boundary}\r\nContent-Disposition: form-data; name="action"\r\n\r\nsave`,
		`--${boundary}\r\nContent-Disposition: form-data; name="code"\r\n\r\n`,
		`--${boundary}\r\nContent-Disposition: form-data; name="note"\r\n\r\nНК Роснефть`,
		`--${boundary}\r\nContent-Disposition: form-data; name="doc"; filename="отчёт.pdf"\r\nContent-Type: application/pdf\r\n\r\n%PDF-1.4`,
		`--${boundary}--\r\n`
	].join('\r\n');
}

describe('multipart form data', () => {
	it('ignores the text buffer while the Form tab is active', () => {
		expect(
			resolveRequestBody({
				body: '{"keep":1}',
				body_type: 'multipart',
				form: [],
				multipart: [{ name: 'note', value: 'hi' }],
				files: []
			})
		).toBe('');
		// Form-encode rows still win over the buffer on their own tab.
		expect(
			resolveRequestBody({ body: '{"keep":1}', body_type: 'form', form: [{ name: 'q', value: 'a b' }] })
		).toBe('q=a+b');
	});

	it('requires RFC 9110 token names and writes values verbatim', () => {
		expect(validatePropertyField('multipart', 'name', 'upload_file')).toBeNull();
		expect(validatePropertyField('multipart', 'name', 'bad name')?.kind).toBe('multipart-field-name');
		// A field value is written raw into the body, so it is not percent-encoded.
		expect(validatePropertyField('multipart', 'value', 'a b&c=d')).toBeNull();
		expect(validatePropertyField('multipart', 'value', 'a\rb')?.kind).toBe('multipart-field-value');
		expect(validateProperties('multipart', [{ name: '', value: 'x' }])).toEqual([
			{ kind: 'name-required', field: 'name', name: '', char: '', row: 0 }
		]);
	});

	it('requires a part name and a readable file for every file row', () => {
		expect(validateFileRow({ name: 'doc', path: '/tmp/a.pdf', filename: 'a.pdf' }, 0)).toEqual([]);
		expect(validateFileRow({ name: 'bad name', path: '/tmp/a.pdf', filename: 'a.pdf' }, 2)).toEqual([
			{ kind: 'multipart-file-name', field: 'name', name: 'bad name', char: '␠', row: 2 }
		]);
		expect(validateFileRow({ name: '', path: '/tmp/a.pdf', filename: 'a.pdf' }, 1)).toEqual([
			{ kind: 'name-required', field: 'name', name: '', char: '', row: 1 }
		]);
		// A persisted row has a filename left but no bytes: it must not be sent silently.
		expect(validateFileRow({ name: 'doc', path: '', filename: 'a.pdf' }, 3)).toEqual([
			{ kind: 'file-missing', field: 'value', name: 'doc', char: '', row: 3 }
		]);
		// The browser build holds the picked File in memory instead of a path.
		expect(
			validateFileRow({ name: 'doc', path: '', filename: 'a.pdf', file: new File(['x'], 'a.pdf') }, 0)
		).toEqual([]);
		expect(validateFileRow({ name: '', path: '', filename: '' }, 0)).toEqual([]);
	});

	it('checks the Form tables only when the Form tab is selected', () => {
		const source = {
			url: 'https://example.com/upload',
			headers: [],
			form: [],
			multipart: [{ name: 'bad name', value: 'v' }],
			files: [{ name: 'doc', path: '', filename: 'gone.txt' }]
		};
		expect(validateRequest({ ...source, body_type: 'json' as const })).toEqual([]);
		expect(validateRequest({ ...source, body_type: 'multipart' as const }).map((i) => i.kind)).toEqual([
			'multipart-field-name',
			'file-missing'
		]);
	});

	it('lays out the body preview with the boundary, a marker and no invented closers', () => {
		const preview = buildMultipartPreview({
			boundary: 'B-1',
			fields: [
				{ name: 'note', value: 'hello' },
				{ name: '   ', value: 'dropped' }
			],
			files: [{ name: 'doc', path: '/tmp/报告.pdf', filename: '报告.pdf' }]
		});
		expect(preview).toBe(
			[
				'--B-1',
				'content-disposition: form-data; name="note"',
				'',
				'hello',
				'--B-1',
				"content-disposition: form-data; name=\"doc\"; filename=\"报告.pdf\"; filename*=UTF-8''%E6%8A%A5%E5%91%8A.pdf",
				'content-type: application/pdf',
				'',
				'<bytes of /tmp/报告.pdf>',
				'--B-1--',
				''
			].join('\r\n')
		);
		// A name-less part is dropped exactly as the encoders drop it.
		expect(preview).not.toContain('dropped');
	});

	it('replaces a hand-typed content type in the raw preview', () => {
		const dump = buildRawRequestPreview(
			'POST',
			'https://example.com/upload',
			[{ name: 'Content-Type', value: 'text/plain' }],
			'{"ignored":1}',
			'multipart',
			{ boundary: 'B-1', fields: [{ name: 'note', value: 'hi' }], files: [] }
		);
		expect(dump).toContain('content-type: multipart/form-data; boundary=B-1');
		expect(dump).not.toContain('text/plain');
		expect(dump).not.toContain('{"ignored":1}');
		// The body itself contains \r\n\r\n, so cut at the first header separator only.
		const split = dump.indexOf('\r\n\r\n');
		const body = dump.slice(split + 4);
		expect(dump).toContain(`content-length: ${byteLength(body)}`);
		expect(body).toBe('--B-1\r\ncontent-disposition: form-data; name="note"\r\n\r\nhi\r\n--B-1--\r\n');
	});

	it('exports --form parts and never a body that is not on the wire', () => {
		const command = buildCurl(
			{
				method: 'POST',
				url: 'https://example.com/upload',
				headers: [{ name: 'Content-Type', value: 'multipart/form-data; boundary=stale' }],
				body: '{"ignored":1}',
				body_type: 'multipart',
				multipart: [
					{ name: 'note', value: 'a b' },
					{ name: '   ', value: 'drop' }
				],
				files: [
					{ name: 'doc', path: '/tmp/a.pdf', filename: 'alias.pdf' },
					// No path means no bytes to export; validation blocks the send first.
					{ name: 'gone', path: '', filename: 'gone.txt' }
				]
			},
			'posix'
		);
		expect(command).toContain("--form 'note=a b'");
		expect(command).toContain("--form 'doc=@/tmp/a.pdf;filename=alias.pdf'");
		expect(command).not.toContain('--data-raw');
		expect(command).not.toContain('Content-Type');
		expect(command).not.toContain('gone');
		expect(command).not.toContain('{"ignored":1}');

		const windows = buildCurl(
			{
				method: 'POST',
				url: 'https://example.com/upload',
				headers: [],
				body: '',
				body_type: 'multipart',
				multipart: [],
				files: [{ name: 'doc', path: 'C:/tmp/a.pdf', filename: 'a.pdf' }]
			},
			'windows'
		);
		expect(windows).toContain('--form "doc=@C:/tmp/a.pdf"');
		// The alias equals the file name here, so it is not repeated.
		expect(windows).not.toContain(';filename=');
	});

	it('imports --form, -F and --form-string and drops the foreign boundary', () => {
		const parsed = parseCurl(
			"curl -X POST https://example.com/upload -H 'content-type: multipart/form-data; boundary=abc' --form 'note=hi there' -F 'doc=@/tmp/a.pdf;filename=alias.pdf;type=application/pdf' --form-string 'lit=@not-a-path'"
		);
		expect(parsed?.bodyType).toBe('multipart');
		expect(parsed?.body).toBe('');
		expect(parsed?.multipart).toEqual([
			{ name: 'note', value: 'hi there' },
			{ name: 'lit', value: '@not-a-path' }
		]);
		expect(parsed?.files).toEqual([{ name: 'doc', path: '/tmp/a.pdf', filename: 'alias.pdf' }]);
		// curl generates the boundary itself, so an imported one is noise in the table.
		expect(parsed?.headers).toEqual([]);

		// `-d` next to `-F` is another part of the same body.
		expect(parseCurl("curl -F 'a=1' -d 'b=2' https://example.com/u")?.multipart).toEqual([
			{ name: 'a', value: '1' },
			{ name: 'b', value: '2' }
		]);
	});

	it('imports a multipart body a browser export wrote into --data-raw', () => {
		const boundary = '----WebKitFormBoundaryabc';
		// "Copy as cURL (bash)" keeps the payload in `$'…'`, so every break is an escape.
		const escaped = multipartDump(boundary)
			.replace(/\\/g, '\\\\')
			.replace(/'/g, "\\'")
			.replace(/\r/g, '\\r')
			.replace(/\n/g, '\\n');
		const parsed = parseCurl(
			`curl --url 'https://example.com/register' \\\n` +
				`  -H 'content-type: multipart/form-data; boundary=${boundary}' \\\n` +
				`  --data-raw $'${escaped}'`
		);
		expect(parsed?.method).toBe('POST');
		expect(parsed?.bodyType).toBe('multipart');
		// The tables own the body, so the text buffer stays empty.
		expect(parsed?.body).toBe('');
		expect(parsed?.multipart).toEqual([
			{ name: 'action', value: 'save' },
			{ name: 'code', value: '' },
			{ name: 'note', value: 'НК Роснефть' }
		]);
		// A part with a filename cannot carry its bytes through a command line: the row
		// keeps the name and stays blocked until the user picks the file.
		expect(parsed?.files).toEqual([{ name: 'doc', path: '', filename: 'отчёт.pdf' }]);
		expect(validateFileRows(parsed?.files ?? [])).toHaveLength(1);
		// The imported boundary belongs to the exporter, not to this client.
		expect(parsed?.headers).toEqual([]);
	});

	it('reads the boundary out of the body when the command has no Content-Type', () => {
		const boundary = '----WebKitFormBoundaryabc';
		const parsed = parseCurl(
			`curl 'https://example.com/register' --data-raw '${multipartDump(boundary)}'`
		);
		expect(parsed?.bodyType).toBe('multipart');
		expect(parsed?.multipart).toEqual([
			{ name: 'action', value: 'save' },
			{ name: 'code', value: '' },
			{ name: 'note', value: 'НК Роснефть' }
		]);
	});

	it('unescapes a caret-escaped cmd.exe command copied from a browser', () => {
		const boundary = '----WebKitFormBoundaryabc';
		// cmd.exe flavor: every quote becomes `^\^"`, every `%` is doubled, a break is
		// escaped as `^` + CR, and the command lines continue with ` ^`.
		const cmdBody = multipartDump(boundary)
			.replace(/\r/g, '^\r')
			.replace(/"/g, '^\\^"')
			.replace(/%/g, '%%');
		const parsed = parseCurl(
			`curl --url ^"https://example.com/register^" ^\r\n` +
				`  -H ^"cache-control: no-cache^" ^\r\n` +
				`  -H ^"content-type: multipart/form-data; boundary=${boundary}^" ^\r\n` +
				`  -H ^"sec-ch-ua: ^\\^"Google Chrome^\\^";v=^\\^"159^\\^"^" ^\r\n` +
				`  -b ^"acs=100%%^|^;sid=^$9^" ^\r\n` +
				`  --data-raw ^"${cmdBody}^" ^\r\n`
		);
		expect(parsed?.url).toBe('https://example.com/register');
		expect(parsed?.headers).toEqual([
			{ name: 'cache-control', value: 'no-cache' },
			{ name: 'sec-ch-ua', value: '"Google Chrome";v="159"' },
			{ name: 'Cookie', value: 'acs=100%|;sid=$9' }
		]);
		expect(parsed?.bodyType).toBe('multipart');
		expect(parsed?.body).toBe('');
		expect(parsed?.multipart).toEqual([
			{ name: 'action', value: 'save' },
			{ name: 'code', value: '' },
			{ name: 'note', value: 'НК Роснефть' }
		]);
		expect(parsed?.files).toEqual([{ name: 'doc', path: '', filename: 'отчёт.pdf' }]);
	});

	it('keeps imported field values free of the continuation framing', () => {
		const boundary = '----WebKitFormBoundaryabc';
		// A browser that wraps the payload writes each payload break the way cmd.exe joins
		// argument lines: `^` + break + the joiner's two-space indent. Those characters
		// frame the dump rather than belong to a part, so a leftover CR would paint every
		// value cell of the Fields table as invalid.
		const wrapped = multipartDump(boundary)
			.replace(/\r/g, '^\r')
			.replace(/\n/g, '^\n  ')
			.replace(/"/g, '^\\^"');
		const parsed = parseCurl(
			`curl --url ^"https://example.com/register^" ^\r\n` +
				`  -H ^"content-type: multipart/form-data; boundary=${boundary}^" ^\r\n` +
				`  --data-raw ^"${wrapped}^"`
		);
		expect(parsed?.bodyType).toBe('multipart');
		expect(parsed?.multipart).toEqual([
			{ name: 'action', value: 'save' },
			{ name: 'code', value: '' },
			{ name: 'note', value: 'НК Роснефть' }
		]);
		// Nothing on the Fields table is reported: the import produced clean values.
		expect(validateProperties('multipart', parsed?.multipart ?? [])).toEqual([]);
	});

	it('imports the cmd.exe payload Chrome writes after an empty --data-raw', () => {
		const boundary = '----MultipartFormBoundaryByMQ6AY1ZBj74yDf';
		// Chrome's cmd.exe export gives up on a multipart body: it passes an empty
		// argument and then writes the dump as continuation lines, each closed by the
		// caret that joins the next one and indented like the arguments.
		const command = [
			'curl --url ^"https://www.tez-tour.com/agencyRegistration.html^" ^',
			'  --compressed ^',
			'  -H ^"cache-control: no-cache^" ^',
			`  -H ^"content-type: multipart/form-data; boundary=${boundary}^" ^`,
			'  -H ^"sec-ch-ua-platform: ^"Windows^"^" ^',
			'  -b ^"PHPSESSID=ok0q%%^|^;userAuth=;^" ^',
			'  --data-raw "^" ^',
			`  --${boundary} ^`,
			'  Content-Disposition: form-data; name="firstName" ^',
			'  ^',
			'  Ekaterinburg ^',
			`  --${boundary} ^`,
			'  Content-Disposition: form-data; name="attachment"; filename=^"report.txt^" ^',
			'  Content-Type: text/plain ^',
			'  ^',
			'  ^',
			`  --${boundary}--`
		].join('\r\n');

		const parsed = parseCurl(command);
		expect(parsed?.url).toBe('https://www.tez-tour.com/agencyRegistration.html');
		expect(parsed?.method).toBe('POST');
		expect(parsed?.bodyType).toBe('multipart');
		expect(parsed?.body).toBe('');
		expect(parsed?.headers).toEqual([
			{ name: 'cache-control', value: 'no-cache' },
			{ name: 'sec-ch-ua-platform', value: 'Windows' },
			{ name: 'Cookie', value: 'PHPSESSID=ok0q%|;userAuth=;' }
		]);
		expect(parsed?.multipart).toEqual([{ name: 'firstName', value: 'Ekaterinburg' }]);
		expect(parsed?.files).toEqual([{ name: 'attachment', path: '', filename: 'report.txt' }]);
		// The Fields table is clean; only the file row reports itself, because the
		// clipboard can carry a file name but never the bytes behind it.
		expect(validateProperties('multipart', parsed?.multipart ?? [])).toEqual([]);
		expect(validateFileRows(parsed?.files ?? []).map((issue) => issue.kind)).toEqual([
			'file-missing'
		]);
		// The other spelling of the placeholder argument carries the same dump.
		expect(parseCurl(command.replace('--data-raw "^"', '--data-raw ^"^"'))?.multipart).toEqual([
			{ name: 'firstName', value: 'Ekaterinburg' }
		]);
	});

	it('keeps a payload break inside an imported field value', () => {
		const boundary = '--abc';
		const parsed = parseCurl(
			`curl 'https://example.com/u' --data-raw '--abc\r\nContent-Disposition: form-data; name="msg"\r\n\r\nline one\r\nline two\r\n--abc--\r\n'`
		);
		expect(parsed?.multipart).toEqual([{ name: 'msg', value: 'line one\r\nline two' }]);
	});

	it('keeps a body that merely mentions form-data as text', () => {
		const parsed = parseCurl(
			`curl 'https://example.com/log' --data-raw 'Content-Disposition: form-data; name="a"'`
		);
		expect(parsed?.bodyType).toBe('text');
		expect(parsed?.multipart).toEqual([]);
		expect(parsed?.body).toBe('Content-Disposition: form-data; name="a"');
	});

	it('round trips an exported multipart command back into the tables', () => {
		const command = buildCurl(
			{
				method: 'POST',
				url: 'https://example.com/upload',
				headers: [{ name: 'X-Trace', value: 'one' }],
				body: '',
				body_type: 'multipart',
				multipart: [{ name: 'note', value: "with ' quote" }],
				files: [{ name: 'doc', path: '/tmp/a.pdf', filename: 'alias.pdf' }]
			},
			'posix'
		);
		const parsed = parseCurl(command);
		expect(parsed?.multipart).toEqual([{ name: 'note', value: "with ' quote" }]);
		expect(parsed?.files).toEqual([{ name: 'doc', path: '/tmp/a.pdf', filename: 'alias.pdf' }]);
		expect(parsed?.headers).toEqual([{ name: 'X-Trace', value: 'one' }]);
	});

	it('builds a browser body from the picked files', () => {
		const form = buildMultipartFormData(
			[
				{ name: 'note', value: 'hi' },
				{ name: '   ', value: 'drop' }
			],
			[{ name: 'doc', path: '', filename: 'alias.txt', file: new File(['x'], 'live.txt') }]
		);
		expect(form?.get('note')).toBe('hi');
		expect(form?.get('   ')).toBeNull();
		expect((form?.get('doc') as File | undefined)?.name).toBe('alias.txt');
		// Without the in-memory File there is nothing to append, so the caller reports it.
		expect(buildMultipartFormData([], [{ name: 'doc', path: '/tmp/a.txt', filename: 'a.txt' }])).toBeNull();
	});
});

describe('curl flag aliases', () => {
	it('expands cookie, user-agent, referer, credentials and --url flags', () => {
		const parsed = parseCurl(
			`curl -b 'sid=1&lang=ru' -A 'Mozilla/9' -e 'https://ref/' -u 'al:ice' --url 'https://api.example/x'`
		);
		expect(parsed?.headers).toEqual([
			{ name: 'Cookie', value: 'sid=1&lang=ru' },
			{ name: 'User-Agent', value: 'Mozilla/9' },
			{ name: 'Referer', value: 'https://ref/' },
			{ name: 'Authorization', value: `Basic ${btoa('al:ice')}` }
		]);
		expect(parsed?.url).toBe('https://api.example/x');
		expect(parsed?.method).toBe('GET');
		expect(parsed?.bodyType).toBe('text');
		expect(parsed?.body).toBe('');
	});

	it('keeps a bare cookie flag and skips credentials that btoa cannot encode', () => {
		const parsed = parseCurl(`curl -b debug -u 'ключ:1' https://x/`);
		expect(parsed?.headers).toEqual([]);
		expect(parsed?.url).toBe('https://x/');
	});

	it('percent-encodes --data-urlencode arguments into one body', () => {
		const parsed = parseCurl(`curl --data-urlencode 'q=hello world' --data-urlencode 'plain value' https://x/`);
		expect(parsed?.body).toBe('q=hello%20world&plain%20value');
		expect(parsed?.bodyType).toBe('text');
		expect(parsed?.method).toBe('POST');
		expect(parsed?.form).toEqual([]);
	});

	it('decodes short ANSI-C hex and octal escapes typed by hand', () => {
		// Bash takes one to N digits, so `\u49` is `I` and `\114` is `L`.
		const parsed = parseCurl(`curl -H $'\\x48:1' -H $'\\u49:2' -H $'\\114:3' https://x/`);
		expect(parsed?.headers).toEqual([
			{ name: 'H', value: '1' },
			{ name: 'I', value: '2' },
			{ name: 'L', value: '3' }
		]);
		expect(parsed?.method).toBe('GET');
	});
});

describe('content type and header lookup', () => {
	it('reads a header by its trimmed, case-insensitive name', () => {
		expect(getHeader([{ name: ' Content-Type ', value: 'application/json' }], 'CONTENT-TYPE')).toBe('application/json');
	});

	it('falls back to text for an unknown content type', () => {
		expect(bodyTypeFromContentType('')).toBeNull();
		expect(bodyTypeFromContentType('application/pdf')).toBe('text');
		expect(bodyTypeFromContentType('TEXT/HTML; charset=ascii')).toBe('text');
		expect(bodyTypeFromContentType('multipart/form-data; boundary=x')).toBe('multipart');
	});

	it('names the default content type of every body kind', () => {
		expect(defaultContentType('json')).toBe('application/json');
		expect(defaultContentType('xml')).toBe('application/xml');
		expect(defaultContentType('form')).toBe('application/x-www-form-urlencoded');
		expect(defaultContentType('text')).toBe('text/plain');
		expect(defaultContentType('multipart')).toBe(`multipart/form-data; boundary=${MULTIPART_BOUNDARY_PLACEHOLDER}`);
		expect(defaultContentType('multipart', 'abc')).toBe('multipart/form-data; boundary=abc');
	});

	it('produces a version-4 UUID so a dump never loses the boundary shape', () => {
		expect(multipartBoundary()).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
	});
});

describe('multipart file rows', () => {
	it('sniffs the part type from the path, the way the desktop encoder does', () => {
		const preview = buildMultipartPreview({
			boundary: 'B',
			fields: [],
			files: [{ name: 'doc', path: '/tmp/a.pdf', filename: 'report.txt' }]
		});
		expect(preview).toContain('filename="report.txt"');
		expect(preview).toContain('content-type: application/pdf');
		expect(preview).toContain('<bytes of /tmp/a.pdf>');
	});

	it('leaves an alias-less row with no path without a display name', () => {
		const preview = buildMultipartPreview({ boundary: 'B', fields: [], files: [{ name: 'doc', path: '', filename: '' }] });
		expect(preview).toContain('filename=""');
		expect(preview).toContain('content-type: application/octet-stream');
		expect(preview).toContain('<bytes of , not loaded>');
	});

	it('derives a display name and a type from a Windows path without an alias', () => {
		expect(basenameOf('C:\\temp\\report.pdf')).toBe('report.pdf');
		expect(basenameOf('/tmp/dir/')).toBe('dir');
		expect(contentTypeForFilename('C:\\temp\\report.PDF')).toBe('application/pdf');
		expect(contentTypeForFilename('notes')).toBe('application/octet-stream');
		const preview = buildMultipartPreview({
			boundary: 'B',
			fields: [],
			files: [{ name: 'doc', path: 'C:\\temp\\report.pdf', filename: '' }]
		});
		expect(preview).toContain('filename="report.pdf"');
		expect(preview).toContain('content-type: application/pdf');
	});
});

describe('multipart validation entry points', () => {
	it('reports a value without a name as a missing name, and never an empty row', () => {
		expect(validateRowName({ name: '', value: 'x' })).toEqual({
			kind: 'name-required',
			field: 'name',
			name: '',
			char: ''
		});
		expect(validateRowName({ name: '', value: '' })).toBeNull();
		expect(validateRowName({ name: 'ok', value: '' })).toBeNull();
	});

	it('marks a persisted file row whose bytes were never stored', () => {
		expect(validateFileRows([{ name: 'doc', path: '', filename: 'a.txt' }])).toEqual([
			{ kind: 'file-missing', field: 'value', name: 'doc', char: '', row: 0 }
		]);
		expect(validateFileRows([])).toEqual([]);
	});
});

describe('content-disposition file name', () => {
	it('prefers the RFC 5987 form and percent-decodes it', () => {
		expect(
			filenameFromDisposition(`attachment; filename="a.txt"; filename*=UTF-8''%D0%BE%D1%82%D0%B2%D0%B5%D1%82.txt`)
		).toBe('ответ.txt');
		expect(filenameFromDisposition('attachment; filename=a.txt')).toBe('a.txt');
		expect(filenameFromDisposition('inline')).toBeNull();
	});

	it('keeps a broken escape as its own text rather than dropping the name', () => {
		expect(filenameFromDisposition("attachment; filename*=UTF-8''%zz.txt")).toBe('%zz.txt');
	});

	it('answers null for an empty parameter so the caller picks its own name', () => {
		expect(filenameFromDisposition('attachment; filename=""')).toBeNull();
		expect(filenameFromDisposition('attachment; filename*=UTF-8\'\'')).toBeNull();
		expect(filenameFromDisposition('')).toBeNull();
	});

	it('keeps a semicolon inside a quoted name instead of ending at it', () => {
		expect(filenameFromDisposition('attachment; filename="a;b.txt"')).toBe('a;b.txt');
		expect(filenameFromDisposition("attachment; filename*=UTF-8''a%3Bb.txt")).toBe('a;b.txt');
	});

	it('does not fall back to the plain name when the extended one is broken', () => {
		expect(
			filenameFromDisposition('attachment; filename="fallback.txt"; filename*=UTF-8\'\'caf%E9.txt')
		).toBe('caf%E9.txt');
	});
});

describe('attachment disposition', () => {
	it('reads only the disposition type', () => {
		expect(isAttachmentDisposition('attachment')).toBe(true);
		expect(isAttachmentDisposition('ATTACHMENT; filename="report.csv"')).toBe(true);
		// A filename that happens to contain the word is not the disposition type.
		expect(isAttachmentDisposition('inline; filename="attachment.pdf"')).toBe(false);
		expect(isAttachmentDisposition('')).toBe(false);
	});
});
