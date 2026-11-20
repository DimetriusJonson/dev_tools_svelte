import { isTauriRuntime } from './db';

export type FileMode = 'xml' | 'json' | 'txt';

const FILTERS: Record<FileMode, { name: string; extensions: string[] }[]> = {
	xml: [
		{ name: 'XML', extensions: ['xml'] },
		{ name: 'All files', extensions: ['*'] }
	],
	json: [
		{ name: 'JSON', extensions: ['json'] },
		{ name: 'All files', extensions: ['*'] }
	],
	txt: [
		{ name: 'Text', extensions: ['txt'] },
		{ name: 'All files', extensions: ['*'] }
	]
};

export async function pickInputFile(mode: 'xml' | 'json'): Promise<string | null> {
	if (!isTauriRuntime()) return null;
	const { open } = await import('@tauri-apps/plugin-dialog');
	const selection = await open({
		multiple: false,
		directory: false,
		title: mode === 'xml' ? 'Select XML file' : 'Select JSON file',
		filters: FILTERS[mode]
	});
	return typeof selection === 'string' ? selection : null;
}

export async function pickShareFile(): Promise<string | null> {
	if (!isTauriRuntime()) return null;
	const { open } = await import('@tauri-apps/plugin-dialog');
	const selection = await open({
		multiple: false,
		directory: false,
		title: 'Select file to share',
		filters: [{ name: 'All files', extensions: ['*'] }]
	});
	return typeof selection === 'string' ? selection : null;
}

export async function pickSaveFile(defaultPath: string): Promise<string | null> {
	if (!isTauriRuntime()) return null;
	const { save } = await import('@tauri-apps/plugin-dialog');
	const selection = await save({
		defaultPath,
		title: 'Save shared file',
		filters: [{ name: 'All files', extensions: ['*'] }]
	});
	return typeof selection === 'string' ? selection : null;
}

export async function pickOutputFile(defaultPath: string, mode: FileMode): Promise<string | null> {
	if (!isTauriRuntime()) return null;
	const { save } = await import('@tauri-apps/plugin-dialog');
	const selection = await save({
		defaultPath,
		title: 'Save formatted file',
		filters: FILTERS[mode]
	});
	return typeof selection === 'string' ? selection : null;
}

export function basename(path: string): string {
	const normalized = path.replace(/\\/g, '/');
	const index = normalized.lastIndexOf('/');
	return index >= 0 ? normalized.slice(index + 1) : normalized;
}

/** A file chosen for a `multipart/form-data` part. */
export interface PickedPartFile {
	path: string;
	filename: string;
	file?: File;
}

/**
 * Picks the file of a multipart part. Desktop returns the path the backend reads when
 * the request is sent; the browser hands over the `File` itself, whose bytes live in
 * memory for this session and cannot be turned back into a path.
 */
export async function pickPartFile(title: string): Promise<PickedPartFile | null> {
	if (isTauriRuntime()) {
		const { open } = await import('@tauri-apps/plugin-dialog');
		const selection = await open({
			multiple: false,
			directory: false,
			title,
			filters: [{ name: 'All files', extensions: ['*'] }]
		});
		if (typeof selection !== 'string') return null;
		return { path: selection, filename: basename(selection) };
	}
	return pickBrowserFile();
}

function pickBrowserFile(): Promise<PickedPartFile | null> {
	if (typeof document === 'undefined') return Promise.resolve(null);
	return new Promise((resolve) => {
		const input = document.createElement('input');
		input.type = 'file';
		input.style.position = 'fixed';
		input.style.left = '-9999px';
		const finish = (value: PickedPartFile | null): void => {
			input.remove();
			resolve(value);
		};
		input.addEventListener('change', () => {
			const file = input.files?.[0];
			finish(file ? { path: '', filename: file.name, file } : null);
		});
		input.addEventListener('cancel', () => finish(null));
		document.body.appendChild(input);
		input.click();
	});
}

export function formattedPath(path: string, mode: 'xml' | 'json'): string {
	const extension = `.${mode}`;
	if (path.toLowerCase().endsWith(extension)) {
		return `${path.slice(0, -extension.length)}.formatted${extension}`;
	}
	return `${path}.formatted.${mode}`;
}
