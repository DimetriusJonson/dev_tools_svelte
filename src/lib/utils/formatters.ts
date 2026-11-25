export type Mode = 'xml' | 'json';

export interface FormatOptions {
	indent: number;
	doFormat: boolean;
	doUnescape: boolean;
}

const NAMED_ENTITIES: Record<string, string> = {
	lt: '<',
	gt: '>',
	amp: '&',
	quot: '"',
	apos: "'",
	nbsp: '\u00a0'
};

const UNESCAPE_MAP: Record<string, string> = {
	'"': '"',
	'\\': '\\',
	'/': '/',
	b: '\b',
	f: '\f',
	n: '\n',
	r: '\r',
	t: '\t'
};

/** Numeric XML entities must name a Unicode scalar: `String.fromCodePoint` throws for
 * out-of-range values and lone surrogates, which must stay literal entity text instead. */
function isValidCodePoint(code: number): boolean {
	return Number.isFinite(code) && code >= 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff);
}

export function decodeXmlEntities(input: string): string {
	return input.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*);/g, (match, body: string) => {
		if (body.startsWith('#x') || body.startsWith('#X')) {
			const code = Number.parseInt(body.slice(2), 16);
			return isValidCodePoint(code) ? String.fromCodePoint(code) : match;
		}
		if (body.startsWith('#')) {
			const code = Number.parseInt(body.slice(1), 10);
			return isValidCodePoint(code) ? String.fromCodePoint(code) : match;
		}
		const value = NAMED_ENTITIES[body];
		return value ?? match;
	});
}

export function escapeXmlText(input: string): string {
	return input
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&apos;');
}

export function escapeXml(input: string): string {
	return escapeXmlText(input);
}

export function unescapeJson(input: string): string {
	const trimmed = input.trim();
	if (trimmed.startsWith('"') && trimmed.endsWith('"')) {
		try {
			const parsed = JSON.parse(trimmed);
			if (typeof parsed === 'string') return parsed;
		} catch {
			/* fall through to manual unescape */
		}
	}

	return input.replace(/\\(u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2}|["\\/bfnrt])/g, (match, body: string) => {
		if (body.startsWith('u') || body.startsWith('x')) {
			const code = Number.parseInt(body.slice(1), 16);
			return isValidCodePoint(code) ? String.fromCodePoint(code) : match;
		}
		return UNESCAPE_MAP[body] ?? match;
	});
}

export function escapeJson(input: string): string {
	let out = '';
	for (const char of input) {
		const code = char.codePointAt(0) ?? 0;
		switch (char) {
			case '"':
				out += '\\"';
				break;
			case '\\':
				out += '\\\\';
				break;
			case '\b':
				out += '\\b';
				break;
			case '\f':
				out += '\\f';
				break;
			case '\n':
				out += '\\n';
				break;
			case '\r':
				out += '\\r';
				break;
			case '\t':
				out += '\\t';
				break;
			default:
				if (code < 0x20) {
					out += `\\u${code.toString(16).padStart(4, '0')}`;
				} else {
					out += char;
				}
		}
	}
	return out;
}

/**
 * Splits loose content that sits before the first JSON bracket from the JSON body itself,
 * so `aaa` followed by a JSON document still gets formatted. Junk that looks like markup
 * is refused: an HTML body has no JSON structure to re-indent.
 */
function splitJsonJunk(source: string): { junk: string; body: string } | null {
	const index = source.search(/[{[]/);
	if (index === -1) return null;
	const junk = source.slice(0, index);
	const body = source.slice(index);
	if (junk.includes('<')) return null;
	if (junk.trim().length > 0 && !body.includes('"')) return null;
	return { junk, body };
}

function withJsonJunk(junk: string, body: string): string {
	const prefix = junk.trimEnd();
	return prefix ? `${prefix}\n${body}` : body;
}

/** Structural characters decide which unescape candidate is the better layout source. */
function jsonShapeScore(source: string): number {
	let score = 0;
	for (const char of source) {
		if (char === '{' || char === '}' || char === '[' || char === ']' || char === '"') score += 1;
	}
	return score;
}

function jsonTolerantCandidate(candidates: string[]): string | null {
	let best: string | null = null;
	let bestScore = -1;
	for (const candidate of candidates) {
		if (!splitJsonJunk(candidate)) continue;
		const score = jsonShapeScore(candidate);
		if (score >= bestScore) {
			bestScore = score;
			best = candidate;
		}
	}
	return best;
}

/**
 * Re-indents JSON that `JSON.parse` rejects. Structural characters break lines, other
 * content is kept verbatim, and unclosed brackets stay unclosed: nothing is invented.
 */
export function prettyJsonTolerant(source: string, indent: number): string {
	const pad = ' '.repeat(indent);
	const parts: { depth: number; text: string }[] = [];
	let depth = 0;
	let lineDepth = 0;
	let line = '';
	let spacing = false;
	let inString = false;
	let escaped = false;

	const commit = (): void => {
		const text = line.trim();
		if (text) parts.push({ depth: lineDepth, text });
		line = '';
		spacing = false;
	};
	const startLine = (): void => {
		commit();
		lineDepth = depth;
	};
	/** Emits the single space that a collapsed whitespace run stood for. */
	const pendingSpace = (): void => {
		if (spacing) line += ' ';
		spacing = false;
	};
	const word = (char: string): void => {
		pendingSpace();
		line += char;
	};

	for (const char of source) {
		if (inString) {
			line += char;
			if (escaped) escaped = false;
			else if (char === '\\') escaped = true;
			else if (char === '"') inString = false;
			continue;
		}
		if (char === '"') {
			word(char);
			inString = true;
			continue;
		}
		if (/\s/.test(char)) {
			if (char === '\n' || char === '\r') {
				startLine();
				continue;
			}
			if (line.trim()) spacing = true;
			continue;
		}
		if (char === '{' || char === '[') {
			pendingSpace();
			line += char;
			depth += 1;
			startLine();
			continue;
		}
		if (char === '}' || char === ']') {
			spacing = false;
			depth = Math.max(0, depth - 1);
			startLine();
			line += char;
			continue;
		}
		if (char === ',') {
			spacing = false;
			line += char;
			startLine();
			continue;
		}
		if (char === ':') {
			spacing = false;
			line += ':';
			spacing = true;
			continue;
		}
		word(char);
	}
	commit();

	return parts.map((part) => pad.repeat(part.depth) + part.text).join('\n');
}

function parseJsonSmart(source: string, doUnescape: boolean): unknown {
	const attempts = [source];

	if (doUnescape) {
		const unescaped = unescapeJson(source);
		attempts.push(unescaped);
		const decoded = decodeXmlEntities(unescaped);
		if (decoded !== unescaped) attempts.push(decoded);
	}

	let lastError: unknown = null;
	for (const attempt of attempts) {
		try {
			let value = JSON.parse(attempt);
			if (doUnescape && typeof value === 'string') {
				try {
					value = JSON.parse(value);
				} catch {
					/* keep the string value */
				}
			}
			return value;
		} catch (error) {
			lastError = error;
		}
	}

	throw lastError instanceof Error ? lastError : new Error('Invalid JSON');
}

export function formatJsonText(source: string, indent: number, doUnescape = false): string {
	const candidates = [source];
	if (doUnescape) {
		const unescaped = unescapeJson(source);
		candidates.push(unescaped);
		const decoded = decodeXmlEntities(unescaped);
		if (decoded !== unescaped) candidates.push(decoded);
	}

	let strictError: unknown = null;
	for (const candidate of candidates) {
		try {
			return JSON.stringify(parseJsonSmart(candidate, doUnescape), null, indent);
		} catch (error) {
			strictError = strictError ?? error;
		}
	}

	// Junk around a real JSON document is kept as its own line, the body is formatted strictly.
	for (const candidate of candidates) {
		const split = splitJsonJunk(candidate);
		if (!split) continue;
		try {
			const value = parseJsonSmart(split.body, doUnescape);
			return withJsonJunk(split.junk, JSON.stringify(value, null, indent));
		} catch {
			/* the body itself is broken too: fall through to the tolerant layout */
		}
	}

	const candidate = jsonTolerantCandidate(candidates);
	if (!candidate) throw strictError instanceof Error ? strictError : new Error('Invalid JSON');

	const split = splitJsonJunk(candidate);
	if (!split) return prettyJsonTolerant(candidate, indent);
	return withJsonJunk(split.junk, prettyJsonTolerant(split.body, indent));
}

function xmlParseError(doc: Document): boolean {
	return doc.getElementsByTagName('parsererror').length > 0;
}

function extractDeclaration(source: string): string | null {
	const match = source.match(/^\s*(<\?xml[\s\S]*?\?>)/);
	return match ? match[1] : null;
}

function serializeAttributes(element: Element): string {
	let out = '';
	for (const attr of Array.from(element.attributes)) {
		out += ` ${attr.name}="${escapeXmlText(attr.value)}"`;
	}
	return out;
}

/**
 * Format-agnostic XML tree. `verbatim` text is emitted as it was read (the tolerant
 * path never re-escapes source bytes), and `block` raw nodes force a multiline element.
 */
type XmlText = { kind: 'text'; text: string; verbatim?: boolean };
type XmlRaw = { kind: 'raw'; text: string; block: boolean };
type XmlElement = { kind: 'element'; name: string; attrs: string; children: XmlVirtual[] };
type XmlVirtual = XmlText | XmlRaw | XmlElement;

function toVirtualNode(node: Node): XmlVirtual | null {
	switch (node.nodeType) {
		case Node.TEXT_NODE:
			return { kind: 'text', text: node.textContent ?? '' };
		case Node.CDATA_SECTION_NODE:
			return { kind: 'raw', text: `<![CDATA[${node.textContent ?? ''}]]>`, block: false };
		case Node.COMMENT_NODE:
			return { kind: 'raw', text: `<!--${node.textContent ?? ''}-->`, block: true };
		case Node.PROCESSING_INSTRUCTION_NODE: {
			const instruction = node as ProcessingInstruction;
			return {
				kind: 'raw',
				text: `<?${instruction.target}${instruction.data ? ` ${instruction.data}` : ''}?>`,
				block: false
			};
		}
		case Node.DOCUMENT_TYPE_NODE: {
			const doctype = node as DocumentType;
			let value = `<!DOCTYPE ${doctype.name}`;
			if (doctype.publicId) value += ` PUBLIC "${doctype.publicId}"`;
			if (doctype.systemId) value += `${doctype.publicId ? '' : ' SYSTEM'} "${doctype.systemId}"`;
			return { kind: 'raw', text: `${value}>`, block: true };
		}
		case Node.ELEMENT_NODE: {
			const element = node as Element;
			const children = Array.from(element.childNodes)
				.map(toVirtualNode)
				.filter((child): child is XmlVirtual => child !== null);
			return {
				kind: 'element',
				name: element.tagName,
				attrs: serializeAttributes(element),
				children
			};
		}
		default:
			return null;
	}
}

function serializeVirtual(node: XmlVirtual, depth: number, pad: string, block: boolean): string {
	const indent = pad.repeat(depth);

	if (node.kind === 'text') {
		const text = node.text.trim();
		if (!text) return block ? '' : node.text;
		return indent + (node.verbatim ? text : escapeXmlText(text));
	}

	if (node.kind === 'raw') return indent + node.text;

	const open = `<${node.name}${node.attrs}`;
	const children = node.children.filter(
		(child) => child.kind !== 'text' || child.text.trim().length > 0
	);

	if (children.length === 0) return `${indent}${open}/>`;

	const inlineText = (child: XmlVirtual): string => {
		if (child.kind === 'raw') return child.text;
		if (child.kind === 'element') return '';
		const text = child.text.trim();
		return child.verbatim ? text : escapeXmlText(text);
	};

	const hasBlockChild = children.some(
		(child) => child.kind === 'element' || (child.kind === 'raw' && child.block)
	);

	if (!hasBlockChild) {
		return `${indent}${open}>${children.map(inlineText).join('')}</${node.name}>`;
	}

	const inner = children
		.map((child) => serializeVirtual(child, depth + 1, pad, true))
		.filter((value) => value.length > 0)
		.join('\n');

	return `${indent}${open}>\n${inner}\n${indent}</${node.name}>`;
}

function serializeXmlRoot(children: XmlVirtual[], pad: string): string {
	return children
		.map((child) => serializeVirtual(child, 0, pad, false).trim())
		.filter((value) => value.length > 0)
		.join('\n');
}

type XmlToken =
	| { kind: 'open'; name: string; attrs: string; selfClosing: boolean }
	| { kind: 'close'; name: string }
	| { kind: 'raw'; text: string }
	| { kind: 'text'; text: string };

const NAME_START = /[A-Za-z_:]/;
const NAME_CHAR = /[A-Za-z0-9_.:-]/;

/** Index of the `>` that ends a tag, ignoring `>` inside quoted attribute values. */
function tagEnd(markup: string, from: number): number {
	let quote: string | null = null;
	for (let index = from; index < markup.length; index += 1) {
		const char = markup[index];
		if (quote) {
			if (char === quote) quote = null;
		} else if (char === '"' || char === "'") {
			quote = char;
		} else if (char === '>') {
			return index;
		}
	}
	return -1;
}

function rawToken(
	markup: string,
	index: number,
	opener: string,
	closer: string
): { kind: 'raw'; text: string } {
	const found = markup.slice(index + opener.length).indexOf(closer);
	const end = found === -1 ? markup.length : index + opener.length + found + closer.length;
	return { kind: 'raw', text: markup.slice(index, end) };
}

/** Reads one markup construct at `index`, or `null` when `<` there is just text. */
function markupToken(markup: string, index: number): { token: XmlToken; end: number } | null {
	if (markup.startsWith('<!--', index)) {
		const token = rawToken(markup, index, '<!--', '-->');
		return { token, end: index + token.text.length };
	}
	if (markup.startsWith('<![CDATA[', index)) {
		const token = rawToken(markup, index, '<![CDATA[', ']]>');
		return { token, end: index + token.text.length };
	}
	if (markup.startsWith('<?', index)) {
		const token = rawToken(markup, index, '<?', '?>');
		return { token, end: index + token.text.length };
	}
	if (markup.startsWith('<!', index)) {
		const token = rawToken(markup, index, '<!', '>');
		return { token, end: index + token.text.length };
	}

	const closing = markup.startsWith('</', index);
	const start = closing ? index + 2 : index + 1;
	if (!NAME_START.test(markup[start] ?? '')) return null;

	let cursor = start;
	while (cursor < markup.length && NAME_CHAR.test(markup[cursor])) cursor += 1;
	const name = markup.slice(start, cursor);
	const end = tagEnd(markup, cursor);
	if (end === -1) return null;

	if (closing) return { token: { kind: 'close', name }, end: end + 1 };

	const raw = markup.slice(index, end + 1);
	const selfClosing = raw.endsWith('/>');
	const attrs = selfClosing ? raw.slice(name.length + 1, raw.length - 2) : raw.slice(name.length + 1, raw.length - 1);
	return { token: { kind: 'open', name, attrs, selfClosing }, end: end + 1 };
}

/** Splits markup into tokens without validating it: unknown `<` stays text. */
function tokenizeXml(markup: string): XmlToken[] {
	const tokens: XmlToken[] = [];
	let index = 0;
	let textStart = 0;

	while (index < markup.length) {
		if (markup[index] === '<') {
			const found = markupToken(markup, index);
			if (found) {
				if (index > textStart) tokens.push({ kind: 'text', text: markup.slice(textStart, index) });
				tokens.push(found.token);
				index = found.end;
				textStart = found.end;
				continue;
			}
		}
		index += 1;
	}

	if (markup.length > textStart) tokens.push({ kind: 'text', text: markup.slice(textStart) });
	return tokens;
}

function markupTokenCount(tokens: XmlToken[]): number {
	return tokens.filter((token) => token.kind !== 'text').length;
}

/**
 * Builds a tree from unvalidated tokens: unmatched closing tags are dropped from the
 * stack (unclosed elements close where their parent does), and a stray closing tag is
 * kept as literal content.
 */
function buildTolerantTree(tokens: XmlToken[]): XmlVirtual[] {
	const root: XmlVirtual[] = [];
	const stack: XmlElement[] = [];
	const container = (): XmlVirtual[] => (stack.length ? stack[stack.length - 1].children : root);

	for (const token of tokens) {
		if (token.kind === 'text') {
			container().push({ kind: 'text', text: token.text, verbatim: true });
			continue;
		}
		if (token.kind === 'raw') {
			container().push({ kind: 'raw', text: token.text, block: token.text.startsWith('<!--') });
			continue;
		}
		if (token.kind === 'open') {
			const element: XmlElement = {
				kind: 'element',
				name: token.name,
				attrs: token.attrs,
				children: []
			};
			container().push(element);
			if (!token.selfClosing) stack.push(element);
			continue;
		}

		let matched = -1;
		for (let index = stack.length - 1; index >= 0; index -= 1) {
			if (stack[index].name === token.name) {
				matched = index;
				break;
			}
		}
		if (matched >= 0) {
			stack.length = matched;
			continue;
		}
		container().push({ kind: 'raw', text: `</${token.name}>`, block: true });
	}

	return root;
}

export function formatXmlText(source: string, indent: number, doUnescape = false): string {
	const parser = new DOMParser();
	const pad = ' '.repeat(indent);
	const candidates = [source];
	if (doUnescape) {
		const decoded = decodeXmlEntities(source);
		if (decoded !== source) candidates.push(decoded);
	}

	for (const candidate of candidates) {
		const doc = parser.parseFromString(candidate, 'application/xml');
		if (xmlParseError(doc)) continue;

		const children: XmlVirtual[] = [];
		for (const node of Array.from(doc.childNodes)) {
			if (
				node.nodeType === Node.PROCESSING_INSTRUCTION_NODE &&
				(node as ProcessingInstruction).target.toLowerCase() === 'xml'
			) {
				continue;
			}
			const virtual = toVirtualNode(node);
			if (virtual) children.push(virtual);
		}

		const declaration = extractDeclaration(candidate);
		const body = serializeXmlRoot(children, pad);
		if (declaration) return body ? `${declaration}\n${body}` : declaration;
		return body;
	}

	// Malformed markup is still worth formatting: re-indent it instead of rejecting it.
	let best: XmlToken[] = [];
	for (const candidate of candidates) {
		const tokens = tokenizeXml(candidate);
		if (markupTokenCount(tokens) > markupTokenCount(best)) best = tokens;
	}
	if (markupTokenCount(best) === 0) throw new Error('Invalid XML');

	const root = buildTolerantTree(best);
	const parts: string[] = [];
	for (const child of root) {
		// Markup constructs such as `<?xml?>` keep their place in the tolerant layout.
		const serialized = serializeVirtual(child, 0, pad, false).trim();
		if (serialized) parts.push(serialized);
	}
	return parts.join('\n');
}

export function formatContent(mode: Mode, source: string, options: FormatOptions): string {
	if (!source.trim()) return '';
	const indent = Math.max(1, Math.min(8, Math.round(options.indent) || 2));

	if (mode === 'json') {
		if (options.doFormat) return formatJsonText(source, indent, options.doUnescape);
		if (options.doUnescape) return unescapeJson(source);
		return source;
	}

	if (options.doFormat) return formatXmlText(source, indent, options.doUnescape);
	if (options.doUnescape) return decodeXmlEntities(source);
	return source;
}

export function escapeContent(mode: Mode, source: string): string {
	return mode === 'json' ? escapeJson(source) : escapeXml(source);
}

/**
 * Escapes every character outside the unreserved set, reserved ones included:
 * a URL copied out of another tool has to come back as the text that was put
 * in, so `http://host:3030&id=…` becomes `http%3A%2F%2Fhost%3A3030%26id%3D…`.
 * `encodeURI` leaves `:` `/` `?` `&` `=` as structure and never produces that.
 */
export function encodeUrl(input: string): string {
	try {
		return encodeURIComponent(input);
	} catch {
		return input;
	}
}

/**
 * Decodes the text in two steps, the order a link copied out of HTML arrives in:
 * markup entities first (`&amp;` → `&`), then percent-escapes (`%5C` → `\`).
 * Percent-escapes go second and entities are decoded once, so an entity that is
 * itself percent-encoded (`%26amp%3B`) stays the literal text `&amp;` — that is
 * what keeps this the exact inverse of `encodeUrl`. `decodeURI` cannot recover
 * the reserved characters, so a link copied from another tool would stay
 * `http%3A%2F%2Fhost%3A3030`.
 */
export function decodeUrl(input: string): string {
	const entities = decodeXmlEntities(input);
	try {
		return decodeURIComponent(entities);
	} catch {
		return entities;
	}
}
