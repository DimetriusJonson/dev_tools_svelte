import { describe, expect, it } from 'vitest';
import {
	soapVersionFromBinding,
	soapActionForCall,
	lockedHeadersFor,
	lockedHeaderNames,
	keepEditableHeaders,
	mergeLockedHeaders,
	editableForSave,
	normalizeHeaderRow,
	normalizeHeaderRows,
	selectEndpoint,
	endpointChanged,
	describeErrorText,
	describeWarningText,
} from './wsdl';
import { buildEnvelope, buildSoapRawRequest } from './wsdl';

const SOAP_NS = 'http://schemas.xmlsoap.org/wsdl/soap/';
const SOAP12_NS = 'http://schemas.xmlsoap.org/wsdl/soap12/';

describe('binding version and action', () => {
	it('binds the two SOAP namespaces to their versions', () => {
		// The namespace decides; the attribute only steps in when it says 1.2.
		expect(soapVersionFromBinding(SOAP_NS, '')).toBe('soap11');
		expect(soapVersionFromBinding(SOAP12_NS, '')).toBe('soap12');
		expect(soapVersionFromBinding(SOAP_NS, '1.2')).toBe('soap11');
		expect(soapVersionFromBinding('http://example.com/bind', '1.2')).toBe('soap12');
		expect(soapVersionFromBinding('http://example.com/bind', '')).toBe('soap11');
	});

	it('quotes the action exactly once', () => {
		// An empty action stays empty: the header row is still there, empty.
		// An action already written with quotes keeps its own quotes.
		expect(soapActionForCall('')).toBe('');
		expect(soapActionForCall('urn:demo:echo')).toBe('"urn:demo:echo"');
		expect(soapActionForCall('"urn:demo:echo"')).toBe('"urn:demo:echo"');
	});
});

describe('locked header rows', () => {
	it('locks the mandatory rows per version', () => {
		// SOAP 1.1 pins Content-Type and an always-present SOAPAction row.
		// SOAP 1.2 pins one content type that carries the action inside.
		expect(lockedHeadersFor('soap11', 'urn:demo:echo')).toEqual([
			{ name: 'Content-Type', value: 'text/xml; charset=utf-8' },
			{ name: 'SOAPAction', value: '"urn:demo:echo"' }
		]);
		const empty = lockedHeadersFor('soap11', '');
		// The action row stays present even when the WSDL gives no action.
		expect(empty[1]).toEqual({ name: 'SOAPAction', value: '' });
		expect(lockedHeadersFor('soap12', '')).toEqual([
			{ name: 'Content-Type', value: 'application/soap+xml; charset=utf-8' }
		]);
		expect(lockedHeadersFor('soap12', 'urn:demo:echo')).toEqual([
			{
				name: 'Content-Type',
				value: 'application/soap+xml; charset=utf-8; action="urn:demo:echo"'
			}
		]);
		// Mandatory names are lowercased for the case-insensitive clash check.
		expect(lockedHeaderNames('soap11', 'a')).toEqual(['content-type', 'soapaction']);
		expect(lockedHeaderNames('soap12', 'a')).toEqual(['content-type']);
	});
});

describe('editable header rows', () => {
	it('drops rows that clash with the binding', () => {
		// A blank name never reaches the wire, so the row is dropped.
		// The clash check ignores the case of a user row.
		expect(keepEditableHeaders('soap11', 'a', { name: '  ', value: 'x' })).toBe(false);
		expect(keepEditableHeaders('soap11', 'a', { name: 'content-type', value: 'y' })).toBe(false);
		expect(keepEditableHeaders('soap11', 'a', { name: 'soapaction', value: 'y' })).toBe(false);
		// The wire trims a header name, so a padded spelling clashes as well.
		expect(keepEditableHeaders('soap11', 'a', { name: ' SOAPAction ', value: 'y' })).toBe(false);
		// A user row for a non-mandatory header survives untouched.
		expect(keepEditableHeaders('soap11', 'a', { name: 'Authorization', value: 't' })).toBe(true);
		// A 1.2 operation locks only the content type it needs.
		expect(keepEditableHeaders('soap12', 'a', { name: 'SOAPAction', value: 'y' })).toBe(true);
	});

	it('shows locked rows first and saves only editable ones', () => {
		const user = [
			{ name: 'Authorization', value: 'secret' },
			{ name: '   ', value: 'noise' }
		];
		const visible = mergeLockedHeaders(lockedHeadersFor('soap11', 'urn:demo:echo'), user);
		// Mandatory rows come first; blank-name user rows never go out.
		expect(visible.map((row) => row.name)).toEqual(['Content-Type', 'SOAPAction', 'Authorization']);
		// Saving strips the locked rows back out: only editable rows persist.
		expect(editableForSave('soap11', 'urn:demo:echo', visible)).toEqual(user.slice(0, 1));
	});

	it('refuses to save a padded spelling of a locked name', () => {
		// The backend trims a header name when it merges, so a padded mandatory row is
		// no more savable than a clean one: it would clash on the next load.
		expect(
			editableForSave('soap11', 'a', [
				{ name: 'Content-Type', value: 'text/xml; charset=utf-8' },
				{ name: '  SOAPAction  ', value: 'stale' },
				{ name: 'Authorization', value: 't' }
			])
		).toEqual([{ name: 'Authorization', value: 't' }]);
	});
});

describe('normalization before the table sees rows', () => {
	it('trims names and keeps values verbatim', () => {
		// Only the name is cleaned: values are kept verbatim, quotes and all.
		expect(normalizeHeaderRow({ name: ' Cache ', value: ' a ' })).toEqual({ name: 'Cache', value: ' a ' });
		// A value typed without a name stays visible so the text is not lost.
		// A row with neither name nor value is dropped.
		expect(normalizeHeaderRows([
			{ name: ' X ', value: '1' },
			{ name: '', value: 'loose' },
			{ name: '', value: '   ' }
		])).toEqual([
			{ name: 'X', value: '1' },
			{ name: '', value: 'loose' }
		]);
	});
});

describe('endpoint selection', () => {
	it('falls back to the first endpoint of the file', () => {
		const endpoints = [{ id: 'a' }, { id: 'b' }];
		// No saved id means the file's first endpoint is current.
		expect(selectEndpoint(endpoints, null)).toBe(endpoints[0]);
		expect(selectEndpoint(endpoints, '')).toBe(endpoints[0]);
		// An unknown id behaves like no id at all.
		expect(selectEndpoint(endpoints, 'gone')).toBe(endpoints[0]);
		// A saved id that is still present selects that endpoint.
		expect(selectEndpoint(endpoints, 'b')).toBe(endpoints[1]);
		expect(endpointChanged(endpoints, null)).toBe(false);
		expect(endpointChanged(endpoints, 'gone')).toBe(false);
		// Picking the second endpoint of a multi-port file is a change.
		expect(endpointChanged(endpoints, 'b')).toBe(true);
		// A single-port file never reports a change.
		expect(endpointChanged([{ id: 'a' }], 'a')).toBe(false);
		// No endpoints at all selects nothing.
		expect(selectEndpoint([], 'x')).toBe(null);
	});
});

describe('envelope and raw preview', () => {
	it('wraps the body fragment into the soap envelope', () => {
		const opened = buildEnvelope('<urn:Get><n>5</n></urn:Get>');
		expect(opened).toContain('<soap:Body>');
		expect(opened).toContain('<urn:Get><n>5</n></urn:Get>');
	});

	it('keeps an empty fragment as an empty body', () => {
		expect(buildEnvelope('  ')).toContain('<soap:Body></soap:Body>');
	});

	it('previews the same exchange the REST page builds', () => {
		const raw = buildSoapRawRequest(
			'http://localhost:9/soap', 'soap11', 'urn:demo:echo',
			[{ name: 'Authorization', value: 't' }],
			'<urn:Get/>'
		);
		// A SOAP call is always a POST to the endpoint.
		expect(raw).toContain('POST /soap HTTP/1.1');
		// The locked rows show what the binding sends.
		expect(raw).toContain('Content-Type: text/xml; charset=utf-8');
		expect(raw).toContain('SOAPAction: "urn:demo:echo"');
		// The envelope is the body that goes on the wire.
		expect(raw).toContain('<soap:Envelope');
	});
});

describe('backend reasons and warning codes', () => {
	it('splits a fetch reason into the key and its detail', () => {
		// The prefix stays English on the wire; the UI reads the key and shows the detail.
		expect(describeErrorText('not-url: "wpnd"')).toEqual({
			key: 'wsdl.import.notUrl',
			detail: '"wpnd"'
		});
		expect(describeErrorText('timeout: the download took more than 30 seconds')).toEqual({
			key: 'wsdl.import.timeout',
			detail: 'the download took more than 30 seconds'
		});
		// Both transport spellings answer with the same sentence.
		expect(describeErrorText('connect: tcp connect error').key).toBe('wsdl.import.unreachable');
		expect(describeErrorText('network: error decoding response body').key).toBe(
			'wsdl.import.unreachable'
		);
		expect(describeErrorText('http: 404 Not Found')).toEqual({
			key: 'wsdl.import.http',
			detail: '404 Not Found'
		});
	});

	it('reads a cancellation as a reason of its own without a detail', () => {
		// The reason is the whole word, so nothing is left to interpolate.
		expect(describeErrorText('cancelled')).toEqual({ key: 'wsdl.import.cancelled', detail: '' });
	});

	it('shows a reason the backend invented verbatim instead of hiding it', () => {
		expect(describeErrorText('some internal failure')).toEqual({
			key: 'wsdl.import.failed',
			detail: 'some internal failure'
		});
	});

	it('maps every warning code the parser emits', () => {
		// Codes carry the part they name, so the sentence ends with it.
		expect(describeWarningText('no-binding:EchoService::EchoSoap')).toEqual({
			key: 'wsdl.warning.noBinding',
			detail: 'EchoService::EchoSoap'
		});
		expect(describeWarningText('no-address:EchoService::EchoSoap').key).toBe(
			'wsdl.warning.noAddress'
		);
		expect(describeWarningText('no-port-type:EchoService::EchoSoap').key).toBe(
			'wsdl.warning.noPortType'
		);
		expect(describeWarningText('no-input:S::P::echo').key).toBe('wsdl.warning.noInput');
		expect(describeWarningText('unknown-type:EchoResponse').key).toBe('wsdl.warning.unknownType');
		expect(describeWarningText('cycle:Node')).toEqual({
			key: 'wsdl.warning.cycle',
			detail: 'Node'
		});
		// Bare codes name no part, so their sentences interpolate nothing.
		expect(describeWarningText('import-external')).toEqual({
			key: 'wsdl.warning.external',
			detail: ''
		});
		expect(describeWarningText('depth-limit').key).toBe('wsdl.warning.depthLimit');
		expect(describeWarningText('element-limit').key).toBe('wsdl.warning.elementLimit');
	});

	it('keeps an unknown code as its own detail', () => {
		expect(describeWarningText('future-code:thing')).toEqual({
			key: 'wsdl.warning.unknownType',
			detail: 'future-code:thing'
		});
	});
});
