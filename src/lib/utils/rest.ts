export interface RestProperty {
	name: string;
	value: string;
}

export type BodyType = 'json' | 'xml' | 'text' | 'form' | 'multipart';
export type CurlShell = 'posix' | 'windows';

/**
 * A `multipart/form-data` file part. `path` is what the desktop dialog picked and
 * what goes to SQLite; `file` only exists in the browser build, where the bytes come
 * from an `<input type=file>` and live in memory for the session.
 */
export interface RestFileRow {
	name: string;
	path: string;
	filename: string;
	file?: File | null;
}

export interface ParsedCurl {
	method: string;
	url: string;
	headers: RestProperty[];
	body: string;
	bodyType: BodyType;
	form: RestProperty[];
	multipart: RestProperty[];
	files: RestFileRow[];
}

const DEFAULT_USER_AGENT = 'WebDevUsefulTools Client';

function splitFragment(url: string): { withoutFragment: string; fragment: string } {
	const index = url.indexOf('#');
	if (index < 0) return { withoutFragment: url, fragment: '' };
	return { withoutFragment: url.slice(0, index), fragment: url.slice(index) };
}

function querySegments(url: string): string[] {
	const { withoutFragment } = splitFragment(url);
	const index = withoutFragment.indexOf('?');
	if (index < 0) return [];
	const query = withoutFragment.slice(index + 1);
	if (!query) return [];
	return query.split('&').filter((segment) => segment.length > 0);
}

export function parseQuery(url: string): RestProperty[] {
	return querySegments(url).map((segment) => {
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
		// A row with no name and no value is noise and goes away; a value with no
		// name is kept as `=value` so editing the name does not delete the value.
		// `validateQueryUrl` reports it, and the request stays blocked until it is fixed.
		.filter((row) => row.name.trim().length > 0 || row.value.trim().length > 0)
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

/**
 * Reads a file name out of a `Content-Disposition` value: the RFC 5987 extended
 * form wins over the plain one, quotes are stripped, and a broken percent
 * escape is kept as its own text rather than dropped. An empty parameter answers
 * `null`, like the Rust backend, so a caller can fall back to its own name.
 */
export function filenameFromDisposition(value: string): string | null {
	if (!value) return null;
	const star = value.match(/filename\*=([^;]+)/i);
	if (star) {
		const raw = star[1].trim().replace(/^UTF-8''/i, '').replace(/^"|"$/g, '');
		try {
			return decodeURIComponent(raw) || null;
		} catch {
			return raw || null;
		}
	}
	const plain = value.match(/filename=([^;]+)/i);
	if (plain) return plain[1].trim().replace(/^"|"$/g, '') || null;
	return null;
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
		});
}

export function buildCookieHeader(rows: RestProperty[]): string {
	return rows
		// A cookie with no name and no value is noise; a value with no name is kept
		// as `=value` so clearing the name does not delete the value, and
		// `validateProperties('cookie', …)` reports it as a missing name.
		.filter((row) => row.name.trim().length > 0 || row.value.trim().length > 0)
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
	multipart?: RestProperty[];
	files?: RestFileRow[];
}

/**
 * Resolves the text body that goes on the wire. The Form-encode and Form tables have
 * their own storage and never mix with the JSON/XML/Text buffer: when either tab is
 * active the tables win and the text body is ignored. A multipart body is binary, so
 * it is assembled separately (`buildMultipartPreview` for the raw dump, `FormData` in
 * the browser and the byte encoder in Rust for the real request).
 */
export function resolveRequestBody(source: RestBodySource): string {
	if (source.body_type === 'form') return encodeForm(source.form);
	if (source.body_type === 'multipart') return '';
	return source.body;
}

export function bodyTypeFromContentType(contentType: string): BodyType | null {
	const value = contentType.toLowerCase();
	if (value.includes('multipart/form-data')) return 'multipart';
	if (value.includes('json')) return 'json';
	if (value.includes('xml')) return 'xml';
	if (value.includes('x-www-form-urlencoded')) return 'form';
	if (value.startsWith('text/') || value.length > 0) return 'text';
	return null;
}

export function detectLanguageFromContentType(contentType: string): 'json' | 'xml' | 'html' | 'text' | null {
	const value = contentType.toLowerCase();
	if (!value) return null;
	// Anything whose subtype says HTML is read by the HTML language: `text/html`,
	// `application/xhtml+xml`, `application/xhtml`, `text/x-html`, `application/…+html`.
	// This runs before the XML test so XHTML never falls through to the XML language.
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
		// An HTML tag or an HTML doctype is HTML even without a `text/html` header;
		// other markup is better read by the XML language.
		const lower = trimmed.toLowerCase();
		return lower.includes('<html') || lower.startsWith('<!doctype html') ? 'html' : 'xml';
	}
	if (trimmed.startsWith('{') || trimmed.startsWith('[')) return 'json';
	return 'text';
}

export const MULTIPART_BOUNDARY_PLACEHOLDER = 'BoundaryPlaceholder';

export function defaultContentType(bodyType: BodyType, boundary?: string): string {
	switch (bodyType) {
		case 'json':
			return 'application/json';
		case 'xml':
			return 'application/xml';
		case 'form':
			return 'application/x-www-form-urlencoded';
		case 'multipart':
			return `multipart/form-data; boundary=${boundary ?? MULTIPART_BOUNDARY_PLACEHOLDER}`;
		default:
			return 'text/plain';
	}
}

/**
 * A boundary is a random UUID so it cannot collide with the text of a field value.
 * The desktop encoder generates its own in Rust; this one is used for the browser
 * request and for the raw-request preview, where it is only display text.
 */
export function multipartBoundary(): string {
	const crypto = globalThis.crypto as Crypto | undefined;
	if (crypto && typeof crypto.randomUUID === 'function') return crypto.randomUUID();
	const bytes = new Uint8Array(16);
	if (crypto && typeof crypto.getRandomValues === 'function') crypto.getRandomValues(bytes);
	else for (let index = 0; index < bytes.length; index += 1) bytes[index] = Math.floor(Math.random() * 256);
	bytes[6] = (bytes[6] & 0x0f) | 0x40;
	bytes[8] = (bytes[8] & 0x3f) | 0x80;
	const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
	return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * Media types for the browser build, where there is no OS media-type table. The
 * desktop encoder resolves the same thing through `mime_guess`, which knows more
 * extensions; both fall back to `application/octet-stream`.
 */
const MEDIA_TYPES: Record<string, string> = {
	txt: 'text/plain',
	md: 'text/markdown',
	csv: 'text/csv',
	html: 'text/html',
	htm: 'text/html',
	xml: 'application/xml',
	json: 'application/json',
	yaml: 'application/x-yaml',
	yml: 'application/x-yaml',
	toml: 'application/toml',
	js: 'text/javascript',
	mjs: 'text/javascript',
	css: 'text/css',
	svg: 'image/svg+xml',
	png: 'image/png',
	jpg: 'image/jpeg',
	jpeg: 'image/jpeg',
	gif: 'image/gif',
	webp: 'image/webp',
	pdf: 'application/pdf',
	zip: 'application/zip',
	gz: 'application/gzip',
	tar: 'application/x-tar',
	mp3: 'audio/mpeg',
	wav: 'audio/wav',
	mp4: 'video/mp4',
	webm: 'video/webm',
	woff: 'font/woff',
	woff2: 'font/woff2',
	ttf: 'font/ttf',
	otf: 'font/otf',
	xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
	docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
	pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation'
};

export function contentTypeForFilename(filename: string): string {
	const extension = filename.includes('.')
		? filename.slice(filename.lastIndexOf('.') + 1).toLowerCase()
		: '';
	return MEDIA_TYPES[extension] ?? 'application/octet-stream';
}

/** Last path segment of a POSIX or Windows path. */
export function basenameOf(path: string): string {
	const trimmed = path.replace(/[\\/]+$/, '');
	const index = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
	return index < 0 ? trimmed : trimmed.slice(index + 1);
}

/**
 * Part names and filenames are the only user text inside a part header, so the
 * encoder keeps them inside the quoting. The token check rejects names the
 * `Content-Disposition` grammar could not carry; this guard is the last one for a
 * filename, which is never validated because it comes from the file dialog.
 */
function quoteParameter(value: string): string {
	return value.replace(/["\\]/g, '_').replace(/[\r\n]/g, ' ');
}

/** True when the filename needs RFC 5987 `filename*` to survive. */
function needsExtendedFilename(filename: string): boolean {
	return !/^[A-Za-z0-9 .!#$%&'*+\-.^_`|~@()\[\]{}]+$/.test(filename);
}

export interface MultipartPreviewOptions {
	fields: RestProperty[];
	files: RestFileRow[];
	boundary: string;
}

/**
 * The wire layout of a multipart body with the file payloads replaced by a marker:
 * neither the browser nor the preview can read the bytes of a disk path, so the raw
 * dump only shows where the content goes. Parts with a blank name are skipped exactly
 * as the encoders skip them, which is why such a row is reported as `name-required`.
 */
export function buildMultipartPreview(options: MultipartPreviewOptions): string {
	const { boundary } = options;
	const lines: string[] = [];
	for (const field of options.fields) {
		if (!field.name.trim()) continue;
		lines.push(
			`--${boundary}`,
			`content-disposition: form-data; name="${quoteParameter(field.name.trim())}"`,
			'',
			field.value
		);
	}
	for (const file of options.files) {
		if (!file.name.trim()) continue;
		const filename = file.filename || (file.path ? basenameOf(file.path) : '');
		const parameters = [
			`name="${quoteParameter(file.name.trim())}"`,
			`filename="${quoteParameter(filename)}"`
		];
		if (needsExtendedFilename(filename)) parameters.push(`filename*=UTF-8''${encodeURIComponent(filename)}`);
		lines.push(
			`--${boundary}`,
			`content-disposition: form-data; ${parameters.join('; ')}`,
			`content-type: ${contentTypeForFilename(file.path || filename)}`,
			'',
			file.path ? `<bytes of ${file.path}>` : `<bytes of ${filename}, not loaded>`
		);
	}
	lines.push(`--${boundary}--`);
	return `${lines.join('\r\n')}\r\n`;
}

/**
 * Browser execution path: the bytes come from the picked `File` object, which only
 * lives in memory for the current session. `null` means a file row lost its object
 * after a reload, and `validateRequest` already blocks such a request.
 */
export function buildMultipartFormData(fields: RestProperty[], files: RestFileRow[]): FormData | null {
	const form = new FormData();
	for (const field of fields) {
		if (!field.name.trim()) continue;
		form.append(field.name.trim(), field.value);
	}
	for (const file of files) {
		if (!file.name.trim()) continue;
		if (!file.file) return null;
		form.append(file.name.trim(), file.file, file.filename || file.file.name);
	}
	return form;
}

/** Which property table a value belongs to; each one has its own grammar. */
export type PropertyKind = 'header' | 'cookie' | 'query' | 'form' | 'multipart';

export type ValidationKind =
	| 'header-name'
	| 'header-value'
	| 'cookie-name'
	| 'cookie-value'
	| 'query-param'
	| 'form-param'
	| 'multipart-field-name'
	| 'multipart-field-value'
	| 'multipart-file-name'
	| 'file-missing'
	| 'name-required';

export interface ValidationIssue {
	kind: ValidationKind;
	field: 'name' | 'value';
	/** Property name for display; it falls back to the offending text. */
	name: string;
	/** The offending character in a displayable form, such as `␍` or `U+0000`. */
	char: string;
	/** Row index within the table or the query string; absent for a single cell. */
	row?: number;
}

// RFC 9110 §5.6.2 token. Header names and cookie names are both tokens.
const TOKEN_PATTERN = /^[A-Za-z0-9!#$%&'*+\-.^_`|~]+$/;
const TOKEN_CHARACTER = /[A-Za-z0-9!#$%&'*+\-.^_`|~]/;
// RFC 9110 field-value keeps OWS (SP and HTAB) and printable characters. CR, LF
// and the rest of the control range would let a value break out of the framing.
const CONTROL_CHARACTER = /[\u0000-\u0008\u000A-\u001F\u007F]/;
// RFC 3986 §3.4: query = *( pchar / "/" / "?" ), and pchar already covers
// unreserved characters, sub-delims, ":" and "@". Percent-escapes are checked apart.
const QUERY_CHARACTER = /[A-Za-z0-9\-._~!$&'()*+,;=:@/?]/;
const PERCENT_ESCAPE = /^[0-9A-Fa-f]{2}/;
// RFC 6265 §4.1.1 cookie-value: printable ASCII without DQUOTE, comma, semicolon
// or backslash, because those delimit or quote the Cookie header.
const COOKIE_VALUE_CHARACTER = /[\u0021\u0023-\u002B\u002D-\u003A\u003C-\u005B\u005D-\u007E]/;

export function describeCharacter(character: string): string {
	switch (character) {
		case '\r':
			return '␍';
		case '\n':
			return '␊';
		case '\t':
			return '␉';
		case ' ':
			return '␠';
		default: {
			const code = character.codePointAt(0) ?? 0;
			if (code < 0x20 || code === 0x7f) {
				return `U+${code.toString(16).toUpperCase().padStart(4, '0')}`;
			}
			return character;
		}
	}
}

function firstDisallowed(text: string, allowed: RegExp): string {
	for (const character of text) {
		if (!allowed.test(character)) return character;
	}
	return '';
}

// A percent sign is legal only as the start of a complete `%XX` escape, so the
// scan skips the two hexadecimal digits with it instead of judging them apart.
function firstQueryViolation(text: string): string {
	for (let index = 0; index < text.length; index += 1) {
		const character = text[index];
		if (character === '%') {
			if (!PERCENT_ESCAPE.test(text.slice(index + 1))) return '%';
			index += 2;
			continue;
		}
		if (!QUERY_CHARACTER.test(character)) return character;
	}
	return '';
}

function makeIssue(
	kind: ValidationKind,
	field: 'name' | 'value',
	text: string,
	character: string
): ValidationIssue {
	return { kind, field, name: text, char: describeCharacter(character) };
}

export function validateHeaderName(name: string): ValidationIssue | null {
	if (!name) return null;
	if (!TOKEN_PATTERN.test(name)) {
		return makeIssue('header-name', 'name', name, firstDisallowed(name, TOKEN_CHARACTER));
	}
	return null;
}

export function validateHeaderValue(value: string): ValidationIssue | null {
	if (!value) return null;
	const match = CONTROL_CHARACTER.exec(value);
	if (match) return makeIssue('header-value', 'value', value, match[0]);
	return null;
}

export function validateCookieName(name: string): ValidationIssue | null {
	if (!name) return null;
	if (!TOKEN_PATTERN.test(name)) {
		return makeIssue('cookie-name', 'name', name, firstDisallowed(name, TOKEN_CHARACTER));
	}
	return null;
}

export function validateCookieValue(value: string): ValidationIssue | null {
	if (!value) return null;
	const character = firstDisallowed(value, COOKIE_VALUE_CHARACTER);
	if (character) return makeIssue('cookie-value', 'value', value, character);
	return null;
}

// Query rows are written into the URL verbatim, so they must already be valid.
function validateQueryText(text: string, field: 'name' | 'value'): ValidationIssue | null {
	if (!text) return null;
	const character = firstQueryViolation(text);
	if (character) return makeIssue('query-param', field, text, character);
	return null;
}

// Form rows are percent-encoded when the request is sent, so only characters that
// cannot survive the message framing are rejected here.
function validateFormText(text: string, field: 'name' | 'value'): ValidationIssue | null {
	if (!text) return null;
	const match = CONTROL_CHARACTER.exec(text);
	if (match) return makeIssue('form-param', field, text, match[0]);
	return null;
}

// A multipart part name is a `Content-Disposition` parameter written inside quotes and
// it is never percent-encoded, so it has to stay an RFC 9110 token. The value is
// written raw into the body, so only the framing characters are rejected.
function validateMultipartName(name: string): ValidationIssue | null {
	if (!name) return null;
	if (!TOKEN_PATTERN.test(name)) {
		return makeIssue('multipart-field-name', 'name', name, firstDisallowed(name, TOKEN_CHARACTER));
	}
	return null;
}

function validateMultipartValue(value: string): ValidationIssue | null {
	if (!value) return null;
	const match = CONTROL_CHARACTER.exec(value);
	if (match) return makeIssue('multipart-field-value', 'value', value, match[0]);
	return null;
}

/** Validates one cell of a property table. Blank cells are not errors. */
export function validatePropertyField(
	kind: PropertyKind,
	field: 'name' | 'value',
	text: string
): ValidationIssue | null {
	if (!text) return null;
	switch (kind) {
		case 'header':
			return field === 'name' ? validateHeaderName(text) : validateHeaderValue(text);
		case 'cookie':
			return field === 'name' ? validateCookieName(text) : validateCookieValue(text);
		case 'query':
			return validateQueryText(text, field);
		case 'form':
			return validateFormText(text, field);
		case 'multipart':
			return field === 'name' ? validateMultipartName(text) : validateMultipartValue(text);
	}
}

/**
 * A row needs a name to reach the request at all: header rows, cookie rows and
 * form rows with a blank name are dropped on persistence and on the wire, so the
 * value in such a row silently disappears. A fully empty row is harmless noise and
 * is not reported, while a value without a name is.
 */
export function validateRowName(row: RestProperty): ValidationIssue | null {
	if (row.name.trim() || !row.value.trim()) return null;
	return { kind: 'name-required', field: 'name', name: '', char: '' };
}

function validatePropertyRow(
	kind: PropertyKind,
	row: RestProperty,
	index: number
): ValidationIssue[] {
	const issues: ValidationIssue[] = [];
	const missingName = validateRowName(row);
	if (missingName) issues.push({ ...missingName, row: index });
	for (const field of ['name', 'value'] as const) {
		const issue = validatePropertyField(kind, field, row[field]);
		if (issue) issues.push({ ...issue, name: row.name.trim() || row[field], row: index });
	}
	return issues;
}

export function validateProperties(kind: PropertyKind, rows: RestProperty[]): ValidationIssue[] {
	return rows.flatMap((row, index) => validatePropertyRow(kind, row, index));
}

/**
 * Query rows are shown decoded, so the raw query string of the URL is what has to
 * follow RFC 3986. Issues carry the row index they belong to.
 */
export function validateQueryUrl(url: string): ValidationIssue[] {
	return querySegments(url)
		.map((segment, index): ValidationIssue | null => {
			const separator = segment.indexOf('=');
			const rawName = separator < 0 ? segment : segment.slice(0, separator);
			const rawValue = separator < 0 ? '' : segment.slice(separator + 1);
			// `?=1` is syntactically legal RFC 3986 but carries no name, so the
			// table row cannot be edited into anything sensible.
			if (!rawName && rawValue) {
				return { kind: 'name-required', field: 'name', name: '', char: '', row: index };
			}
			const issue = validateQueryText(rawName, 'name') ?? validateQueryText(rawValue, 'value');
			if (!issue) return null;
			return { ...issue, name: rawName || rawValue, row: index };
		})
		.filter((issue): issue is ValidationIssue => issue !== null);
}

export interface RequestValidationSource {
	url: string;
	headers: RestProperty[];
	form: RestProperty[];
	body_type: BodyType;
	multipart?: RestProperty[];
	files?: RestFileRow[];
}

/**
 * A file row must point at something the request can read: `path` on the desktop, the
 * picked `File` in the browser. A row that keeps only its filename after a reload has
 * no bytes left, so the send has to be blocked rather than silently dropping the part.
 */
export function validateFileRow(row: RestFileRow, index: number): ValidationIssue[] {
	const issues: ValidationIssue[] = [];
	const name = row.name.trim();
	// Something was picked, even if its content is gone: that is a row to report, not noise.
	const hasContent = Boolean(row.path.trim() || row.filename.trim() || row.file);
	const isReadable = Boolean(row.path.trim() || row.file);

	if (!name && hasContent) {
		issues.push({ kind: 'name-required', field: 'name', name: '', char: '', row: index });
	}
	if (name && !TOKEN_PATTERN.test(name)) {
		issues.push({
			...makeIssue('multipart-file-name', 'name', name, firstDisallowed(name, TOKEN_CHARACTER)),
			row: index
		});
	}
	if (name && !isReadable) {
		issues.push({ kind: 'file-missing', field: 'value', name, char: '', row: index });
	}
	return issues;
}

export function validateFileRows(rows: RestFileRow[]): ValidationIssue[] {
	return rows.flatMap((row, index) => validateFileRow(row, index));
}

/** Everything that goes on the wire: headers, cookies, query and the active body tables. */
export function validateRequest(request: RequestValidationSource): ValidationIssue[] {
	return [
		...validateProperties('header', request.headers),
		...validateProperties('cookie', parseCookies(request.headers)),
		...validateQueryUrl(request.url),
		// Body tables only reach the request when their tab is selected.
		...(request.body_type === 'form' ? validateProperties('form', request.form) : []),
		...(request.body_type === 'multipart'
			? [
					...validateProperties('multipart', request.multipart ?? []),
					...validateFileRows(request.files ?? [])
				]
			: [])
	];
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

/**
 * Builds the raw HTTP dump shown in the Raw request tab and stored with a response.
 * A multipart dump is synthetic by nature: the boundary differs on every run and the
 * file parts carry a marker instead of their bytes, because neither the browser nor
 * the preview can read a disk path.
 */
export function buildRawRequestPreview(
	method: string,
	url: string,
	headers: RestProperty[],
	body: string,
	bodyType: BodyType,
	options?: MultipartPreviewOptions
): string {
	const parsed = normalizeUrl(url);
	if (!parsed) return `${method.toUpperCase()} ${url}`;
	const multipart = bodyType === 'multipart' ? options : undefined;
	// A multipart boundary belongs inside the content-type, so a hand-typed
	// Content-Type header is replaced the same way the encoders replace it.
	const prepared = headers
		.filter((header) => header.name.trim())
		.filter((header) => !(multipart && header.name.toLowerCase() === 'content-type'))
		.map((header) => ({ ...header }));
	const has = (name: string) =>
		prepared.some((header) => header.name.toLowerCase() === name.toLowerCase());

	if (!has('user-agent')) prepared.push({ name: 'user-agent', value: DEFAULT_USER_AGENT });
	if (!has('accept-encoding')) prepared.push({ name: 'accept-encoding', value: 'gzip' });
	if (!has('host')) {
		const port = parsed.port ? `:${parsed.port}` : '';
		prepared.push({ name: 'host', value: `${parsed.hostname}${port}` });
	}

	const previewBody = multipart ? buildMultipartPreview(multipart) : body;
	if (multipart || previewBody) {
		if (!has('content-type')) {
			const value = multipart
				? defaultContentType('multipart', multipart.boundary)
				: defaultContentType(bodyType);
			prepared.push({ name: 'content-type', value });
		}
		if (!has('content-length')) {
			prepared.push({ name: 'content-length', value: String(byteLength(previewBody)) });
		}
	}

	const path = `${parsed.pathname || '/'}${parsed.search}`;
	const lines = [`${method.toUpperCase()} ${path} HTTP/1.1`];
	for (const header of prepared) lines.push(`${header.name}: ${header.value}`);
	return `${lines.join('\r\n')}\r\n\r\n${previewBody}`;
}

/**
 * Builds the raw HTTP response dump: the status line the server answered with, one
 * header per line, the blank line that separates the headers from the payload, and the
 * body exactly as it arrived — never formatted, never re-encoded.
 *
 * A stored answer from a database written before the status line was persisted has no
 * first line; that dump still shows what was kept, so an empty status line is skipped
 * rather than invented. With neither line nor body there is nothing to show, hence `''`.
 */
export function buildRawResponse(
	statusLine: string,
	headers: RestProperty[],
	body: string
): string {
	const lines: string[] = [];
	if (statusLine.trim()) lines.push(statusLine.trim());
	for (const header of headers) {
		if (header.name.trim()) lines.push(`${header.name}: ${header.value}`);
	}
	if (!lines.length) return body;
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

// Bash ANSI-C quoting (`$'…'`) turns these escapes back into characters, and
// browsers emit it for "Copy as cURL (bash)" whenever an argument has line breaks.
const ANSI_C_ESCAPES: Record<string, string> = {
	n: '\n',
	r: '\r',
	t: '\t',
	a: '\u0007',
	b: '\u0008',
	f: '\u000C',
	v: '\u000B',
	e: '\u001B',
	'\\': '\\',
	"'": "'",
	'"': '"',
	'?': '?',
	'$': '$'
};

interface EscapeDecoding {
	text: string;
	/** Index of the first character after the escape sequence. */
	next: number;
}

function decodeAnsiCEscape(input: string, start: number): EscapeDecoding {
	const char = input[start];
	if (char === '\n' || char === '\r') return { text: '', next: start + 1 };

	const hexLength = char === 'x' ? 2 : char === 'u' ? 4 : char === 'U' ? 8 : 0;
	if (hexLength > 0) {
		// Bash takes one to N hex digits, so `\u41` and `\x4A` name a character, and no
		// digits at all leaves the escape literal.
		const digits = new RegExp(`^[0-9a-fA-F]{1,${hexLength}}`).exec(input.slice(start + 1))?.[0] ?? '';
		const code = Number.parseInt(digits, 16);
		if (digits && Number.isFinite(code) && code <= 0x10ffff) {
			// A lone surrogate names no character: `String.fromCodePoint` throws for it.
			if (code < 0xd800 || code > 0xdfff) {
				return { text: String.fromCodePoint(code), next: start + 1 + digits.length };
			}
		}
		return { text: `\\${char}`, next: start + 1 };
	}

	if (char >= '0' && char <= '7') {
		const digits = /^[0-7]{1,3}/.exec(input.slice(start))?.[0] ?? '';
		const code = Number.parseInt(digits, 8);
		// Bash drops a NUL byte instead of putting it in the argument.
		return { text: code > 0 ? String.fromCharCode(code) : '', next: start + digits.length };
	}

	const simple = ANSI_C_ESCAPES[char];
	if (simple !== undefined) return { text: simple, next: start + 1 };
	return { text: `\\${char}`, next: start + 1 };
}

// POSIX double quotes keep the backslash except in front of these.
const DOUBLE_QUOTE_ESCAPES = new Set(['$', '`', '"', '\\', '\n', '\r']);

function tokenizeCurl(input: string): string[] {
	const tokens: string[] = [];
	let current = '';
	// A quoted argument is a token even when it is empty, so `-H ''` does not shift
	// the following argument into its place.
	let started = false;
	let quote: 'single' | 'double' | 'ansi' | null = null;

	const flush = () => {
		if (started) tokens.push(current);
		current = '';
		started = false;
	};

	for (let index = 0; index < input.length; index += 1) {
		const char = input[index];

		if (quote === 'single') {
			if (char === "'") quote = null;
			else current += char;
			continue;
		}

		if (quote === 'ansi') {
			if (char === "'") {
				quote = null;
				continue;
			}
			if (char === '\\' && index + 1 < input.length) {
				const decoded = decodeAnsiCEscape(input, index + 1);
				current += decoded.text;
				index = decoded.next - 1;
				continue;
			}
			current += char;
			continue;
		}

		if (quote === 'double') {
			if (char === '"') {
				quote = null;
				continue;
			}
			if (char === '\\' && DOUBLE_QUOTE_ESCAPES.has(input[index + 1] ?? '')) {
				index += 1;
				const next = input[index];
				// `\<newline>` inside quotes is a line continuation, not a character.
				if (next !== '\n' && next !== '\r') current += next;
				continue;
			}
			current += char;
			continue;
		}

		if (char === "'") {
			quote = 'single';
			started = true;
			continue;
		}
		if (char === '"') {
			quote = 'double';
			started = true;
			continue;
		}
		if (char === '$' && (input[index + 1] === "'" || input[index + 1] === '"')) {
			// `$'…'` is bash ANSI-C quoting; `$"…"` is locale quoting and escapes
			// like a double-quoted Windows argument.
			quote = input[index + 1] === "'" ? 'ansi' : 'double';
			started = true;
			index += 1;
			continue;
		}
		if (char === '\\' && index + 1 < input.length) {
			index += 1;
			current += input[index];
			started = true;
			continue;
		}
		if (/\s/.test(char)) {
			flush();
			continue;
		}
		current += char;
		started = true;
	}

	flush();
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

// Exporters that are not shell-correct wrap the payload in plain quotes and still
// escape it (`\"`, `\n`), so the shell would send the backslashes literally. Only
// these sequences mark a value as escaped; `\u` is excluded because JSON bodies use
// it as real content.
const ESCAPED_PAYLOAD = /\\(?:[nrtbfv]|["'])/;

const PAYLOAD_ESCAPES: Record<string, string> = {
	n: '\n',
	r: '\r',
	t: '\t',
	b: '\u0008',
	f: '\u000C',
	v: '\u000B',
	'"': '"',
	"'": "'",
	'\\': '\\'
};

function decodePayloadEscapes(value: string): string {
	return value.replace(/\\(.)/g, (whole, char: string) => PAYLOAD_ESCAPES[char] ?? whole);
}

/** `null` when the body type has no grammar to check against. */
function isStructuredBodyValid(bodyType: BodyType, text: string): boolean | null {
	if (!text.trim()) return null;
	if (bodyType === 'json') {
		try {
			JSON.parse(text);
			return true;
		} catch {
			return false;
		}
	}
	if (bodyType === 'xml') {
		if (typeof DOMParser === 'undefined') return null;
		const doc = new DOMParser().parseFromString(text, 'application/xml');
		return doc.getElementsByTagName('parsererror').length === 0;
	}
	return null;
}

/**
 * Turns shell-level escape sequences of an imported body back into characters. It
 * runs only when the raw text cannot be what the user meant: a body that already
 * parses as JSON/XML is kept verbatim, and a body with real line breaks is kept too,
 * because there `\n` is content rather than an encoded break. Form-encode bodies are
 * left alone: their rows go into a table that cannot hold control characters.
 */
function unescapeImportedBody(raw: string, bodyType: BodyType): string {
	if (!raw || bodyType === 'form' || !ESCAPED_PAYLOAD.test(raw)) return raw;
	const decoded = decodePayloadEscapes(raw);
	if (decoded === raw) return raw;
	if (isStructuredBodyValid(bodyType, raw) === true) return raw;
	if (isStructuredBodyValid(bodyType, decoded) === true) return decoded;
	return /[\r\n]/.test(raw) ? raw : decoded;
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

/**
 * curl's `-F` grammar puts trailing parameters after the value
 * (`name=@file.txt;filename=alias.txt;type=text/plain`). Only the trailing pair is
 * taken apart, so a semicolon inside the value itself survives.
 */
function splitFormParameters(raw: string): { value: string; filename: string } {
	// curl accepts `;filename=` and `;type=` in any order after the value, and each
	// parameter ends at the next semicolon. Other semicolons stay inside the text, so
	// a plain field value like `a;b` is not cut apart by this parser.
	const parameter = /;filename=([^;]*)|;type=([^;]*)/g;
	let value = '';
	let filename = '';
	let cursor = 0;
	for (const match of raw.matchAll(parameter)) {
		value += raw.slice(cursor, match.index ?? 0);
		cursor = (match.index ?? 0) + match[0].length;
		if (match[1] !== undefined) filename = match[1];
	}
	value += raw.slice(cursor);
	return { value, filename };
}

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
	const multipart: RestProperty[] = [];
	const files: RestFileRow[] = [];

	const addHeader = (value: string) => {
		const header = splitHeader(value);
		if (!header) return;
		const existing = headers.findIndex(
			(item) => item.name.toLowerCase() === header.name.toLowerCase()
		);
		if (existing >= 0) headers[existing] = header;
		else headers.push(header);
	};

	// `-F name=@file` uploads a file; `--form-string` keeps `@` as literal text.
	const addFormPart = (raw: string, interpretFile: boolean) => {
		const separator = raw.indexOf('=');
		if (separator < 0) return;
		const name = raw.slice(0, separator);
		if (!name) return;
		const { value, filename } = splitFormParameters(raw.slice(separator + 1));
		if (interpretFile && value.startsWith('@') && value.length > 1) {
			const path = value.slice(1);
			files.push({ name, path, filename: filename || basenameOf(path) });
		} else {
			multipart.push({ name, value });
		}
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
		if (token === '-F' || token === '--form') {
			addFormPart(tokens[index + 1] ?? '', true);
			index += 1;
			continue;
		}
		if (token === '--form-string') {
			addFormPart(tokens[index + 1] ?? '', false);
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
	const hasFormParts = multipart.length > 0 || files.length > 0;
	// curl sends `-d` together with `-F` as extra parts of the same multipart body.
	if (hasFormParts && rawBody) multipart.push(...parseForm(rawBody));
	const contentTypeIndex = headers.findIndex((header) => header.name.toLowerCase() === 'content-type');
	// The client generates the boundary, so an imported boundary from another tool is
	// noise the user cannot act on; dropping it keeps the header list honest.
	if (hasFormParts && contentTypeIndex >= 0) headers.splice(contentTypeIndex, 1);

	const contentType = getHeader(headers, 'content-type');
	const importedType = hasFormParts ? 'multipart' : bodyTypeFromContentType(contentType) ?? sniffBodyType(rawBody);
	const textBody = unescapeImportedBody(rawBody, importedType);
	// Re-sniff after de-escaping: an escaped payload usually only looks like JSON or
	// XML once the escapes are gone, and there is no Content-Type to trust.
	const bodyType = hasFormParts
		? 'multipart'
		: bodyTypeFromContentType(contentType) ?? sniffBodyType(textBody);
	const form = bodyType === 'form' ? parseForm(rawBody) : [];
	// Form-encode rows and Form tables live in their own storage, so an imported body
	// of those kinds does not occupy the JSON/XML/Text buffer.
	const body = bodyType === 'form' || bodyType === 'multipart' ? '' : textBody;

	return {
		method: (method || (rawBody || hasFormParts ? 'POST' : 'GET')).toUpperCase(),
		url,
		headers,
		body,
		bodyType,
		form,
		multipart,
		files
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
		body_type?: BodyType;
		multipart?: RestProperty[];
		files?: RestFileRow[];
	},
	shell: CurlShell
): string {
	const quote = shell === 'posix' ? quotePosix : quoteWindows;
	const executable = shell === 'posix' ? 'curl' : 'curl.exe';
	// cmd.exe needs ` ^` before every newline so the lines are one command.
	const joiner = shell === 'posix' ? ' ' : ' ^\r\n';
	const methodPart = `--request ${request.method.toUpperCase()}`;
	const isMultipart = request.body_type === 'multipart';
	// curl sets its own boundary for `--form`, so a Content-Type from the table would
	// only contradict it.
	const headerParts = request.headers
		.filter((header) => header.name.trim())
		.filter((header) => !(isMultipart && header.name.toLowerCase() === 'content-type'))
		.map((header) => `-H ${quote(`${header.name}: ${header.value}`)}`);
	const bodyParts = isMultipart
		? multipartCurlParts(request.multipart ?? [], request.files ?? [], quote)
		: request.body
			? [`--data-raw ${quote(request.body)}`]
			: [];
	// cmd.exe is picky about the URL token position, so it goes right after curl.exe.
	const parts =
		shell === 'posix'
			? [executable, methodPart, ...headerParts, ...bodyParts, quote(request.url)]
			: [`${executable} ${quote(request.url)}`, methodPart, ...headerParts, ...bodyParts];
	return parts.join(joiner);
}

/**
 * `-F` parts for a multipart body. `--form-string` is not used because curl would
 * then refuse a `@` at the start of a value; a value with a leading `@` is rare and
 * the exported command is still readable. File rows without a path cannot be
 * exported, so they are skipped — `validateRequest` blocks that case beforehand.
 */
function multipartCurlParts(
	fields: RestProperty[],
	files: RestFileRow[],
	quote: (value: string) => string
): string[] {
	const parts: string[] = [];
	for (const field of fields) {
		if (!field.name.trim()) continue;
		parts.push(`--form ${quote(`${field.name.trim()}=${field.value}`)}`);
	}
	for (const file of files) {
		if (!file.name.trim() || !file.path.trim()) continue;
		const name = file.name.trim();
		const alias = file.filename.trim() && file.filename.trim() !== basenameOf(file.path.trim())
			? `;filename=${file.filename.trim()}`
			: '';
		parts.push(`--form ${quote(`${name}=@${file.path.trim()}${alias}`)}`);
	}
	return parts;
}
