export interface EditorStateRecord {
	tool_key: string;
	content: string;
	indent: number;
	opt_format: boolean;
	opt_unescape: boolean;
}

export interface ProcessFileRequest {
	input_path: string;
	output_path: string;
	mode: 'xml' | 'json';
	indent: number;
	do_format: boolean;
	do_unescape: boolean;
	do_escape: boolean;
}

export interface ProcessFileResult {
	output_path: string;
	bytes_in: number;
	bytes_out: number;
}

const EDITOR_PREFIX = 'devtools.editor.';
const SETTING_PREFIX = 'devtools.setting.';

export function isTauriRuntime(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<T>(command, args);
}

function readLocal<T>(key: string): T | null {
	if (typeof window === 'undefined') return null;
	try {
		const raw = window.localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : null;
	} catch {
		return null;
	}
}

function writeLocal(key: string, value: unknown): void {
	if (typeof window === 'undefined') return;
	try {
		window.localStorage.setItem(key, JSON.stringify(value));
	} catch {
		/* storage can be unavailable in private mode */
	}
}

export async function loadEditorState(toolKey: string): Promise<EditorStateRecord | null> {
	if (isTauriRuntime()) {
		try {
			const result = await invokeCommand<EditorStateRecord | null>('get_editor_state', {
				tool_key: toolKey
			});
			if (result) return result;
		} catch (error) {
			console.warn('Failed to load editor state from SQLite', error);
		}
	}
	return readLocal<EditorStateRecord>(EDITOR_PREFIX + toolKey);
}

export async function saveEditorState(state: EditorStateRecord): Promise<void> {
	writeLocal(EDITOR_PREFIX + state.tool_key, state);
	if (!isTauriRuntime()) return;
	try {
		await invokeCommand('save_editor_state', {
			tool_key: state.tool_key,
			content: state.content,
			indent: state.indent,
			opt_format: state.opt_format,
			opt_unescape: state.opt_unescape
		});
	} catch (error) {
		console.warn('Failed to save editor state to SQLite', error);
	}
}

export async function getSetting(key: string): Promise<string | null> {
	if (isTauriRuntime()) {
		try {
			const value = await invokeCommand<string | null>('get_setting', { key });
			if (value !== null) return value;
		} catch (error) {
			console.warn('Failed to read setting from SQLite', error);
		}
	}
	return readLocal<string>(SETTING_PREFIX + key);
}

export async function setSetting(key: string, value: string): Promise<void> {
	writeLocal(SETTING_PREFIX + key, value);
	if (!isTauriRuntime()) return;
	try {
		await invokeCommand('set_setting', { key, value });
	} catch (error) {
		console.warn('Failed to save setting to SQLite', error);
	}
}

export async function processLargeFile(request: ProcessFileRequest): Promise<ProcessFileResult> {
	if (!isTauriRuntime()) {
		throw new Error('Large file processing is available in the desktop app only');
	}
	return invokeCommand<ProcessFileResult>('process_large_file', { ...request });
}

export async function saveTextFile(path: string, content: string): Promise<void> {
	if (!isTauriRuntime()) {
		throw new Error('Saving files is available in the desktop app only');
	}
	await invokeCommand('save_text_file', { path, content });
}
