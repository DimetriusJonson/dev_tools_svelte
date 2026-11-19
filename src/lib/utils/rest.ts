export interface RestProperty {
	name: string;
	value: string;
}

export type BodyType = 'json' | 'xml' | 'text' | 'form';
export type CurlShell = 'posix' | 'windows';

export interface ParsedCurl {
	method: string;
	url: string;
	headers: RestProperty[];
	body: string;
	bodyType: BodyType;
	form: RestProperty[];
}

const DEFAULT_USER_AGENT = 'WebDevUsefulTools Client';

function splitFragment(url: string): { withoutFragment: string; fragment: string } {
	const index = url.indexOf('#');
	if (index < 0) return { withoutFragment: url, fragment: '' };
	return { withoutFragment: url.slice(0, index), fragment: url.slice(index) };
}

export function parseQuery(url: string): RestProperty[] {
	const { withoutFragment } = splitFragment(url);
	const index = withoutFragment.indexOf('?');
	if (index < 0) return [];
	const query = withoutFragment.slice(index + 1);
	if (!query) return [];

	return query
		.split('&')
		.filter((segment) => segment.length > 0)
		.map((segment) => {
			const separator = segment.indexOf('=');
			const rawName = separator < 0 ? segment : segment.slice(0, separator);
			const rawValue = separator < 0 ? '' : segment.slice(separator + 1);
			return { name: decodeQueryPart(rawName), value: decodeQueryPart(rawValue) };
		});
}

function decodeQueryPart(value: string): string {
	try {
		return decodeURIComponent(value.replace(/\+/g, ' '));
	} catch {
		return value;
	}
}

export function buildQuery(rows: RestProperty[]): string {
	return rows
		.filter((row) => row.name.trim().length > 0)
		.map(
			(row) =>
				`${encodeURIComponent(row.name)}=${encodeURIComponent(row.value)}`
		)
		.join('&');
}

export function updateUrlQuery(url: string, rows: RestProperty[]): string {
	const { withoutFragment, fragment } = splitFragment(url);
	const index = withoutFragment.indexOf('?');
	const base = index < 0 ? withoutFragment : withoutFragment.slice(0, index);
	const query = buildQuery(rows);
	return `${base}${query ? `?${query}` : ''}${fragment}`;
}

export function getHeader(headers: RestProperty[], name: string): string {
	const found = headers.find((header) => header.name.trim().toLowerCase() === name.toLowerCase());
	return found?.value ?? '';
}

export function parseCookies(headers: RestProperty[]): RestProperty[] {
	const value = getHeader(headers, 'cookie');
	if (!value.trim()) return [];
	return value
		.split(';')
		.map((segment) => segment.trim())
		.filter(Boolean)
		.map((segment) => {
			const separator = segment.indexOf('=');
			if (separator < 0) return { name: segment, value: '' };
			return {
				name: segment.slice(0, separator).trim(),
				value: segment.slice(separator + 1).trim()
			};
		})
		.filter((row) => row.name.length > 0);
}

export function buildCookieHeader(rows: RestProperty[]): string {
	return rows
		.filter((row) => row.name.trim().length > 0)
		.map((row) => `${row.name.trim()}=${row.value}`)
		.join('; ');
}

export function setCookieRows(headers: RestProperty[], rows: RestProperty[]): RestProperty[] {
	const value = buildCookieHeader(rows);
	const index = headers.findIndex((header) => header.name.trim().toLowerCase() === 'cookie');
	if (!value) {
		return headers.filter((_, itemIndex) => itemIndex !== index);
	}
	const next = [...headers];
	if (index >= 0) {
		next[index] = { name: 'Cookie', value };
	} else {
		next.push({ name: 'Cookie', value });
	}
	return next;
}

export function encodeForm(rows: RestProperty[]): string {
	const params = new URLSearchParams();
	for (const row of rows) {
		if (row.name.trim()) params.append(row.name, row.value);
	}
	return params.toString();
}

export function parseForm(body: string): RestProperty[] {
	if (!body.trim()) return [];
	const params = new URLSearchParams(body);
	return Array.from(params.entries()).map(([name, value]) => ({ name, value }));
}

export interface RestBodySource {
	body: string;
	body_type: BodyType;
	form: RestProperty[];
}

/**
 * Resolves the body that goes on the wire. Form-encode rows are stored in their own
 * table and never mix with the JSON/XML/Text buffer: when the Form-encode tab is
 * active the table wins and the text body is ignored, otherwise the text body is used
 * unchanged.
 */
export function resolveRequestBody(source: RestBodySource): string {
	return source.body_type === 'form' ? encodeForm(source.form) : source.body;
}

export function bodyTypeFromContentType(contentType: string): BodyType | null {
	const value = contentType.toLowerCase();
	if (value.includes('json')) return 'json';
	if (value.includes('xml')) return 'xml';
	if (value.includes('x-www-form-urlencoded')) return 'form';
	if (value.startsWith('text/') || value.length > 0) return 'text';
	return null;
}

export function detectLanguageFromContentType(contentType: string): 'json' | 'xml' | 'html' | 'text' | null {
	const value = contentType.toLowerCase();
	if (!value) return null;
	if (value.includes('html')) return 'html';
	if (value.includes('json')) return 'json';
	if (value.includes('xml')) return 'xml';
	if (
		value.startsWith('text/') ||
		value.includes('javascript') ||
		value.includes('ecmascript') ||
		value.includes('x-www-form-urlencoded') ||
		value.includes('yaml') ||
		value.includes('csv') ||
		value.includes('svg')
	) {
		return 'text';
	}
	return null;
}

export function detectLanguageFromHeaders(
	headers: RestProperty[],
	body: string
): 'json' | 'xml' | 'html' | 'text' | null {
	const fromType = detectLanguageFromContentType(getHeader(headers, 'content-type'));
	if (fromType) return fromType;
	const trimmed = body.trimStart();
	if (!trimmed) return null;
	if (trimmed.startsWith('<')) {
		return trimmed.toLowerCase().includes('<html') ? 'html' : 'xml';
	}
	if (trimmed.startsWith('{') || trimmed.startsWith('[')) return 'json';
	return 'text';
}

export function defaultContentType(bodyType: BodyType): string {
	switch (bodyType) {
		case 'json':
			return 'application/json';
		case 'xml':
			return 'application/xml';
		case 'form':
			return 'application/x-www-form-urlencoded';
		default:
			return 'text/plain';
	}
}

function normalizeUrl(raw: string): URL | null {
	const trimmed = raw.trim();
	if (!trimmed) return null;
	try {
		return new URL(trimmed.includes('://') ? trimmed : `http://${trimmed}`);
	} catch {
		return null;
	}
}

export function buildRawRequestPreview(
	method: string,
	url: string,
	headers: RestProperty[],
	body: string,
	bodyType: BodyType
): string {
	const parsed = normalizeUrl(url);
	if (!parsed) return `${method.toUpperCase()} ${url}`;
	const prepared = headers
		.filter((header) => header.name.trim())
		.map((header) => ({ ...header }));
	const has = (name: string) =>
		prepared.some((header) => header.name.toLowerCase() === name.toLowerCase());

	if (!has('user-agent')) prepared.push({ name: 'user-agent', value: DEFAULT_USER_AGENT });
	if (!has('accept-encoding')) prepared.push({ name: 'accept-encoding', value: 'gzip' });
	if (!has('host')) {
		const port = parsed.port ? `:${parsed.port}` : '';
		prepared.push({ name: 'host', value: `${parsed.hostname}${port}` });
	}
	if (body) {
		if (!has('content-type')) {
			prepared.push({ name: 'content-type', value: defaultContentType(bodyType) });
		}
		if (!has('content-length')) {
			prepared.push({ name: 'content-length', value: String(new TextEncoder().encode(body).length) });
		}
	}

	const path = `${parsed.pathname || '/'}${parsed.search}`;
	const lines = [`${method.toUpperCase()} ${path} HTTP/1.1`];
	for (const header of prepared) lines.push(`${header.name}: ${header.value}`);
	return `${lines.join('\r\n')}\r\n\r\n${body}`;
}

export function formatRestSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} bytes`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KiB`;
	return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}

export function formatRestTime(milliseconds: number): string {
	if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`;
	return `${(milliseconds / 1000).toFixed(2)} sec`;
}

export function byteLength(value: string): number {
	return new TextEncoder().encode(value).length;
}

function tokenizeCurl(input: string): string[] {
	const tokens: string[] = [];
	let current = '';
	let quote: "'" | '"' | null = null;

	for (let index = 0; index < input.length; index += 1) {
		const char = input[index];

		if (quote === "'") {
			if (char === "'") quote = null;
			else current += char;
			continue;
		}

		if (quote === '"') {
			if (char === '"') {
				quote = null;
			} else if (char === '\\' && index + 1 < input.length) {
				index += 1;
				current += input[index];
			} else {
				current += char;
			}
			continue;
		}

		if (char === '"' || char === "'") {
			quote = char;
			continue;
		}
		if (char === '\\' && index + 1 < input.length) {
			index += 1;
			current += input[index];
			continue;
		}
		if (/\s/.test(char)) {
			if (current) {
				tokens.push(current);
				current = '';
			}
			continue;
		}
		current += char;
	}

	if (current) tokens.push(current);
	return tokens;
}

function splitHeader(value: string): RestProperty | null {
	const separator = value.indexOf(':');
	if (separator < 0) return null;
	const name = value.slice(0, separator).trim();
	const headerValue = value.slice(separator + 1).trim();
	return name ? { name, value: headerValue } : null;
}

function sniffBodyType(body: string): BodyType {
	const trimmed = body.trim();
	if (!trimmed) return 'text';
	if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
		try {
			JSON.parse(trimmed);
			return 'json';
		} catch {
			/* keep looking */
		}
	}
	if (trimmed.startsWith('<')) return 'xml';
	return 'text';
}

const CURL_FLAGS_WITH_VALUE = new Set([
	'-o',
	'--output',
	'-m',
	'--max-time',
	'--connect-timeout',
	'-x',
	'--proxy',
	'--cacert',
	'--cert',
	'--key',
	'--retry',
	'--retry-delay',
	'--resolve',
	'--interface',
	'--limit-rate'
]);

const CURL_FLAGS_IGNORED = new Set([
	'-L',
	'--location',
	'-k',
	'--insecure',
	'-s',
	'--silent',
	'-S',
	'--show-error',
	'-v',
	'--verbose',
	'-i',
	'--include',
	'-f',
	'--fail',
	'--compressed',
	'--http1.1',
	'--http2',
	'--http2-prior-knowledge',
	'-g',
	'--globoff'
]);

export function parseCurl(command: string): ParsedCurl | null {
	let normalized = command.replace(/\\\r?\n|\^\r?\n/g, ' ');
	// Windows commands quote arguments either plainly (`"value"`) or with a caret
	// (`^"value^"`), and batch files double `%`. Detect curl.exe so the normalization
	// never touches POSIX commands that keep carets or percent signs literally.
	const firstWord = normalized.trimStart().split(/\s+/)[0] ?? '';
	if (firstWord.toLowerCase().endsWith('curl.exe')) {
		normalized = normalized.replace(/\^"/g, '"').replace(/%%/g, '%');
	}
	const tokens = tokenizeCurl(normalized);
	if (tokens.length === 0) return null;

	const executable = tokens[0].replace(/^.*[/\\]/, '').toLowerCase();
	if (executable !== 'curl' && executable !== 'curl.exe') return null;

	let method = '';
	let url = '';
	const headers: RestProperty[] = [];
	const bodyParts: string[] = [];

	const addHeader = (value: string) => {
		const header = splitHeader(value);
		if (!header) return;
		const existing = headers.findIndex(
			(item) => item.name.toLowerCase() === header.name.toLowerCase()
		);
		if (existing >= 0) headers[existing] = header;
		else headers.push(header);
	};

	for (let index = 1; index < tokens.length; index += 1) {
		const token = tokens[index];

		if (token === '-X' || token === '--request') {
			method = tokens[index + 1] ?? method;
			index += 1;
			continue;
		}
		if (token === '-H' || token === '--header') {
			addHeader(tokens[index + 1] ?? '');
			index += 1;
			continue;
		}
		if (
			token === '-d' ||
			token === '--data' ||
			token === '--data-raw' ||
			token === '--data-binary' ||
			token === '--data-ascii'
		) {
			bodyParts.push(tokens[index + 1] ?? '');
			index += 1;
			continue;
		}
		if (token === '--data-urlencode') {
			const value = tokens[index + 1] ?? '';
			const separator = value.indexOf('=');
			if (separator > 0) {
				const name = value.slice(0, separator);
				const raw = value.slice(separator + 1);
				bodyParts.push(`${encodeURIComponent(name)}=${encodeURIComponent(raw)}`);
			} else {
				bodyParts.push(encodeURIComponent(value));
			}
			index += 1;
			continue;
		}
		if (token === '--url') {
			url = tokens[index + 1] ?? url;
			index += 1;
			continue;
		}
		if (token === '-b' || token === '--cookie') {
			const value = tokens[index + 1] ?? '';
			if (value.includes('=')) addHeader(`Cookie: ${value}`);
			index += 1;
			continue;
		}
		if (token === '-A' || token === '--user-agent') {
			addHeader(`User-Agent: ${tokens[index + 1] ?? ''}`);
			index += 1;
			continue;
		}
		if (token === '-e' || token === '--referer') {
			addHeader(`Referer: ${tokens[index + 1] ?? ''}`);
			index += 1;
			continue;
		}
		if (token === '-u' || token === '--user') {
			const credentials = tokens[index + 1] ?? '';
			if (credentials) {
				try {
					addHeader(`Authorization: Basic ${btoa(credentials)}`);
				} catch {
					/* ignore credentials that cannot be encoded */
				}
			}
			index += 1;
			continue;
		}
		if (CURL_FLAGS_WITH_VALUE.has(token)) {
			index += 1;
			continue;
		}
		if (CURL_FLAGS_IGNORED.has(token)) continue;
		if (token.startsWith('-')) continue;
		if (!url) url = token;
	}

	if (!url.trim()) return null;

	const rawBody = bodyParts.join('&');
	const contentType = getHeader(headers, 'content-type');
	let bodyType = bodyTypeFromContentType(contentType) ?? sniffBodyType(rawBody);
	const form = bodyType === 'form' ? parseForm(rawBody) : [];
	// Form-encode rows live in their own table, so an imported form body does not
	// occupy the JSON/XML/Text buffer.
	const body = bodyType === 'form' ? '' : rawBody;
	if (!contentType && body && bodyType === 'text') {
		bodyType = 'text';
	}

	return {
		method: (method || (rawBody ? 'POST' : 'GET')).toUpperCase(),
		url,
		headers,
		body,
		bodyType,
		form
	};
}

function quotePosix(value: string): string {
	return `'${value.replace(/'/g, `'\\''`)}'`;
}

/**
 * Quotes an argument for cmd.exe. Plain double quotes delimit the argument, and the
 * cmd.exe/CRT argv parser turns `\"` into a literal quote inside them; `^"value^"` is
 * deliberately not used because it does not enter cmd quoting mode, so `&` would split
 * the command. `%` stays single: interactive cmd.exe keeps it literal, which preserves
 * percent-encoded URLs (batch files need manual `%%`, see README). Newlines inside a
 * value would break the command, so they collapse to spaces.
 */
function quoteWindows(value: string): string {
	const inner = value
		.replace(/[\r\n]+/g, ' ')
		.replace(/\\/g, '\\\\')
		.replace(/"/g, '\\"');
	return `"${inner}"`;
}

export function buildCurl(
	request: {
		method: string;
		url: string;
		headers: RestProperty[];
		body: string;
	},
	shell: CurlShell
): string {
	const quote = shell === 'posix' ? quotePosix : quoteWindows;
	const executable = shell === 'posix' ? 'curl' : 'curl.exe';
	// cmd.exe needs ` ^` before every newline so the lines are one command.
	const joiner = shell === 'posix' ? ' ' : ' ^\r\n';
	const methodPart = `--request ${request.method.toUpperCase()}`;
	const headerParts = request.headers
		.filter((header) => header.name.trim())
		.map((header) => `-H ${quote(`${header.name}: ${header.value}`)}`);
	const bodyParts = request.body ? [`--data-raw ${quote(request.body)}`] : [];
	// cmd.exe is picky about the URL token position, so it goes right after curl.exe.
	const parts =
		shell === 'posix'
			? [executable, methodPart, ...headerParts, ...bodyParts, quote(request.url)]
			: [`${executable} ${quote(request.url)}`, methodPart, ...headerParts, ...bodyParts];
	return parts.join(joiner);
}
