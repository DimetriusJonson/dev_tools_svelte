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

export function decodeXmlEntities(input: string): string {
	return input.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*);/g, (match, body: string) => {
		if (body.startsWith('#x') || body.startsWith('#X')) {
			const code = Number.parseInt(body.slice(2), 16);
			return Number.isFinite(code) ? String.fromCodePoint(code) : match;
		}
		if (body.startsWith('#')) {
			const code = Number.parseInt(body.slice(1), 10);
			return Number.isFinite(code) ? String.fromCodePoint(code) : match;
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
			return Number.isFinite(code) ? String.fromCodePoint(code) : match;
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
	const value = parseJsonSmart(source, doUnescape);
	return JSON.stringify(value, null, indent);
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

function serializeNode(node: Node, depth: number, pad: string, block: boolean): string {
	const indent = pad.repeat(depth);

	if (node.nodeType === Node.TEXT_NODE) {
		const text = node.textContent ?? '';
		if (!text.trim()) return block ? '' : text;
		return indent + escapeXmlText(text.trim());
	}

	if (node.nodeType === Node.CDATA_SECTION_NODE) {
		return indent + `<![CDATA[${node.textContent ?? ''}]]>`;
	}

	if (node.nodeType === Node.COMMENT_NODE) {
		return indent + `<!--${node.textContent ?? ''}-->`;
	}

	if (node.nodeType === Node.PROCESSING_INSTRUCTION_NODE) {
		const pi = node as ProcessingInstruction;
		return indent + `<?${pi.target}${pi.data ? ` ${pi.data}` : ''}?>`;
	}

	if (node.nodeType === Node.DOCUMENT_TYPE_NODE) {
		const doctype = node as DocumentType;
		let value = `<!DOCTYPE ${doctype.name}`;
		if (doctype.publicId) value += ` PUBLIC "${doctype.publicId}"`;
		if (doctype.systemId) value += `${doctype.publicId ? '' : ' SYSTEM'} "${doctype.systemId}"`;
		return indent + value + '>';
	}

	if (node.nodeType === Node.ELEMENT_NODE) {
		const element = node as Element;
		const open = `<${element.tagName}${serializeAttributes(element)}`;
		const children = Array.from(element.childNodes).filter((child) => {
			if (child.nodeType === Node.TEXT_NODE) return (child.textContent ?? '').trim().length > 0;
			return true;
		});

		if (children.length === 0) {
			return `${indent}${open}/>`;
		}

		const hasElementChild = children.some(
			(child) => child.nodeType === Node.ELEMENT_NODE || child.nodeType === Node.COMMENT_NODE
		);

		if (!hasElementChild) {
			const inner = children
				.map((child) => {
					if (child.nodeType === Node.CDATA_SECTION_NODE) {
						return `<![CDATA[${child.textContent ?? ''}]]>`;
					}
					if (child.nodeType === Node.PROCESSING_INSTRUCTION_NODE) {
						const pi = child as ProcessingInstruction;
						return `<?${pi.target}${pi.data ? ` ${pi.data}` : ''}?>`;
					}
					return escapeXmlText((child.textContent ?? '').trim());
				})
				.join('');
			return `${indent}${open}>${inner}</${element.tagName}>`;
		}

		const inner = children
			.map((child) => serializeNode(child, depth + 1, pad, true))
			.filter((value) => value.length > 0)
			.join('\n');

		return `${indent}${open}>\n${inner}\n${indent}</${element.tagName}>`;
	}

	return '';
}

export function formatXmlText(source: string, indent: number, doUnescape = false): string {
	const parser = new DOMParser();
	const pad = ' '.repeat(indent);
	let text = source;
	let doc = parser.parseFromString(text, 'application/xml');

	if (xmlParseError(doc) && doUnescape) {
		text = decodeXmlEntities(source);
		doc = parser.parseFromString(text, 'application/xml');
	}

	if (xmlParseError(doc)) {
		throw new Error('Invalid XML');
	}

	const declaration = extractDeclaration(text);
	const parts: string[] = [];

	for (const node of Array.from(doc.childNodes)) {
		if (node.nodeType === Node.PROCESSING_INSTRUCTION_NODE) {
			const pi = node as ProcessingInstruction;
			if (pi.target.toLowerCase() === 'xml') continue;
		}
		const serialized = serializeNode(node, 0, pad, false).trim();
		if (serialized) parts.push(serialized);
	}

	const body = parts.join('\n');
	if (declaration) {
		return body ? `${declaration}\n${body}` : declaration;
	}
	return body;
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

export function encodeUrl(input: string): string {
	try {
		return encodeURI(input);
	} catch {
		return input;
	}
}

export function decodeUrl(input: string): string {
	try {
		return decodeURI(input);
	} catch {
		return input;
	}
}
