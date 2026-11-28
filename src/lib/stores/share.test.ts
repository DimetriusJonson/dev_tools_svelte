import { get } from 'svelte/store';
import { describe, expect, it } from 'vitest';
import { draftShare } from './share';

describe('draftShare store', () => {
	it('starts empty', () => {
		expect(get(draftShare)).toBeNull();
	});

	it('carries the shape the Share page restores (content, filename, contentType, language, readOnly)', () => {
		draftShare.set({
			content: '<root/>',
			filename: 'snippet.xml',
			contentType: 'application/xml',
			language: 'xml',
			readOnly: true
		});

		const draft = get(draftShare);
		expect(draft).not.toBeNull();
		expect(draft?.content).toBe('<root/>');
		expect(draft?.filename).toBe('snippet.xml');
		expect(draft?.contentType).toBe('application/xml');
		expect(draft?.language).toBe('xml');
		expect(draft?.readOnly).toBe(true);

		draftShare.set(null);
		expect(get(draftShare)).toBeNull();
	});

	it('defaults readOnly to absent (mutable draft) when the caller omits it', () => {
		draftShare.set({ content: '{}', filename: 'snippet.json', contentType: 'application/json', language: 'json' });
		expect(get(draftShare)?.readOnly).toBeUndefined();
		draftShare.set(null);
	});
});