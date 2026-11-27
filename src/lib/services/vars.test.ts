import { afterEach, describe, expect, it } from 'vitest';
import {
	createGlobalVar,
	deleteGlobalVar,
	isVarsSupported,
	listGlobalVars,
	revealGlobalVar,
	updateGlobalVar
} from './vars';

// `isTauriRuntime()` looks for the internals object the Tauri WebView injects.
function asDesktop(): void {
	(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}

function leaveDesktop(): void {
	delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
}

describe('global variables service', () => {
	afterEach(leaveDesktop);

	it('reports the feature as unsupported in a browser', () => {
		expect(isVarsSupported()).toBe(false);
	});

	it('refuses every command without the desktop backend', async () => {
		await expect(listGlobalVars()).rejects.toThrow('desktop app only');
		await expect(createGlobalVar({ name: 'token', secure: false, value: 'x' })).rejects.toThrow(
			'desktop app only'
		);
		await expect(updateGlobalVar({ id: 'a', name: 'token', secure: false, value: 'x' })).rejects.toThrow(
			'desktop app only'
		);
		await expect(deleteGlobalVar('a')).rejects.toThrow('desktop app only');
		// A secret never leaves the credential store through the browser bundle.
		await expect(revealGlobalVar('a')).rejects.toThrow('desktop app only');
	});

	it('says the page is available once the desktop runtime is present', () => {
		asDesktop();
		expect(isVarsSupported()).toBe(true);
	});
});
