import { describe, expect, it } from 'vitest';
import { basename, formattedPath, sharedFileName } from './files';

describe('path helpers behind the file dialogs', () => {
	it('reads the picked name across both path separators', () => {
		expect(basename('C:\\temp\\Report.pdf')).toBe('Report.pdf');
		expect(basename('/tmp/a.txt')).toBe('a.txt');
		expect(basename('report.json')).toBe('report.json');
	});

	it('reduces a server-supplied name to one safe file name', () => {
		// A disposition can carry a path or a quote; neither may reach the save dialog.
		expect(sharedFileName('C:\\temp\\Report.pdf')).toBe('Report.pdf');
		expect(sharedFileName('../../secret.xml')).toBe('secret.xml');
		expect(sharedFileName('a"b\u0001c.txt')).toBe('a_b_c.txt');
		expect(sharedFileName('  spaced  ')).toBe('spaced');
	});

	it('names the output file next to the source', () => {
		expect(formattedPath('/tmp/a.xml', 'xml')).toBe('/tmp/a.formatted.xml');
		expect(formattedPath('/tmp/a', 'json')).toBe('/tmp/a.formatted.json');
		expect(formattedPath('/tmp/a.XML', 'xml')).toBe('/tmp/a.formatted.xml');
		expect(formattedPath('/tmp/a.json', 'xml')).toBe('/tmp/a.json.formatted.xml');
	});
});
