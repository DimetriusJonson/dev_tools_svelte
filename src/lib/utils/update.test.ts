import { describe, expect, it } from 'vitest';
import {
	classifyUpdateFailure,
	downloadPercent,
	IDLE_UPDATE_STATE,
	isDismissedVersion,
	isUpdateBusy,
	isWindowsUserAgent,
	updateFailureMessage
} from './update';

describe('downloadPercent', () => {
	it('reports the received share of a known content length', () => {
		expect(downloadPercent(250, 1000)).toBe(25);
		expect(downloadPercent(1000, 1000)).toBe(100);
	});

	it('rounds to a whole percent so the banner never shows fractions', () => {
		expect(downloadPercent(1, 3)).toBe(33);
		expect(downloadPercent(2, 3)).toBe(67);
	});

	it('keeps an unreported size at zero instead of faking progress', () => {
		expect(downloadPercent(512, undefined)).toBe(0);
		expect(downloadPercent(512, 0)).toBe(0);
	});

	it('stays at zero before the first chunk and never exceeds 100', () => {
		expect(downloadPercent(0, 1000)).toBe(0);
		expect(downloadPercent(1200, 1000)).toBe(100);
	});
});

describe('isUpdateBusy', () => {
	it('treats every in-flight phase as busy', () => {
		expect(isUpdateBusy('checking')).toBe(true);
		expect(isUpdateBusy('downloading')).toBe(true);
		expect(isUpdateBusy('installing')).toBe(true);
	});

	it('allows a new check once nothing is running', () => {
		expect(isUpdateBusy('idle')).toBe(false);
		expect(isUpdateBusy('available')).toBe(false);
		expect(isUpdateBusy('ready')).toBe(false);
		expect(isUpdateBusy('error')).toBe(false);
	});
});

describe('isDismissedVersion', () => {
	it('hides exactly the version the user dismissed', () => {
		expect(isDismissedVersion('0.2.0', '0.2.0')).toBe(true);
	});

	it('reports a newer release again', () => {
		expect(isDismissedVersion('0.2.0', '0.3.0')).toBe(false);
	});

	it('nothing was dismissed when the setting is empty', () => {
		expect(isDismissedVersion(null, '0.2.0')).toBe(false);
	});
});

describe('classifyUpdateFailure', () => {
	it('groups configuration and platform problems as unavailable', () => {
		expect(classifyUpdateFailure('Updater does not have any endpoints set.')).toBe('unavailable');
		expect(
			classifyUpdateFailure(
				'`http://example.com/latest.json` is not a secure protocol. Use `https` or enable dangerousInsecureTransportProtocol.'
			)
		).toBe('unavailable');
		expect(classifyUpdateFailure('Unsupported OS, expected one of `linux`, `darwin` or `windows`.')).toBe(
			'unavailable'
		);
		expect(
			classifyUpdateFailure(
				'Unsupported application architecture, expected one of `x86`, `x86_64`, `arm` or `aarch64`.'
			)
		).toBe('unavailable');
	});

	it('treats a release that simply does not exist as not found', () => {
		expect(classifyUpdateFailure('Could not fetch a valid release JSON from the remote')).toBe(
			'notFound'
		);
		expect(
			classifyUpdateFailure(
				'the platform `windows-x86_64` was not found in the response `platforms` object'
			)
		).toBe('notFound');
		expect(
			classifyUpdateFailure(
				'None of the fallback platforms ["windows-x86_64"] were found in the response `platforms` object'
			)
		).toBe('notFound');
	});

	it('keeps download and signature problems as a real failure', () => {
		expect(classifyUpdateFailure('Invalid minisign signature key')).toBe('failed');
		expect(classifyUpdateFailure('error downloading update: 404 Not Found')).toBe('failed');
		expect(classifyUpdateFailure('binary for the current target not found in the archive')).toBe(
			'failed'
		);
		expect(
			classifyUpdateFailure(
				'The update was signed for version 0.2.0 but the update endpoint announced version 0.3.0.'
			)
		).toBe('failed');
		expect(classifyUpdateFailure('')).toBe('failed');
	});
});

describe('updateFailureMessage', () => {
	it('prefers the plugin text over the fallback', () => {
		expect(updateFailureMessage(new Error('  bad signature  '), 'fallback')).toBe('bad signature');
		expect(updateFailureMessage('boom', 'fallback')).toBe('boom');
	});

	it('falls back when the error carries no text', () => {
		expect(updateFailureMessage(new Error('   '), 'fallback')).toBe('fallback');
		expect(updateFailureMessage('', 'fallback')).toBe('fallback');
		expect(updateFailureMessage(undefined, 'fallback')).toBe('fallback');
		expect(updateFailureMessage({ status: 500 }, 'fallback')).toBe('fallback');
	});
});

describe('isWindowsUserAgent', () => {
	it('detects Windows where the installer relaunches the app itself', () => {
		expect(
			isWindowsUserAgent(
				'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120 Safari/537.36'
			)
		).toBe(true);
	});

	it('does not match other platforms that need a manual restart', () => {
		expect(isWindowsUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')).toBe(
			false
		);
		expect(isWindowsUserAgent('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36')).toBe(false);
		expect(isWindowsUserAgent('')).toBe(false);
	});
});

describe('IDLE_UPDATE_STATE', () => {
	it('is the reset target of every finished or failed run', () => {
		expect(IDLE_UPDATE_STATE).toEqual({
			phase: 'idle',
			version: null,
			notes: null,
			percent: 0,
			message: null
		});
	});
});
