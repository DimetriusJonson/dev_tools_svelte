import { buildCurl, buildRawRequestPreview, buildRawResponse, type CurlShell, type RestProperty } from './rest';
import { formatContent } from './formatters';
import { containsTemplate, substituteText, substituteUrl, type TemplateSource, type VarValues } from './vars';

/** A SOAP call is always a POST to the file's endpoint. */
export const SOAP_METHOD = 'POST';

/** The HTTP request a SOAP cURL export writes, before the shell quoting. */
export interface SoapCurlRequest {
	method: string;
	url: string;
	headers: RestProperty[];
	body: string;
	body_type: 'xml';
}

/** The page fields an export reads: the endpoint URL, the SOAP version/action
 *  that decide the locked rows, the user header rows and the body fragment. */
export interface SoapCurlDraft {
	endpointUrl: string;
	soapVersion: SoapVersion;
	soapAction: string;
	userHeaders: RestProperty[];
	body: string;
}

/** The places the export scans for templates — the merged header rows (the
 *  names included, because the backend refuses a template in a header name),
 *  the envelope built from the body fragment, and the endpoint URL. */
export function soapCurlTemplatePlaces(draft: SoapCurlDraft): TemplateSource {
	return {
		url: draft.endpointUrl,
		headers: mergeLockedHeaders(lockedHeadersFor(draft.soapVersion, draft.soapAction), draft.userHeaders),
		body: buildEnvelope(draft.body),
		body_type: 'xml'
	};
}

/** The request the export writes, with the variable values the page holds.
 *  Plain values are written in; a safe one stays the mask until the dialog
 *  reveals it. `null` when no endpoint is picked — the page blocks on that. */
export function soapCurlRequest(draft: SoapCurlDraft, values: VarValues): SoapCurlRequest | null {
	const places = soapCurlTemplatePlaces(draft);
	if (draft.endpointUrl.trim() === '') return null;
	const render = (text: string) => (containsTemplate(text) ? substituteText(text, values) : text);
	return {
		method: SOAP_METHOD,
		url: substituteUrl(places.url, values),
		headers: places.headers.map((row) => ({ name: row.name, value: render(row.value) })),
		body: render(places.body),
		body_type: 'xml'
	};
}

/** The envelope namespace, shared by SOAP 1.1 and SOAP 1.2 bindings. */
export const SOAP_ENVELOPE_NS = 'http://schemas.xmlsoap.org/soap/envelope/';

/** The namespace a WSDL 1.1 document declares; a 2.0 document is refused. */
export const WSDL1_NS = 'http://schemas.xmlsoap.org/wsdl/';
export const WSDL2_NS = 'http://www.w3.org/2002/wsdl/';

/** The namespaces the WSDL bindings use for the SOAP vocabulary. */
export const SOAP_NS = 'http://schemas.xmlsoap.org/wsdl/soap/';
export const SOAP12_NS = 'http://schemas.xmlsoap.org/wsdl/soap12/';

/** A WSDL binds operations to SOAP 1.1 or to SOAP 1.2. */
export type SoapVersion = 'soap11' | 'soap12';

/**
 * The version a binding element uses: one of the two SOAP namespaces decides,
 * and a missing namespace attribute leaves the document default of 1.1.
 */
export function soapVersionFromBinding(bindingNamespace: string, versionAttribute: string): SoapVersion {
	if (bindingNamespace === SOAP12_NS) return 'soap12';
	if (bindingNamespace === SOAP_NS) return 'soap11';
	if (versionAttribute === '1.2') return 'soap12';

	// A document without a version attribute binds 1.1 operations.
	// An unknown namespace is not a SOAP binding at all.
	return 'soap11';
}

/** A row that duplicates a mandatory header name is dropped, not duplicated. */
export type HeaderRowKind = 'locked' | 'user';

/** The action value the wire needs: quotes once, never doubled. */
export function soapActionForCall(action: string): string {
	// A quoted action is sent quoted exactly once, not doubled.
	// An empty action stays empty: the header row is still there, empty.
	if (action === '' || action.startsWith('"')) return action;

	return `"${action}"`;
}

/** Mandatory rows for an operation: 1.1 pins two rows, 1.2 pins one. */
export function lockedHeadersFor(version: SoapVersion, action: string): RestProperty[] {
	// A 1.2 operation gets its own mandatory content type, and the action
	// rides inside it as a quoted attribute rather than a header row.
	if (version === 'soap12') {
		const quoted = soapActionForCall(action);
		if (action === '')
			return [{ name: 'Content-Type', value: 'application/soap+xml; charset=utf-8' }];

		return [
			{
				name: 'Content-Type',
				value: `application/soap+xml; charset=utf-8; action=${quoted}`
			}
		];
	}

	// A 1.1 operation pins both headers the binding mandates.
	// `SOAPAction` is always sent, with an empty value when the WSDL has no action.
	return [
		{ name: 'Content-Type', value: 'text/xml; charset=utf-8' },
		{ name: 'SOAPAction', value: soapActionForCall(action) }
	];
}

/** Mandatory row names, lowercased: the case-insensitive clash check. */
export function lockedHeaderNames(version: SoapVersion, action: string): string[] {
	// The set depends on both the SOAP version and the action, so a 1.2
	// operation locks only one row.
	return lockedHeadersFor(version, action).map((header) => header.name.toLowerCase());
}

/** A row is shown and editable when it is real and does not clash with the file. */
export function keepEditableHeaders(version: SoapVersion, action: string, row: RestProperty): boolean {
	// A row with a blank name never reaches the wire, so the tab does not show
	// it either.
	if (row.name.trim() === '') return false;

	// A user row that repeats a mandatory name is dropped, not duplicated. The
	// wire trims a header name, so a padded spelling clashes too.
	return !lockedHeaderNames(version, action).includes(row.name.trim().toLowerCase());
}

/** The rows the Headers tab shows: mandatory first, user rows after. */
export function mergeLockedHeaders(
	mandatory: RestProperty[],
	user: RestProperty[]
): RestProperty[] {
	// Mandatory rows come first so the tab shows what the binding sends.
	// User rows stay in the order the user typed them.
	return [
		...mandatory,
		...user.filter((row) => row.name.trim() !== '')
	];
}

/** Rows written back to SQLite: the locked rows are recomputed on load. */
export function editableForSave(version: SoapVersion, action: string, rows: RestProperty[]): RestProperty[] {
	const names = lockedHeaderNames(version, action);

	// Locked rows are recomputed from the file on every load and at send, so a
	// persisted copy would shadow the binding after a re-import.
	return rows.filter(
		(row) => row.name.trim() !== '' && !names.includes(row.name.trim().toLowerCase())
	);
}

/** A name never reaches the wire with spaces around it; values stay verbatim. */
export function normalizeHeaderRow(row: RestProperty): RestProperty {
	// Only the name is cleaned: values are kept verbatim, including quotes.
	return { name: row.name.trim(), value: row.value };
}

/** Names are cleaned before the table sees rows, not only on save. */
export function normalizeHeaderRows(rows: RestProperty[]): RestProperty[] {
	// A blank name never reaches the wire, but a value typed without a name
	// stays visible so the user can still fix the row.
	return rows
		.map((row) => normalizeHeaderRow(row))
		.filter((row) => row.name !== '' || row.value.trim() !== '');
}

/** The current endpoint of an operation among the file's endpoints. */
export function selectEndpoint<T extends { id: string }>(
	endpoints: T[],
	savedId: string | null
): T | null {
	// No id means the file's first endpoint is the current one.
	if (savedId === null || savedId === '') return endpoints[0] ?? null;

	// A saved id that is still present selects that endpoint of the file.
	// An unknown id behaves like no id at all: the first endpoint is current.
	return endpoints.find((endpoint) => endpoint.id === savedId) ?? endpoints[0] ?? null;
}

/** True when the current endpoint is not the file's default one. */
export function endpointChanged(endpoints: { id: string }[], savedId: string | null): boolean {
	// An unknown id resolved to the first endpoint, so it is not a change.
	const selected = selectEndpoint(endpoints, savedId);
	if (selected === null) return false;

	// The explorer marks the endpoint as changed only when the user picked a
	// different endpoint than the first one of the file.
	return endpoints.length > 1 && selected.id !== endpoints[0].id;
}

/**
 * The body fragment the user edits, wrapped into the envelope that goes out.
 */
export function buildEnvelope(bodyFragment: string): string {
	const inner = bodyFragment.trim() === '' ? '' : `\n${bodyFragment.trim()}\n`;

	// The fragment keeps its own xmlns declarations, written verbatim.
	// An operation without an input type sends an empty body.
	return (
		`<soap:Envelope xmlns:soap="${SOAP_ENVELOPE_NS}">` +
		`<soap:Body>${inner}</soap:Body>` +
		'</soap:Envelope>'
	);
}

/**
 * The Raw tab: the same HTTP exchange the REST page previews, with the locked
 * and user headers merged and the envelope as the body.
 */
export function buildSoapRawRequest(
	url: string,
	version: SoapVersion,
	action: string,
	userHeaders: RestProperty[],
	body: string
): string {
	// A SOAP call is always a POST to the endpoint.
	// The envelope is the body that goes on the wire.
	return buildRawRequestPreview(
		SOAP_METHOD,
		url,
		mergeLockedHeaders(lockedHeadersFor(version, action), userHeaders),
		buildEnvelope(body),
		// A SOAP body is XML for both versions; the locked rows carry the media
		// type, so this only feeds the preview's other defaults.
		'xml'
	);
}

/**
 * The `Raw` tab's `Format`: the payload of a response dump is re-indented while the
 * status line and the header block above it are copied untouched, so the framing of
 * the exchange stays exactly as it arrived. A body with no markup throws, like every
 * other XML format here.
 */
export function formatRawResponseDump(
	statusLine: string,
	headers: RestProperty[],
	body: string,
	indent = 4
): string {
	const formatted = formatContent('xml', body, { indent, doFormat: true, doUnescape: false });
	return buildRawResponse(statusLine, headers, formatted);
}

/** An i18n key plus the detail to interpolate into it. */
export interface LocalizedDetail {
	key: string;
	detail: string;
}

/**
 * The reason prefixes `fetch_wsdl` and `parse_wsdl` answer with. They stay English on
 * the wire, like the other service texts, and the UI translates them by prefix.
 */
const ERROR_KEYS: [prefix: string, key: string][] = [
	['not-url:', 'wsdl.import.notUrl'],
	['malformed:', 'wsdl.import.malformed'],
	['not-wsdl:', 'wsdl.import.notWsdl'],
	['wsdl2:', 'wsdl.import.wsdl2'],
	['no-operations:', 'wsdl.import.noOperations'],
	['cancelled', 'wsdl.import.cancelled'],
	['timeout:', 'wsdl.import.timeout'],
	['connect:', 'wsdl.import.unreachable'],
	['network:', 'wsdl.import.unreachable'],
	['http:', 'wsdl.import.http'],
	['too-large:', 'wsdl.import.tooLarge'],
	['encoding:', 'wsdl.import.encoding']
];

export function describeErrorText(text: string): LocalizedDetail {
	for (const [prefix, key] of ERROR_KEYS) {
		if (text.startsWith(prefix)) {
			return { key, detail: text.slice(prefix.length).trim() };
		}
	}
	// A reason the backend invented on its own is shown verbatim rather than hidden.
	return { key: 'wsdl.import.failed', detail: text };
}

/**
 * The machine codes `parse_wsdl` and the schema index put in `warnings`. A code is
 * `prefix:detail` or a bare word when nothing names a specific part.
 */
const WARNING_KEYS: [prefix: string, key: string][] = [
	['import-external', 'wsdl.warning.external'],
	['no-binding:', 'wsdl.warning.noBinding'],
	['no-address:', 'wsdl.warning.noAddress'],
	['no-port-type:', 'wsdl.warning.noPortType'],
	['no-input:', 'wsdl.warning.noInput'],
	['unknown-type:', 'wsdl.warning.unknownType'],
	['cycle:', 'wsdl.warning.cycle'],
	['depth-limit', 'wsdl.warning.depthLimit'],
	['element-limit', 'wsdl.warning.elementLimit']
];

export function describeWarningText(warning: string): LocalizedDetail {
	for (const [prefix, key] of WARNING_KEYS) {
		if (warning.startsWith(prefix)) {
			return { key, detail: warning.slice(prefix.length) };
		}
	}
	return { key: 'wsdl.warning.unknownType', detail: warning };
}
