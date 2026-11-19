import { get } from 'svelte/store';
import { isTauriRuntime } from '$lib/services/db';
import { t } from '$lib/stores/i18n';
import { notify } from '$lib/stores/toast';

export async function readClipboardText(): Promise<string> {
	if (isTauriRuntime()) {
		const { readText } = await import('@tauri-apps/plugin-clipboard-manager');
		return (await readText()) ?? '';
	}
	if (navigator.clipboard && window.isSecureContext) {
		return navigator.clipboard.readText();
	}
	throw new Error('Clipboard is not available');
}

export async function copyText(text: string): Promise<boolean> {
	if (!text) return false;
	let copied = false;

	try {
		if (navigator.clipboard && window.isSecureContext) {
			await navigator.clipboard.writeText(text);
			copied = true;
		}
	} catch {
		/* fall through to the legacy implementation */
	}

	if (!copied) {
		try {
			const textarea = document.createElement('textarea');
			textarea.value = text;
			textarea.setAttribute('readonly', '');
			textarea.style.position = 'fixed';
			textarea.style.top = '-1000px';
			textarea.style.opacity = '0';
			document.body.appendChild(textarea);
			textarea.select();
			copied = document.execCommand('copy');
			document.body.removeChild(textarea);
		} catch {
			copied = false;
		}
	}

	const translate = get(t);
	notify(
		translate(copied ? 'common.copied' : 'common.copyFailed'),
		copied ? 'success' : 'error'
	);
	return copied;
}
