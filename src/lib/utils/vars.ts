import type { Translate } from '$lib/stores/i18n';
import { decodeQueryPart, parseQuery, splitFragment, type RestProperty } from './rest';


export const VAR_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;
export const MAX_VAR_NAME_CHARS = 64;
// A safe value is shown as this in the raw-request dump: the real secret never
// reaches SQLite or the frontend. Mirrors `vars.rs::SECRET_MASK`.
export const SECRET_MASK = '********';
// What a secure row shows in the variables table before it is revealed.
export const SECRET_DOTS = '••••••';
// How long a revealed secret stays on screen.
export const REVEAL_MS = 15000;

export interface VarRow {
	id: string;
	name: string;
	secure: boolean;
}

/** A variable as a dump or an export can write it: a safe value is either the mask
 *  or the revealed secret, never the `null` the variables list carries. */
export interface VarValue {
	id: string;
	secure: boolean;
	value: string;
}

/** Templates are looked up by the lowercased name, like the backend scanner. */
export type VarValues = Record<string, VarValue>;

export interface TemplateSource {
	url: string;
	headers: RestProperty[];
	body: string;
	body_type: string;
	form?: RestProperty[];
	multipart?: RestProperty[];
	files?: { name: string; path: string; filename: string }[];
}

/**
* A template starting at `start`, mirroring the Rust scanner: the inner text is
* trimmed and must be a valid name, and a nested `{{` does not stop the search for
* the closing `}}`. Returns the lowercase name and the index just after `}}`.
*/
function templateAt(chars: string[], start: number): { name: string; end: number } | null {
	if (start + 1 >= chars.length || chars[start] !== '{' || chars[start + 1] !== '{') {
		return null;
	}
	for (let index = start + 2; index + 1 < chars.length; index++) {
		if (chars[index] === '}' && chars[index + 1] === '}') {
		 const name = chars.slice(start + 2, index).join('').trim();
			return VAR_NAME_PATTERN.test(name) ? { name: name.toLowerCase(), end: index + 2 } : null;
		}
	}
	return null;
}

/** Every distinct variable name a text mentions, in the order it appears. */
export function extractTemplateNames(text: string): string[] {
	if (!text.includes('{{')) return [];
	const chars = [...text];
	const names: string[] = [];
	for (let index = 0; index < chars.length; index++) {
		const found = templateAt(chars, index);
		if (found) {
			if (!names.includes(found.name)) names.push(found.name);
			index = found.end - 1;
		}
	}
	return names;
}

export function containsTemplate(text: string): boolean {
	return extractTemplateNames(text).length > 0;
}

/** The `{{ name }}` a user copies into a request. */
export function templateFor(name: string): string {
	return `{{ ${name.trim()} }}`;
}

/**
* A name is usable when it is a C-style identifier of at most 64 characters.
* Duplicates are compared case-insensitively, and the row being edited does not
* duplicate itself.
*/
export function validateVarName(
	name: string,
	rows: VarRow[],
	currentId?: string
): 'required' | 'invalid' | 'duplicate' | null {
	const trimmed = name.trim();
	if (!trimmed) return 'required';
	if (trimmed.length > MAX_VAR_NAME_CHARS || !VAR_NAME_PATTERN.test(trimmed)) return 'invalid';
	const taken = rows.some(
		(row) => row.id !== currentId && row.name.toLowerCase() === trimmed.toLowerCase()
	);
	return taken ? 'duplicate' : null;
}

/**
 * Only the braces are decoded, mirroring `vars.rs::decoded_braces`: `Url::parse` writes a
 * literal `{`/`}` as `%7B`/`%7D`, so a template typed in the path or in a parameter name
 * reaches the backend in that form, and the encoder may have written either case.
 */
export function decodedBraces(text: string): string {
	return text
		.replace(/%7B/g, '{')
		.replace(/%7b/g, '{')
		.replace(/%7D/g, '}')
		.replace(/%7d/g, '}');
}

/**
 * Which places of a request can carry a template, and what they contain. The list mirrors
 * the backend: the host, path and query **names** are positions it refuses rather than
 * substitutes, so they count as templates too — a browser build cannot resolve any of them.
 */
function templateTexts(source: TemplateSource): string[] {
	// One scan of the raw URL covers the host, the path, parameter names and parameter
	// values, in the percent-encoded spelling `Url::parse` produces.
	const texts = [decodedBraces(source.url), ...source.headers.map((header) => header.name)];
	// A template typed into the Query table is written to the URL as `%7B%7Bname%7D%7D`,
	// so the decoded rows are the text the backend actually substitutes.
	for (const row of parseQuery(source.url)) {
		texts.push(decodedBraces(row.name), decodedBraces(row.value));
	}
	texts.push(...source.headers.map((header) => header.value));
	// Only the body the selected tab produces goes on the wire, so an unused buffer with a
	// broken template must not block the send — the same rule as `referencedVarNames`.
	if (source.body_type !== 'form' && source.body_type !== 'multipart') {
		texts.push(source.body);
	}
	if (source.body_type === 'form') texts.push(...(source.form ?? []).map((row) => row.value));
	if (source.body_type === 'multipart') {
		texts.push(...(source.multipart ?? []).map((row) => row.value));
		// Part names and file rows go into the body verbatim, so a template there is
		// something the backend refuses rather than something it substitutes.
		texts.push(...(source.multipart ?? []).map((row) => row.name));
		for (const file of source.files ?? []) texts.push(file.name, file.path, file.filename);
	}
	return texts;
}

export function requestHasTemplates(source: TemplateSource): boolean {
	return templateTexts(source).some(containsTemplate);
}

/**
 * Whether a SOAP call carries a template. It has no URL table, query rows or Form tabs,
 * so the places to scan are the envelope and the header table — the header **names**
 * included, because the backend refuses a template there rather than substituting it.
 */
export function soapHasTemplates(source: { body: string; headers: RestProperty[] }): boolean {
	return [
		source.body,
		...source.headers.map((row) => row.name),
		...source.headers.map((row) => row.value)
	].some(containsTemplate);
}

/**
 * Names the request mentions, deduplicated. Query values are read decoded, because
 * the URL field stores `{{ token }}` as `%7B%7Btoken%7D%7D`.
 */
export function referencedVarNames(source: TemplateSource): string[] {
	const names: string[] = [];
	for (const text of templateTexts(source)) {
		for (const name of extractTemplateNames(text)) {
			if (!names.includes(name)) names.push(name);
		}
	}
	return names;
}

/**
* A localized explanation of a backend error. Unknown text is returned verbatim so
* nothing is hidden behind a generic message, and an empty one answers `''` so the
* caller's own fallback names the failed action. An `Error` is read through its
* `message`, like `notifyError` does, so a browser failure keeps its own wording
* instead of gaining an `Error: ` prefix.
*/
export function describeVarError(error: unknown, t: Translate): string {
	const text = error instanceof Error ? error.message : String(error ?? '');
	const unknown = /^unknown-var:(.*)$/.exec(text);
	if (unknown) return t('vars.unknown', { name: unknown[1] });

	const missing = /^secret-missing:(.*)$/.exec(text);
	if (missing) return t('vars.secretMissing', { name: missing[1] });

	const store = /^secret-store:(.*)$/.exec(text);
	if (store) return t('vars.secretStore', { message: store[1] });

	if (text === 'secret-too-large') return t('vars.secretTooLarge');

	const duplicate = /^duplicate-name:(.*)$/.exec(text);
	if (duplicate) return t('vars.nameDuplicate', { name: duplicate[1] });

	const invalid = /^invalid-name:(.*)$/.exec(text);
	if (invalid) return t('vars.nameInvalid', { name: invalid[1] });

	// `<field>=<char>`: the backend found a control character in the text a variable
	// produced, which client validation could not see. The field is read up to the **last**
	// `=` because a multipart part name is not an RFC token in Rust and may contain one.
	const invalidValue = /^invalid-value:(.*)=([^=]*)$/.exec(text);
	if (invalidValue) {
		return t('vars.invalidValue', { field: invalidValue[1], char: invalidValue[2] });
	}

	const unresolved = /^unresolved-template:(.*)$/.exec(text);
	if (unresolved) return t('vars.unsupportedTemplate', { name: unresolved[1] });

	return text;
}

// The same grammar the scanner accepts: braces, an optional space, an ASCII name.
const TEMPLATE_BRACES = /\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;

/** A row as the variables list answers it: a safe value is never in the list. */
export interface VarRecord {
	id: string;
	name: string;
	secure: boolean;
	value: string | null;
}

/**
 * Indexes the variables list by the lowercased name, which is how the backend looks a
 * template up. A safe row starts as the mask, so a dump can be built without reading
 * the credential store.
 */
export function varValues(rows: VarRecord[]): VarValues {
	const values: VarValues = {};
	for (const row of rows) {
		values[row.name.toLowerCase()] = {
			id: row.id,
			secure: row.secure,
			value: row.secure ? SECRET_MASK : (row.value ?? '')
		};
	}
	return values;
}

/** The map a dump reads: a safe value becomes the mask the backend stores with it. */
export function maskedValues(values: VarValues): VarValues {
	const masked: VarValues = {};
	for (const [name, entry] of Object.entries(values)) {
		masked[name] = entry.secure ? { ...entry, value: SECRET_MASK } : entry;
	}
	return masked;
}

/** The map that leaves a safe template as text: plain variables only. */
export function plainValues(values: VarValues): VarValues {
	const plain: VarValues = {};
	for (const [name, entry] of Object.entries(values)) {
		if (!entry.secure) plain[name] = entry;
	}
	return plain;
}

/**
 * Replaces every `{{ name }}` whose variable is in `values`. There is no recursion:
 * the written value is never rescanned, so a value that contains a template stays
 * text, and `{{a{{b}}}}` becomes `{{a` + the value of `b` + `}}`.
 */
export function substituteText(text: string, values: VarValues): string {
	if (!text.includes('{{')) return text;
	let result = '';
	let cursor = 0;
	for (const match of text.matchAll(TEMPLATE_BRACES)) {
		const start = match.index ?? 0;
		result += text.slice(cursor, start);
		const entry = values[match[1].trim().toLowerCase()];
		result += entry ? entry.value : match[0];
		cursor = start + match[0].length;
	}
	return result + text.slice(cursor);
}

/** Percent-encodes a value the way the backend writes a substituted query pair. */
function formEncode(value: string): string {
	return encodeURIComponent(value).replace(/%20/g, '+');
}

/**
 * Substitutes the query **values** of a raw URL and leaves every other part exactly
 * as typed: a pair whose value had no template keeps its own spelling, a bare flag
 * gains no `=`, an empty pair survives and the fragment stays outside the query.
 * The value is decoded before it is scanned, which is what `vars.rs::substitute_url`
 * does with `form_decode` — a template typed into the Query table reaches the URL as
 * `%7B%7B%20name%20%7D%7D`, so scanning the raw text would never find it.
 */
export function substituteUrl(url: string, values: VarValues): string {
	const { withoutFragment, fragment } = splitFragment(url);
	const index = withoutFragment.indexOf('?');
	if (index < 0) return url;
	const head = withoutFragment.slice(0, index);
	const pairs = withoutFragment
		.slice(index + 1)
		.split('&')
		.map((segment) => {
			const separator = segment.indexOf('=');
			if (separator < 0) return segment;
			const name = segment.slice(0, separator);
			const decoded = decodeQueryPart(segment.slice(separator + 1));
			const substituted = substituteText(decoded, values);
			if (substituted === decoded) return segment;
			return `${name}=${formEncode(substituted)}`;
		})
		.join('&');
	return `${head}?${pairs}${fragment}`;
}
