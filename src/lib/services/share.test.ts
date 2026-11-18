import { describe, expect, it } from 'vitest';
import { buildViewerUrl, formatBytes, MAX_UPLOAD_BYTES } from './share';

describe('formatBytes', () => {
	it('formats bytes, kilobytes, and megabytes', () => {
		expect(formatBytes(512)).toBe('512 B');
		expect(formatBytes(1024)).toBe('1.0 KB');
		expect(formatBytes(1536)).toBe('1.5 KB');
		expect(formatBytes(MAX_UPLOAD_BYTES)).toBe('50.0 MB');
	});
});

describe('buildViewerUrl', () => {
	it('encodes server and id as query parameters', () => {
		const url = buildViewerUrl({
			server: 'http://192.168.1.10:3030',
			id: '550e8400-e29b-41d4-a716-446655440000'
		});
		const parsed = new URL(url, 'http://localhost');
		expect(parsed.pathname).toBe('/view');
		expect(parsed.searchParams.get('server')).toBe('http://192.168.1.10:3030');
		expect(parsed.searchParams.get('id')).toBe('550e8400-e29b-41d4-a716-446655440000');
	});
});
