import { describe, expect, it } from 'vitest';
import {
	buildCookieHeader,
	buildCurl,
	buildQuery,
	buildRawRequestPreview,
	detectLanguageFromHeaders,
	encodeForm,
	formatRestSize,
	formatRestTime,
	parseCookies,
	parseCurl,
	parseForm,
	parseQuery,
	resolveRequestBody,
	setCookieRows,
	updateUrlQuery
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
});
