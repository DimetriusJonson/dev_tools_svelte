import { getSetting, invokeCommand, isTauriRuntime, setSetting } from './db';

export const MAX_UPLOAD_BYTES = 50 * 1024 * 1024;
export const SHARE_EVENT = 'share:deep-link';

/** How long an upload lives. `none` keeps the payload in RAM only, so its link dies with
 * the sharer's app; the day options store it with an expiry; `always` never expires. */
export type ShareRetention = 'none' | '1d' | '3d' | '7d' | 'always';

export const RETENTION_OPTIONS: ShareRetention[] = ['none', '1d', '3d', '7d', 'always'];
/** Uploads are memory-only unless the user opts into a stored lifetime, so an unknown or
 * missing stored value falls back here too and never writes a row it did not ask for. */
export const DEFAULT_RETENTION: ShareRetention = 'none';
export const RETENTION_SETTING_KEY = 'share.retention';

export function isRetention(value: unknown): value is ShareRetention {
	return (
		typeof value === 'string' && (RETENTION_OPTIONS as string[]).includes(value.trim().toLowerCase())
	);
}

/** Normalizes anything read from storage or a `<select>` into a retention code. */
export function normalizeRetention(value: unknown): ShareRetention {
	if (typeof value !== 'string') return DEFAULT_RETENTION;
	const code = value.trim().toLowerCase();
	return isRetention(code) ? code : DEFAULT_RETENTION;
}

export interface NetworkInterface {
	name: string;
	ip: string;
	url: string;
	is_loopback: boolean;
}

export interface ShareServerStatus {
	running: boolean;
	port: number;
	error: string | null;
}

export interface DeepLinkTarget {
	server: string;
	id: string;
}

export interface ShareUploadResult {
	id: string;
	link: string;
	server: string;
	filename: string;
	size: number;
	/** `false` when the payload stays in the sharer's RAM and dies with the app. */
	stored: boolean;
	/** `null` for a memory-only upload or one kept forever. */
	expires_at: string | null;
}

export interface SharedUploadResponse {
	id: string;
	filename: string;
	content_type: string;
	size: number;
	created_at: string;
	viewer: 'code' | 'binary';
	language: 'xml' | 'json' | 'text' | null;
	content: string | null;
	raw_url: string;
	stored: boolean;
	expires_at: string | null;
}

export async function listShareInterfaces(): Promise<NetworkInterface[]> {
	if (!isTauriRuntime()) return [];
	return invokeCommand<NetworkInterface[]>('list_share_interfaces');
}

export async function getShareServerStatus(): Promise<ShareServerStatus | null> {
	if (!isTauriRuntime()) return null;
	return invokeCommand<ShareServerStatus>('get_share_server_status');
}

export async function restartShareServer(): Promise<ShareServerStatus | null> {
	if (!isTauriRuntime()) return null;
	return invokeCommand<ShareServerStatus>('restart_share_server');
}

export async function uploadShareContent(
	content: string,
	filename: string,
	contentType: string,
	server: string,
	retention: ShareRetention
): Promise<ShareUploadResult> {
	if (!isTauriRuntime()) {
		throw new Error('Sharing is available in the desktop app only');
	}
	return invokeCommand<ShareUploadResult>('upload_share_content', {
		content,
		filename,
		content_type: contentType,
		server,
		retention
	});
}

export async function uploadShareFile(
	path: string,
	server: string,
	retention: ShareRetention
): Promise<ShareUploadResult> {
	if (!isTauriRuntime()) {
		throw new Error('Sharing is available in the desktop app only');
	}
	return invokeCommand<ShareUploadResult>('upload_share_file', { path, server, retention });
}

export async function loadSharedUpload(
	server: string,
	id: string
): Promise<SharedUploadResponse> {
	if (!isTauriRuntime()) {
		throw new Error('Shared uploads are available in the desktop app only');
	}
	return invokeCommand<SharedUploadResponse>('load_shared_upload', { server, id });
}

export async function downloadSharedUpload(
	server: string,
	id: string,
	path: string
): Promise<void> {
	if (!isTauriRuntime()) {
		throw new Error('Downloads are available in the desktop app only');
	}
	await invokeCommand('download_shared_upload', { server, id, path });
}

export async function deleteSharedUpload(server: string, id: string): Promise<void> {
	if (!isTauriRuntime()) {
		throw new Error('Shared uploads are available in the desktop app only');
	}
	await invokeCommand('delete_shared_upload', { server, id });
}

export async function consumePendingDeepLink(): Promise<DeepLinkTarget | null> {
	if (!isTauriRuntime()) return null;
	return invokeCommand<DeepLinkTarget | null>('consume_pending_deep_link');
}

export async function clearPendingDeepLink(): Promise<void> {
	if (!isTauriRuntime()) return;
	await invokeCommand('clear_pending_deep_link');
}

export async function listenForDeepLinks(
	handler: (target: DeepLinkTarget) => void
): Promise<() => void> {
	if (!isTauriRuntime()) return () => {};
	const { listen } = await import('@tauri-apps/api/event');
	return listen<DeepLinkTarget>(SHARE_EVENT, (event) => {
		void clearPendingDeepLink().catch(() => {});
		handler(event.payload);
	});
}

export function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function buildViewerUrl(target: DeepLinkTarget): string {
	const params = new URLSearchParams({ server: target.server, id: target.id });
	return `/view?${params.toString()}`;
}

/** The retention the Share page preselects. Settings mirror through SQLite and localStorage. */
export async function loadRetention(): Promise<ShareRetention> {
	return normalizeRetention(await getSetting(RETENTION_SETTING_KEY));
}

export async function saveRetention(retention: ShareRetention): Promise<void> {
	await setSetting(RETENTION_SETTING_KEY, retention);
}
