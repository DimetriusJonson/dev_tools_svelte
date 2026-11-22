import { beforeEach, describe, expect, it } from 'vitest';
import {
	clearWsdlResponse,
	fileOperations,
	firstOperation,
	getWsdlOperation,
	isWsdlSupported,
	listWsdlFiles,
	loadWsdlSelection,
	resolveWsdlSelection,
	saveWsdlSelection,
	soapVersionFromWire,
	type WsdlFileSummary,
	type WsdlOperation,
	type WsdlProject
} from './wsdl';

function project(id: string, name: string): WsdlProject {
	return { id, name, created_at: '2026-01-01 00:00:00' };
}

function operation(id: string): WsdlOperation {
	return { id, name: id, soap_version: 'soap11', soap_action: '', endpoint_count: 1 };
}

function file(
	id: string,
	projectId: string,
	operationsByService: [string, WsdlOperation[]][]
): WsdlFileSummary {
	return {
		id,
		project_id: projectId,
		name: `${id}.wsdl`,
		source_url: `https://example.com/${id}.wsdl`,
		warnings: [],
		services: operationsByService.map(([name, operations]) => ({ id: `${id}-${name}`, name, operations })),
		imported_at: '2026-01-01 00:00:00'
	};
}

const projects = [project('p1', 'Alpha'), project('p2', 'Beta')];
const files = [
	file('f1', 'p1', [
		['Ping', [operation('o1'), operation('o2')]],
		['Echo', [operation('o3')]]
	]),
	file('f2', 'p2', [['Orders', [operation('o4')]]]),
	file('f3', '', [['Free', [operation('o5')]]])
];

describe('WSDL tree helpers', () => {
	it('reads the version the wire spells as the client union', () => {
		expect(soapVersionFromWire('1.2')).toBe('soap12');
		expect(soapVersionFromWire('1.1')).toBe('soap11');
		// A document that spells the version differently still binds SOAP 1.1.
		expect(soapVersionFromWire('')).toBe('soap11');
	});

	it('walks operations in the order the tree shows them', () => {
		expect(fileOperations(files[0]).map((item) => item.id)).toEqual(['o1', 'o2', 'o3']);
		expect(firstOperation(files[0])?.id).toBe('o1');
	});

	it('reports no operation for a file whose services are empty', () => {
		const empty = file('f4', 'p1', []);
		expect(firstOperation(empty)).toBeNull();
		expect(fileOperations(empty)).toEqual([]);
	});
});

describe('WSDL selection restore', () => {
	it('reopens on the stored project and operation', () => {
		expect(resolveWsdlSelection(projects, files, { project_id: 'p2', operation_id: 'o4' })).toEqual({
			project_id: 'p2',
			operation_id: 'o4'
		});
	});

	it('keeps the no-project view when that is what was open', () => {
		expect(resolveWsdlSelection(projects, files, { project_id: null, operation_id: 'o5' })).toEqual({
			project_id: null,
			operation_id: 'o5'
		});
	});

	it('falls back to the first operation of the stored project when the operation is gone', () => {
		expect(resolveWsdlSelection(projects, files, { project_id: 'p2', operation_id: 'gone' })).toEqual({
			project_id: 'p2',
			operation_id: 'o4'
		});
	});

	it('drops a deleted project and opens the first operation that is left', () => {
		// Deleting a project deletes the files it owns, so nothing of that view survives.
		const afterDelete = files.filter((item) => item.project_id !== 'p2');
		expect(resolveWsdlSelection([projects[0]], afterDelete, { project_id: 'p2', operation_id: 'o4' })).toEqual(
			{ project_id: null, operation_id: 'o1' }
		);
	});

	it('follows an operation that no longer belongs to the stored project', () => {
		// Re-importing the file into another project moves the row: the tree shows its owner.
		const moved = [file('f1', 'p2', [['Ping', [operation('o1'), operation('o2')]]])];
		expect(resolveWsdlSelection(projects, moved, { project_id: 'p1', operation_id: 'o2' })).toEqual({
			project_id: 'p2',
			operation_id: 'o2'
		});
	});

	it('opens the first visible operation when the saved selection names none', () => {
		expect(resolveWsdlSelection(projects, files, { project_id: 'p1', operation_id: null })).toEqual({
			project_id: 'p1',
			operation_id: 'o1'
		});
		expect(resolveWsdlSelection(projects, files, { project_id: null, operation_id: null })).toEqual({
			project_id: null,
			operation_id: 'o1'
		});
	});

	it('leaves the selection empty when there are no files', () => {
		expect(resolveWsdlSelection(projects, [], { project_id: 'p1', operation_id: 'o1' })).toEqual({
			project_id: 'p1',
			operation_id: null
		});
	});
});

describe('WSDL selection persistence', () => {
	beforeEach(() => {
		window.localStorage.clear();
	});

	it('round trips a selection through browser storage', async () => {
		await saveWsdlSelection({ project_id: 'p1', operation_id: 'o1' });
		expect(await loadWsdlSelection()).toEqual({ project_id: 'p1', operation_id: 'o1' });
		expect(window.localStorage.getItem('devtools.setting.wsdl.activeProject')).toBe('"p1"');
	});

	it('reads back an empty selection as no project and no operation', async () => {
		await saveWsdlSelection({ project_id: 'p1', operation_id: 'o1' });
		await saveWsdlSelection({ project_id: null, operation_id: null });
		expect(await loadWsdlSelection()).toEqual({ project_id: null, operation_id: null });
	});

	it('returns no selection when nothing was stored', async () => {
		expect(await loadWsdlSelection()).toEqual({ project_id: null, operation_id: null });
	});
});

describe('WSDL service outside the desktop runtime', () => {
	beforeEach(() => {
		window.localStorage.clear();
	});

	it('reports the feature as unsupported in a browser', () => {
		expect(isWsdlSupported()).toBe(false);
	});

	it('refuses to read the tree without the backend', async () => {
		await expect(listWsdlFiles()).rejects.toThrow('desktop app only');
		await expect(getWsdlOperation('o1')).rejects.toThrow('desktop app only');
		await expect(clearWsdlResponse('o1')).rejects.toThrow('desktop app only');
	});

	it('still stores the selection so the browser page keeps its own state', async () => {
		await saveWsdlSelection({ project_id: 'p1', operation_id: 'o1' });
		expect(await loadWsdlSelection()).toEqual({ project_id: 'p1', operation_id: 'o1' });
	});
});
