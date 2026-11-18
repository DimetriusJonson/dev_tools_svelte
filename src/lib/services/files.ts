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

export function formattedPath(path: string, mode: 'xml' | 'json'): string {
	const extension = `.${mode}`;
	if (path.toLowerCase().endsWith(extension)) {
		return `${path.slice(0, -extension.length)}.formatted${extension}`;
	}
	return `${path}.formatted.${mode}`;
}
