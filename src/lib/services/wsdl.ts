import { getSetting, invokeCommand, isTauriRuntime, setSetting } from './db';
import type { RestProperty } from '$lib/utils/rest';
import type { SoapVersion } from '$lib/utils/wsdl';

export type { RestProperty, SoapVersion };

export interface WsdlProject {
	id: string;
	name: string;
	created_at: string;
}

export interface WsdlEndpoint {
	id: string;
	url: string;
	soap_version: SoapVersion;
	soap_action: string;
	port_name: string;
	binding_name: string;
}

export interface WsdlOperation {
	id: string;
	name: string;
	soap_version: SoapVersion;
	soap_action: string;
	endpoint_count: number;
}

export interface WsdlService {
	id: string;
	name: string;
	operations: WsdlOperation[];
}

/**
 * A tree node without request templates: the explorer lists every file, and only
 * the opened operation carries the body. `project_id` is an empty string when the
 * file sits in the no-project view, which is how SQLite stores it.
 */
export interface WsdlFileSummary {
	id: string;
	project_id: string;
	name: string;
	source_url: string;
	warnings: string[];
	services: WsdlService[];
	imported_at: string;
}

export interface WsdlImportResult {
	summary: WsdlFileSummary;
	reimported: boolean;
	warnings: string[];
}

export interface WsdlState {
	endpoint_id: string;
	request_body: string;
	headers: RestProperty[];
	save_response: boolean;
	response_body: string;
	response_headers: RestProperty[];
	raw_request: string;
	/** The status line of the stored answer; `''` means the dump has no first line. */
	response_status_line: string;
}

export interface WsdlOperationDetail {
	id: string;
	name: string;
	service_id: string;
	service_name: string;
	soap_version: SoapVersion;
	soap_action: string;
	input_element: string;
	output_element: string;
	input_template: string;
	endpoints: WsdlEndpoint[];
	state: WsdlState;
}

export interface WsdlOperationInput {
	operation_id: string;
	endpoint_id: string;
	request_body: string;
	headers: RestProperty[];
	save_response: boolean;
}

/** What the explorer had open: a project (or the no-project view) and an operation. */
export interface WsdlSelection {
	project_id: string | null;
	operation_id: string | null;
}

const DESKTOP_ONLY = 'WSDL support is available in the desktop app only';

/** The WSDL tree, the fetch and the SOAP call all need the Tauri backend. */
export function isWsdlSupported(): boolean {
	return isTauriRuntime();
}

/** The wire spells the version `1.1`/`1.2`; the client uses the union type. */
export function soapVersionFromWire(value: string): SoapVersion {
	return value === '1.2' ? 'soap12' : 'soap11';
}

function mapEndpoint(endpoint: WsdlEndpoint): WsdlEndpoint {
	return { ...endpoint, soap_version: soapVersionFromWire(endpoint.soap_version) };
}

function mapOperation(operation: WsdlOperation): WsdlOperation {
	return { ...operation, soap_version: soapVersionFromWire(operation.soap_version) };
}

function mapFile(summary: WsdlFileSummary): WsdlFileSummary {
	return {
		...summary,
		services: summary.services.map((service) => ({
			...service,
			operations: service.operations.map(mapOperation)
		}))
	};
}

function mapDetail(detail: WsdlOperationDetail): WsdlOperationDetail {
	return {
		...detail,
		soap_version: soapVersionFromWire(detail.soap_version),
		endpoints: detail.endpoints.map(mapEndpoint)
	};
}

/** Rows the backend hands back are copied, so a table edit never mutates storage. */
function rows(value: unknown): RestProperty[] {
	return Array.isArray(value) ? (value as RestProperty[]).map((row) => ({ ...row })) : [];
}

function mapState(state: WsdlState): WsdlState {
	return {
		...state,
		headers: rows(state.headers),
		response_headers: rows(state.response_headers),
		// A state row written before the status line column simply has no first line.
		response_status_line: state.response_status_line ?? ''
	};
}

export async function listWsdlProjects(): Promise<WsdlProject[]> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	return invokeCommand<WsdlProject[]>('list_wsdl_projects');
}

export async function createWsdlProject(name: string): Promise<WsdlProject> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	return invokeCommand<WsdlProject>('create_wsdl_project', { name });
}

export async function renameWsdlProject(id: string, name: string): Promise<void> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand('rename_wsdl_project', { id, name });
}

export async function deleteWsdlProject(id: string): Promise<void> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand('delete_wsdl_project', { id });
}

export async function listWsdlFiles(): Promise<WsdlFileSummary[]> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	const files = await invokeCommand<WsdlFileSummary[]>('list_wsdl_files');
	return files.map(mapFile);
}

export async function importWsdl(projectId: string, url: string): Promise<WsdlImportResult> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	const result = await invokeCommand<WsdlImportResult>('import_wsdl', {
		project_id: projectId,
		url
	});
	return { ...result, summary: mapFile(result.summary) };
}

export async function deleteWsdlFile(id: string): Promise<void> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand('delete_wsdl_file', { id });
}

/** True when a running fetch stopped; a cancel before it only arms the flag. */
export async function cancelWsdlFetch(): Promise<boolean> {
	if (!isWsdlSupported()) return false;
	return invokeCommand<boolean>('cancel_wsdl_fetch');
}

export async function getWsdlOperation(operationId: string): Promise<WsdlOperationDetail> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	const detail = await invokeCommand<WsdlOperationDetail>('get_wsdl_operation', {
		operation_id: operationId
	});
	const mapped = mapDetail(detail);
	return { ...mapped, state: mapState(mapped.state) };
}

/** Only editable rows are written; the mandatory ones are recomputed on load. */
export async function saveWsdlOperationState(input: WsdlOperationInput): Promise<void> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand('save_wsdl_operation_state', {
		operation_id: input.operation_id,
		endpoint_id: input.endpoint_id,
		request_body: input.request_body,
		headers: input.headers.map((row) => ({ ...row })),
		save_response: input.save_response
	});
}

/**
 * Stores the answer together with the status line, so the `Raw` tab can rebuild the
 * whole exchange later. An empty line is sent as `null`: "no status known" is the
 * absence of a first line, not a blank one.
 */
export async function saveWsdlResponse(
	operationId: string,
	body: string,
	headers: RestProperty[],
	rawRequest: string,
	statusLine: string
): Promise<void> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand('save_wsdl_response', {
		operation_id: operationId,
		body,
		headers: headers.map((row) => ({ ...row })),
		raw_request: rawRequest,
		status_line: statusLine.trim() ? statusLine.trim() : null
	});
}

export async function clearWsdlResponse(operationId: string): Promise<void> {
	if (!isWsdlSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand('clear_wsdl_response', { operation_id: operationId });
}

const SELECTION_PROJECT_KEY = 'wsdl.activeProject';
const SELECTION_OPERATION_KEY = 'wsdl.activeOperation';

/** The first operation of a file, in the order the tree shows it. */
export function firstOperation(file: WsdlFileSummary): WsdlOperation | null {
	for (const service of file.services) {
		const operation = service.operations[0];
		if (operation) return operation;
	}
	return null;
}

/** Every operation of a file, flattened in tree order. */
export function fileOperations(file: WsdlFileSummary): WsdlOperation[] {
	return file.services.flatMap((service) => service.operations);
}

/**
 * Turns a stored selection into one the current data can show. Ids of deleted
 * projects and files are dropped, and the returned operation is always visible in
 * the returned project, so the tree never restores a hidden row.
 */
export function resolveWsdlSelection(
	projects: WsdlProject[],
	files: WsdlFileSummary[],
	stored: WsdlSelection
): WsdlSelection {
	const storedProject = projects.some((project) => project.id === stored.project_id)
		? stored.project_id
		: null;
	// `null` is the no-project view, which lists every file, so any row is visible.
	const isVisible = (file: WsdlFileSummary) =>
		storedProject === null || file.project_id === (storedProject ?? '');

	const storedOperation =
		files
			.flatMap((file) => fileOperations(file))
			.find((operation) => operation.id === stored.operation_id) ?? null;

	if (storedOperation) {
		const owner = files.find((file) =>
			fileOperations(file).some((operation) => operation.id === storedOperation.id)
		);
		// The stored project no longer holds this file: follow the file instead of
		// silently selecting another row.
		if (!owner || !isVisible(owner)) {
			return { project_id: owner?.project_id || null, operation_id: storedOperation.id };
		}
		return { project_id: storedProject, operation_id: storedOperation.id };
	}

	const visible = files.filter(isVisible);
	const operation = visible.flatMap((file) => fileOperations(file))[0] ?? null;
	return { project_id: storedProject, operation_id: operation?.id ?? null };
}

export async function loadWsdlSelection(): Promise<WsdlSelection> {
	const [project, operation] = await Promise.all([
		getSetting(SELECTION_PROJECT_KEY),
		getSetting(SELECTION_OPERATION_KEY)
	]);
	return { project_id: project || null, operation_id: operation || null };
}

/** An absent selection is stored as an empty string so a cleared row stays cleared. */
export async function saveWsdlSelection(selection: WsdlSelection): Promise<void> {
	await Promise.all([
		setSetting(SELECTION_PROJECT_KEY, selection.project_id ?? ''),
		setSetting(SELECTION_OPERATION_KEY, selection.operation_id ?? '')
	]);
}
