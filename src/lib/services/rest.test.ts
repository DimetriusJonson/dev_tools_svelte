import { beforeEach, describe, expect, it } from 'vitest';
import {
	createRestRequest,
	duplicateRestRequest,
	listRestRequests,
	loadRestSelection,
	normalizeUrl,
	resolveRestSelection,
	saveRestSelection,
	updateRestRequest,
	type RestProject,
	type RestRequest,
	type RestRequestDraft
} from './rest';

function project(id: string, name: string): RestProject {
	return { id, name, created_at: '2026-01-01 00:00:00' };
}

function request(id: string, projectId: string | null): RestRequest {
	return {
		id,
		project_id: projectId,
		name: id,
		method: 'GET',
		url: `https://example.com/${id}`,
		body: '',
		body_type: 'json',
		save_response: false,
		headers: [],
		form: [],
		multipart: [],
		files: [],
		response_body: null,
		response_headers: [],
		raw_request: null,
		created_at: '2026-01-01 00:00:00',
		updated_at: '2026-01-01 00:00:00'
	};
}

describe('REST selection restore', () => {
	const projects = [project('p1', 'Alpha'), project('p2', 'Beta')];
	const requests = [request('r1', 'p1'), request('r2', 'p1'), request('r3', 'p2')];

	it('reopens on the stored project and request', () => {
		expect(resolveRestSelection(projects, requests, { project_id: 'p2', request_id: 'r3' })).toEqual({
			project_id: 'p2',
			request_id: 'r3'
		});
	});

	it('keeps the no-project view when that is what was open', () => {
		expect(resolveRestSelection(projects, requests, { project_id: null, request_id: 'r3' })).toEqual(
			{ project_id: null, request_id: 'r3' }
		);
	});

	it('falls back to the first request of the stored project when the request is gone', () => {
		expect(resolveRestSelection(projects, requests, { project_id: 'p2', request_id: 'gone' })).toEqual(
			{ project_id: 'p2', request_id: 'r3' }
		);
	});

	it('drops a project that was deleted and keeps an unassigned request', () => {
		// Deleting a project unassigns its requests, so the stored request now has no project.
		const afterDelete = requests.map((item) =>
			item.project_id === 'p2' ? { ...item, project_id: null } : item
		);
		expect(resolveRestSelection([projects[0]], afterDelete, { project_id: 'p2', request_id: 'r3' })).toEqual(
			{ project_id: null, request_id: 'r3' }
		);
	});

	it('follows a request that no longer belongs to the stored project', () => {
		expect(resolveRestSelection(projects, requests, { project_id: 'p1', request_id: 'r3' })).toEqual({
			project_id: 'p2',
			request_id: 'r3'
		});
	});

	it('leaves the selection empty when there are no requests', () => {
		expect(resolveRestSelection(projects, [], { project_id: 'p1', request_id: 'r1' })).toEqual({
			project_id: 'p1',
			request_id: null
		});
	});
});

describe('REST selection persistence', () => {
	beforeEach(() => {
		window.localStorage.clear();
	});

	it('round trips a selection through browser storage', async () => {
		await saveRestSelection({ project_id: 'p1', request_id: 'r1' });
		expect(await loadRestSelection()).toEqual({ project_id: 'p1', request_id: 'r1' });
		expect(window.localStorage.getItem('devtools.setting.rest.activeProject')).toBe('"p1"');
	});

	it('reads back an empty selection as no project and no request', async () => {
		await saveRestSelection({ project_id: 'p1', request_id: 'r1' });
		await saveRestSelection({ project_id: null, request_id: null });
		expect(await loadRestSelection()).toEqual({ project_id: null, request_id: null });
	});

	it('returns no selection when nothing was stored', async () => {
		expect(await loadRestSelection()).toEqual({ project_id: null, request_id: null });
	});
});

describe('REST multipart storage in the browser build', () => {
	beforeEach(() => {
		window.localStorage.clear();
	});

	function draft(overrides: Partial<RestRequestDraft> = {}): RestRequestDraft {
		return {
			project_id: null,
			name: 'Upload',
			method: 'POST',
			url: 'https://example.com/upload',
			body: '{"buffer":1}',
			body_type: 'multipart',
			save_response: false,
			headers: [],
			form: [],
			multipart: [{ name: 'note', value: 'hi' }],
			files: [{ name: 'doc', path: '', filename: 'a.txt', file: new File(['x'], 'live.txt') }],
			...overrides
		};
	}

	it('saves the Form tables and keeps the File out of storage', async () => {
		await createRestRequest(draft());
		const [saved] = await listRestRequests();

		expect(saved.body_type).toBe('multipart');
		// Switching to the Form tab never destroys the text buffer.
		expect(saved.body).toBe('{"buffer":1}');
		expect(saved.multipart).toEqual([{ name: 'note', value: 'hi' }]);
		expect(saved.files).toEqual([{ name: 'doc', path: '', filename: 'a.txt' }]);

		const raw = window.localStorage.getItem('devtools.rest') ?? '';
		expect(raw).not.toContain('"file"');
	});

	it('reads back a request stored before the Form tab existed', async () => {
		window.localStorage.setItem(
			'devtools.rest',
			JSON.stringify({
				projects: [],
				requests: [
					{
						id: 'r1',
						name: 'Old',
						method: 'GET',
						url: 'https://example.com/',
						body: '',
						body_type: 'text',
						save_response: false,
						headers: [{ name: 'Accept', value: '*/*' }],
						form: [],
						response_body: null,
						response_headers: [],
						raw_request: null,
						created_at: '2026-01-01 00:00:00',
						updated_at: '2026-01-01 00:00:00'
					}
				]
			})
		);
		const [legacy] = await listRestRequests();
		// A request written by an older build has neither table: both read as empty.
		expect(legacy.multipart).toEqual([]);
		expect(legacy.files).toEqual([]);
		expect(legacy.headers).toEqual([{ name: 'Accept', value: '*/*' }]);
	});

	it('writes every file row through the same stripping when the request is edited', async () => {
		const created = await createRestRequest(draft());
		await updateRestRequest({
			...created,
			multipart: [{ name: 'note', value: 'edited' }],
			files: [{ name: 'doc', path: '/tmp/a.txt', filename: 'a.txt', file: new File(['x'], 'live.txt') }]
		});
		const [saved] = await listRestRequests();
		expect(saved.multipart).toEqual([{ name: 'note', value: 'edited' }]);
		expect(saved.files).toEqual([{ name: 'doc', path: '/tmp/a.txt', filename: 'a.txt' }]);
		expect(window.localStorage.getItem('devtools.rest')).not.toContain('"file"');
	});

	it('copies the Form tables and keeps storage free of File objects', async () => {
		const created = await createRestRequest(draft());
		const copy = await duplicateRestRequest(created.id);

		expect(copy.id).not.toBe(created.id);
		expect(copy.multipart).toEqual([{ name: 'note', value: 'hi' }]);
		expect(copy.files).toEqual([{ name: 'doc', path: '', filename: 'a.txt' }]);
		// The copy is added next to the source, and both stored rows carry path data only.
		const stored = await listRestRequests();
		expect(stored.map((request) => request.id)).toEqual([created.id, copy.id]);
		expect(window.localStorage.getItem('devtools.rest')).not.toContain('"file"');
	});
});

describe('selection with no stored request', () => {
	const projects = [project('p1', 'Alpha'), project('p2', 'Beta')];
	const requests = [request('r1', 'p1'), request('r2', 'p1'), request('r3', 'p2')];

	it('opens the first visible row when the saved selection names no request', () => {
		expect(resolveRestSelection(projects, requests, { project_id: 'p1', request_id: null })).toEqual({ project_id: 'p1', request_id: 'r1' });
		expect(resolveRestSelection(projects, requests, { project_id: null, request_id: null })).toEqual({ project_id: null, request_id: 'r1' });
		expect(resolveRestSelection(projects, requests, { project_id: 'p2', request_id: null })).toEqual({ project_id: 'p2', request_id: 'r3' });
	});

	it('reopens the same closed view after a restart', async () => {
		await saveRestSelection({ project_id: null, request_id: null });
		expect(await loadRestSelection()).toEqual({ project_id: null, request_id: null });
		const restored = await loadRestSelection();
		expect(resolveRestSelection(projects, requests, restored)).toEqual({ project_id: null, request_id: 'r1' });
	});
});

describe('request url shape', () => {
	it('adds the protocol only to a URL that has none', () => {
		expect(normalizeUrl('https://api.example/x').protocol).toBe('https:');
		expect(normalizeUrl(' api.example/x ').protocol).toBe('http:');
	});

	it('keeps a protocol-looking query when the URL itself has no scheme', () => {
		const url = normalizeUrl('x?next=http://other.example');
		expect(url.protocol).toBe('http:');
		expect(url.search).toBe('?next=http://other.example');
	});

	it('rejects a URL that the client cannot send', () => {
		expect(() => normalizeUrl('ftp://api.example/x')).toThrow();
		expect(() => normalizeUrl('   ')).toThrow();
		expect(() => normalizeUrl('?next=http://other.example')).toThrow();
	});
});

