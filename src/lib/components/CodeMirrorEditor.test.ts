import { mount, unmount } from 'svelte';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import CodeMirrorEditor from './CodeMirrorEditor.svelte';

function wait(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Style classes CodeMirror generates for highlight tags, as opposed to its own `cm-*` markup. */
function highlightClasses(line: Element | null): string[] {
	if (!line) return [];
	return [...line.querySelectorAll('span[class]')]
		.map((span) => span.getAttribute('class') ?? '')
		.filter((cls) => !cls.startsWith('cm-'));
}

function textOf(line: Element | null, wanted: string): string | null {
	const span = [...(line?.querySelectorAll('span[class]') ?? [])].find(
		(node) => node.textContent === wanted
	);
	return span?.getAttribute('class') ?? null;
}

describe('CodeMirrorEditor', () => {
	let target: HTMLDivElement;
	const originalClientRects = Range.prototype.getClientRects;
	const originalRect = Range.prototype.getBoundingClientRect;

	beforeAll(() => {
		// jsdom does not lay text out, so a Range has no client rectangles. CodeMirror's
		// measurement pass would throw on it; answering "no rectangles" keeps the console
		// clean and still renders the highlight spans this file asserts on.
		Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
		Range.prototype.getBoundingClientRect = () => new DOMRect();
	});

	afterAll(() => {
		Range.prototype.getClientRects = originalClientRects;
		Range.prototype.getBoundingClientRect = originalRect;
	});

	beforeEach(() => {
		target = document.createElement('div');
		target.style.height = '300px';
		document.body.appendChild(target);
	});

	afterEach(() => {
		target.remove();
	});

	it('parses markup with the HTML language', async () => {
		const component = mount(CodeMirrorEditor, {
			target,
			props: {
				value: '<html><body><p class="x">hi</p></body></html>',
				language: 'html',
				readonly: true
			}
		});

		await wait(150);
		const line = target.querySelector('.cm-line');
		// The HTML parser tags a tag name, an attribute name and an attribute value
		// differently, so the three get three distinct highlight styles.
		const tagName = textOf(line, 'html');
		const attributeName = textOf(line, 'class');
		const attributeValue = textOf(line, '"x"');
		expect(tagName).toBeTruthy();
		expect(attributeName).toBeTruthy();
		expect(attributeValue).toBeTruthy();
		expect(new Set([tagName, attributeName, attributeValue]).size).toBe(3);
		unmount(component);
	});

	it('keeps plain text unhighlighted', async () => {
		const component = mount(CodeMirrorEditor, {
			target,
			props: { value: 'hello world', language: 'text', readonly: true }
		});

		await wait(150);
		expect(highlightClasses(target.querySelector('.cm-line'))).toEqual([]);
		unmount(component);
	});
});
