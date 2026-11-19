import { beforeEach, describe, expect, it } from 'vitest';
import {
	loadRestSelection,
	resolveRestSelection,
	saveRestSelection,
	type RestProject,
	type RestRequest
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
