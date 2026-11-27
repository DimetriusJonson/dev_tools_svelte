import { invokeCommand, isTauriRuntime } from './db';

const DESKTOP_ONLY = 'Global variables are available in the desktop app only';

export interface GlobalVar {
	id: string;
	name: string;
	secure: boolean;
	/** `null` for a secure variable: only the keyring holds its value. */
	value: string | null;
	created_at: string;
	updated_at: string;
}

export interface GlobalVarDraft {
	name: string;
	secure: boolean;
	value: string;
}

export interface GlobalVarUpdate {
	id: string;
	name: string;
	secure: boolean;
	/** `null` means "keep the value the row already has". */
	value: string | null;
}

export function isVarsSupported(): boolean {
	return isTauriRuntime();
}

export async function listGlobalVars(): Promise<GlobalVar[]> {
	if (!isVarsSupported()) throw new Error(DESKTOP_ONLY);
	return invokeCommand<GlobalVar[]>('list_global_vars');
}

export async function createGlobalVar(draft: GlobalVarDraft): Promise<GlobalVar> {
	if (!isVarsSupported()) throw new Error(DESKTOP_ONLY);
	return invokeCommand<GlobalVar>('create_global_var', {
		name: draft.name,
		secure: draft.secure,
		value: draft.value
	});
}

export async function updateGlobalVar(update: GlobalVarUpdate): Promise<GlobalVar> {
	if (!isVarsSupported()) throw new Error(DESKTOP_ONLY);
	return invokeCommand<GlobalVar>('update_global_var', {
		id: update.id,
		name: update.name,
		secure: update.secure,
		value: update.value
	});
}

export async function deleteGlobalVar(id: string): Promise<void> {
	if (!isVarsSupported()) throw new Error(DESKTOP_ONLY);
	await invokeCommand<void>('delete_global_var', { id });
}

/**
 * The only path a secret takes out of the keyring. The caller keeps it in page
 * memory and drops it after the reveal timeout.
 */
export async function revealGlobalVar(id: string): Promise<string> {
	if (!isVarsSupported()) throw new Error(DESKTOP_ONLY);
	return invokeCommand<string>('reveal_global_var', { id });
}
