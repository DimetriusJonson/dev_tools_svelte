import { flushSync, mount, unmount } from 'svelte';
import { get } from 'svelte/store';
import { beforeAll, afterAll, afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import XmlJsonFormatter from './XmlJsonFormatter.svelte';
import { draftShare } from '$lib/stores/share';
import { formatContent } from '$lib/utils/formatters';
import { setLocale } from '$lib/stores/i18n';

vi.mock('$app/navigation', () => ({
	goto: vi.fn(async () => {})
}));

vi.mock('$lib/services/db', async () => {
	const actual = await vi.importActual<typeof import('$lib/services/db')>('$lib/services/db');
	return {
		...actual,
		isTauriRuntime: () => false,
		loadEditorState: async () => null,
		saveEditorState: async () => {},
		processLargeFile: async () => ({ output_path: '', bytes_in: 0, bytes_out: 0 })
	};
});

vi.mock('$lib/services/files', async () => ({
	basename: () => 'file.xml',
	formattedPath: () => 'result.xml',
	pickInputFile: async () => null,
	pickOutputFile: async () => null,
	sharedFileName: (name: string) => name,
	pickPartFile: async () => null
}));

vi.mock('$lib/utils/clipboard', () => ({
	copyText: async () => {}
}));

/** jsdom lays out no text; CodeMirror's measure pass needs these to answer "no rectangles". */
const originalClientRects = Range.prototype.getClientRects;
const originalBoundingRect = Range.prototype.getBoundingClientRect;

beforeAll(() => {
	Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
	Range.prototype.getBoundingClientRect = () => new DOMRect();
});

afterAll(() => {
	Range.prototype.getClientRects = originalClientRects;
	Range.prototype.getBoundingClientRect = originalBoundingRect;
});

function stubMatchMedia(): void {
	vi.stubGlobal(
		'matchMedia',
		vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }))
	);
}

function render(mode: 'xml' | 'json'): ReturnType<typeof mount> {
	return mount(XmlJsonFormatter, { target: document.body, props: { mode } });
}

beforeEach(() => {
	setLocale('en');
	stubMatchMedia();
	localStorage.clear();
	draftShare.set(null);
	vi.useFakeTimers();
});

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
	localStorage.clear();
});

describe('XmlJsonFormatter', () => {
	it('formats through execute and hands the result to the Share page as a read-only draft', async () => {
		const navigation = await import('$app/navigation');
		const goto = navigation.goto as ReturnType<typeof vi.fn>;
		const component = render('xml');
		const instance = component as unknown as {
			setSource: (value: string) => void;
			execute: () => void;
			shareOutput: () => void;
		};

		instance.setSource('<root/>');
		instance.execute();
		flushSync();

		const expected = formatContent('xml', '<root/>', { indent: 4, doFormat: true, doUnescape: true });

		instance.shareOutput();
		flushSync();

		const draft = get(draftShare);
		expect(draft).not.toBeNull();
		expect(draft?.content).toBe(expected);
		expect(draft?.filename).toBe('snippet.xml');
		expect(draft?.contentType).toBe('application/xml');
		expect(draft?.language).toBe('xml');
		expect(draft?.readOnly).toBe(true);
		expect(goto).toHaveBeenCalledWith('/share');

		unmount(component);
	});

	it('downloads in the browser through an anchor that is removed from the body', async () => {
		const createObjectURL = vi.fn(() => 'blob:http://localhost:5173/result.json');
		const revokeObjectURL = vi.fn();
		vi.stubGlobal('URL', { createObjectURL, revokeObjectURL } as unknown as typeof URL);

		const component = render('json');
		const instance = component as unknown as {
			setSource: (value: string) => void;
			execute: () => void;
			downloadOutput: () => Promise<void>;
		};

		instance.setSource('{"a":1}');
		instance.execute();
		flushSync();

		await instance.downloadOutput();
		flushSync();

		expect(createObjectURL).toHaveBeenCalled();
		// The anchor must not stay attached after the click, unlike the legacy path.
		expect([...document.body.children].filter((node) => node.tagName === 'A')).toHaveLength(0);
		expect(revokeObjectURL).toHaveBeenCalled();

		unmount(component);
	});
});