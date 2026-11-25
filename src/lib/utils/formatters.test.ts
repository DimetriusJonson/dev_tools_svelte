import { describe, expect, it } from 'vitest';
import {
	decodeUrl,
	decodeXmlEntities,
	encodeUrl,
	escapeContent,
	escapeJson,
	escapeXmlText,
	formatContent,
	formatJsonText,
	formatXmlText,
	prettyJsonTolerant,
	unescapeJson
} from './formatters';

describe('JSON formatting', () => {
	const source = '{"userId":1042,"username":"alex_dev","roles":["admin","editor"],"isActive":true}';

	it('formats with 2, 3 and 4 space indentation', () => {
		expect(formatJsonText(source, 2)).toBe(
			'{\n  "userId": 1042,\n  "username": "alex_dev",\n  "roles": [\n    "admin",\n    "editor"\n  ],\n  "isActive": true\n}'
		);
		expect(formatJsonText(source, 3).split('\n')[1]).toBe('   "userId": 1042,');
		expect(formatJsonText(source, 4).split('\n')[1]).toBe('    "userId": 1042,');
	});

	it('keeps the original key order', () => {
		const formatted = formatJsonText('{"z":1,"a":2}', 2);
		expect(formatted.indexOf('"z"')).toBeLessThan(formatted.indexOf('"a"'));
	});

	it('unescapes escaped JSON payloads', () => {
		const escaped = '{\\"userId\\":1042,\\"roles\\":[\\"admin\\"]}';
		const formatted = formatJsonText(escaped, 2, true);
		expect(formatted).toContain('"userId": 1042');
		expect(formatted).toContain('"admin"');
	});

	it('parses a JSON string that contains JSON when unescape is enabled', () => {
		const payload = JSON.stringify('{"a":1}');
		const formatted = formatJsonText(payload, 2, true);
		expect(formatted).toBe('{\n  "a": 1\n}');
	});

	it('escapes and unescapes back to the original content', () => {
		const value = '{"a":"line\\nbreak","b":"quote \\"x\\""}';
		const escaped = escapeJson(value);
		expect(escaped).toContain('\\"a\\"');
		expect(unescapeJson(escaped)).toBe(value);
	});

	it('formats JSON with junk text before the document', () => {
		const messy = `aaa\n${source}`;
		expect(formatJsonText(messy, 2)).toBe(`aaa\n${formatJsonText(source, 2)}`);
		expect(
			formatContent('json', messy, { indent: 4, doFormat: true, doUnescape: true }).startsWith('aaa\n{')
		).toBe(true);
	});

	it('keeps junk as its own line when the body is broken too', () => {
		expect(formatJsonText('aaa\n{"a":1,,}', 2)).toBe('aaa\n{\n  "a": 1,\n  ,\n}');
		expect(formatJsonText('{"a":1}\nbbb', 2)).toBe('{\n  "a": 1\n}\nbbb');
	});

	it('re-indents JSON that does not parse instead of failing', () => {
		expect(formatJsonText('{invalid', 2)).toBe('{\n  invalid');
		expect(formatJsonText('{"a":1,}', 2)).toBe('{\n  "a": 1,\n}');
		expect(formatJsonText('[{"a":[1,2]', 2)).toBe('[\n  {\n    "a": [\n      1,\n      2\n    ]');
	});

	it('keeps strings and unquoted text verbatim when re-indenting', () => {
		expect(prettyJsonTolerant('{"a":"x, y" garbage}', 2)).toBe('{\n  "a": "x, y" garbage\n}');
		expect(prettyJsonTolerant('some: thing, other', 2)).toBe('some: thing,\nother');
	});

	it('unescapes and re-indents a broken escaped payload', () => {
		expect(formatJsonText('{\\"a\\":1,,}', 2, true)).toBe('{\n  "a": 1,\n  ,\n}');
	});

	it('re-indents the candidate with more structural characters', () => {
		expect(formatJsonText('{&quot;a}', 2, true)).toBe('{\n  "a}');
	});

	it('throws for text that has no JSON structure', () => {
		expect(() => formatJsonText('hello world', 2)).toThrow();
		expect(() => formatJsonText('<!DOCTYPE html><html>', 2)).toThrow();
		expect(() => formatJsonText('<html><body>{ a }</body></html>', 2)).toThrow();
		expect(() => formatJsonText('Error occurred in {module}', 2)).toThrow();
	});
});

describe('XML formatting', () => {
	const source =
		'<?xml version="1.0" encoding="UTF-8"?><catalog><book id="bk101"><author>Gambardella, Matthew</author><title>XML Developer\'s Guide</title></book></catalog>';

	it('formats XML with the requested indentation', () => {
		expect(formatXmlText(source, 2)).toBe(
			[
				'<?xml version="1.0" encoding="UTF-8"?>',
				'<catalog>',
				'  <book id="bk101">',
				'    <author>Gambardella, Matthew</author>',
				'    <title>XML Developer&apos;s Guide</title>',
				'  </book>',
				'</catalog>'
			].join('\n')
		);
		expect(formatXmlText(source, 4).split('\n')[1]).toBe('<catalog>');
		expect(formatXmlText(source, 4).split('\n')[2]).toBe('    <book id="bk101">');
	});

	it('formats escaped XML when unescape is enabled', () => {
		const escaped =
			'&lt;root&gt;&lt;a&gt;1&lt;/a&gt;&lt;b/&gt;&lt;/root&gt;';
		expect(formatXmlText(escaped, 2, true)).toBe('<root>\n  <a>1</a>\n  <b/>\n</root>');
	});

	it('preserves comments', () => {
		expect(formatXmlText('<root><!-- hello --><a>1</a></root>', 2)).toBe(
			'<root>\n  <!-- hello -->\n  <a>1</a>\n</root>'
		);
	});

	it('escapes XML entities', () => {
		expect(escapeXmlText('a < b & c > d "e" \'f\'')).toBe(
			'a &lt; b &amp; c &gt; d &quot;e&quot; &apos;f&apos;'
		);
		expect(decodeXmlEntities('&lt;root&gt;&amp;&#39;&#x41;')).toBe("<root>&'A");
	});

	it('re-indents malformed markup instead of failing', () => {
		expect(formatXmlText('<root><a></root>', 2)).toBe('<root>\n  <a/>\n</root>');
		expect(formatXmlText('<a><b><c>d', 2)).toBe('<a>\n  <b>\n    <c>d</c>\n  </b>\n</a>');
		expect(formatXmlText('<!DOCTYPE html><html><body>x</body>', 2)).toBe(
			'<!DOCTYPE html>\n<html>\n  <body>x</body>\n</html>'
		);
	});

	it('re-indents markup with junk before the declaration', () => {
		const messy = `aaa\n${source}`;
		expect(formatContent('xml', messy, { indent: 4, doFormat: true, doUnescape: true })).toBe(
			[
				'aaa',
				'<?xml version="1.0" encoding="UTF-8"?>',
				'<catalog>',
				'    <book id="bk101">',
				'        <author>Gambardella, Matthew</author>',
				"        <title>XML Developer's Guide</title>",
				'    </book>',
				'</catalog>'
			].join('\n')
		);
	});

	it('keeps source entities verbatim in tolerant mode', () => {
		expect(formatXmlText('<a title="x &amp; y">a &lt; b</a><oops>', 2)).toBe(
			'<a title="x &amp; y">a &lt; b</a>\n<oops/>'
		);
	});

	it('keeps a stray closing tag as content', () => {
		expect(formatXmlText('<a><b></c></a>', 2)).toBe('<a>\n  <b>\n    </c>\n  </b>\n</a>');
	});

	it('unescapes malformed markup only when the unescaped form has markup', () => {
		expect(formatXmlText('&lt;root&gt;&lt;a&gt;1', 2, true)).toBe('<root>\n  <a>1</a>\n</root>');
		expect(() => formatXmlText('&lt;root&gt;&lt;a&gt;1', 2, false)).toThrow();
	});

	it('throws for text that contains no markup', () => {
		expect(() => formatXmlText('hello world', 2)).toThrow();
		expect(() => formatContent('xml', 'hello', { indent: 2, doFormat: true, doUnescape: true })).toThrow();
	});
});

describe('formatContent', () => {
	it('returns the source for empty input', () => {
		expect(formatContent('json', '   ', { indent: 2, doFormat: true, doUnescape: true })).toBe('');
	});

	it('respects disabled format and unescape options', () => {
		const source = '{"a":1}';
		expect(formatContent('json', source, { indent: 2, doFormat: false, doUnescape: false })).toBe(
			source
		);
	});

	it('escapes JSON and XML content', () => {
		expect(escapeContent('json', '{"a":1}')).toBe('{\\"a\\":1}');
		expect(escapeContent('xml', '<a>1</a>')).toBe('&lt;a&gt;1&lt;/a&gt;');
	});
});

describe('URL encoding', () => {
	it('encodes every reserved character and decodes back', () => {
		const source = 'https://example.com/a b?x=1&y=привет';
		const encoded = encodeUrl(source);
		expect(encoded).toBe(
			'https%3A%2F%2Fexample.com%2Fa%20b%3Fx%3D1%26y%3D%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82'
		);
		expect(decodeUrl(encoded)).toBe(source);
	});

	it('encodes the characters a deep link is built from', () => {
		expect(encodeUrl('http://10.7.6.86:3030&id=8f82124b-cd30-42ed-84a6-1b8567810614')).toBe(
			'http%3A%2F%2F10.7.6.86%3A3030%26id%3D8f82124b-cd30-42ed-84a6-1b8567810614'
		);
	});

	it('round-trips text that already holds entity text', () => {
		const source = 'https://example.com/?a=&amp;b=%5C';
		expect(decodeUrl(encodeUrl(source))).toBe(source);
	});

	it('decodes reserved characters', () => {
		expect(decodeUrl('http%3A%2F%2F10.7.6.86%3A3030&id=8f82124b-cd30-42ed-84a6-1b8567810614')).toBe(
			'http://10.7.6.86:3030&id=8f82124b-cd30-42ed-84a6-1b8567810614'
		);
		expect(decodeUrl('a%20b%2Fc%3Fd%3De')).toBe('a b/c?d=e');
	});

	it('unescapes markup entities before percent-escapes', () => {
		const htmlUrl =
			'https://example.com/workplace/result?format=pdf&amp;is_empty=1&amp;report=Agreement.fr3&amp;Agreement_ID=0&amp;Agency_ID=11472&amp;Contract_Season_Name=2026&amp;Contract_Number=11472%5C2026%5C1&amp;filename=StatementOfAcceptance.pdf';
		expect(decodeUrl(htmlUrl)).toBe(
			'https://example.com/workplace/result?format=pdf&is_empty=1&report=Agreement.fr3&Agreement_ID=0&Agency_ID=11472&Contract_Season_Name=2026&Contract_Number=11472\\2026\\1&filename=StatementOfAcceptance.pdf'
		);
		expect(decodeUrl('a%3D1&amp;b=%5C2')).toBe('a=1&b=\\2');
		expect(decodeUrl('&lt;p&gt;%5Cn')).toBe('<p>\\n');
	});

	it('keeps a percent-encoded entity as the literal text it encodes', () => {
		expect(decodeUrl('%26amp%3B')).toBe('&amp;');
		expect(decodeUrl('x=%26nbsp%3B')).toBe('x=&nbsp;');
		expect(decodeUrl('&amp;#92;')).toBe('&#92;');
	});

	it('keeps decoded entities when the percent sequence is malformed', () => {
		expect(decodeUrl('&amp;100%')).toBe('&100%');
	});

	it('returns input unchanged for malformed sequences', () => {
		expect(decodeUrl('%E0%A4%A')).toBe('%E0%A4%A');
		expect(decodeUrl('100%')).toBe('100%');
	});

	it('returns input unchanged when it cannot be encoded', () => {
		expect(encodeUrl('a\uD800b')).toBe('a\uD800b');
	});
});

describe('code points that must stay literal', () => {
	it('leaves an out-of-range or surrogate entity as its own text', () => {
		expect(decodeXmlEntities('&#xD800; &#x110000; &#xD800;&#xDC00; &#x41;')).toBe('&#xD800; &#x110000; &#xD800;&#xDC00; A');
	});

	it('decodes valid escapes but keeps a lone surrogate', () => {
		expect(unescapeJson('a\\uD800b \\x41 \\u041B \\uD83D')).toBe('a\\uD800b A Л \\uD83D');
	});

	it('formats markup and JSON whose only oddity is a broken code point', () => {
		expect(formatXmlText('<r>&#xD800;</r>', 2, true)).toBe('<r>&#xD800;</r>');
		// The strict pass wins for a lone surrogate and re-escapes it as its own text.
		expect(formatJsonText('{"a":"\\uD800"}', 2)).toBe('{\n  "a": "\\ud800"\n}');
	});
});
