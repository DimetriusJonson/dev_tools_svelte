import { beforeEach, describe, expect, it } from 'vitest';
import {
	buildViewerUrl,
	deleteSharedUpload,
	formatBytes,
	loadRetention,
	normalizeRetention,
	RETENTION_OPTIONS,
	RETENTION_SETTING_KEY,
	saveRetention,
	DEFAULT_RETENTION,
	MAX_UPLOAD_BYTES
} from './share';

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

	it('carries no secret: anyone following the link can open and delete the upload', () => {
		const url = buildViewerUrl({ server: 'http://127.0.0.1:3030', id: '550e8400-e29b-41d4-a716-446655440000' });
		expect(url).not.toMatch(/token/i);
	});
});

describe('retention', () => {
	beforeEach(() => {
		window.localStorage.clear();
	});

	it('offers every option the backend parses', () => {
		expect(RETENTION_OPTIONS).toEqual(['none', '1d', '3d', '7d', 'always']);
	});

	it('normalizes typing and casing into a retention code', () => {
		expect(normalizeRetention(' 3D ')).toBe('3d');
		expect(normalizeRetention('none')).toBe('none');
	});

	it('starts with the memory-only option when nothing was chosen yet', async () => {
		expect(DEFAULT_RETENTION).toBe('none');
		expect(await loadRetention()).toBe('none');
	});

	it('falls back to not storing anything for anything unknown', () => {
		expect(normalizeRetention('30d')).toBe('none');
		expect(normalizeRetention('')).toBe('none');
		expect(normalizeRetention(null)).toBe('none');
		expect(normalizeRetention(7)).toBe('none');
	});

	it('round trips the choice through the settings storage', async () => {
		await saveRetention('7d');
		expect(await loadRetention()).toBe('7d');
		expect(window.localStorage.getItem(`devtools.setting.${RETENTION_SETTING_KEY}`)).toBe('"7d"');
	});

	it('answers an unknown stored value with the memory-only default', async () => {
		window.localStorage.setItem(`devtools.setting.${RETENTION_SETTING_KEY}`, '"soon"');
		expect(await loadRetention()).toBe('none');
	});
});

describe('deleteSharedUpload', () => {
	it('is desktop-only like every other share call', async () => {
		await expect(deleteSharedUpload('http://127.0.0.1:3030', 'x')).rejects.toThrow(/desktop app only/);
	});
});
