import { invokeCommand, isTauriRuntime } from './db';

export const MAX_UPLOAD_BYTES = 50 * 1024 * 1024;
export const SHARE_EVENT = 'share:deep-link';

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
	server: string
): Promise<ShareUploadResult> {
	if (!isTauriRuntime()) {
		throw new Error('Sharing is available in the desktop app only');
	}
	return invokeCommand<ShareUploadResult>('upload_share_content', {
		content,
		filename,
		content_type: contentType,
		server
	});
}

export async function uploadShareFile(path: string, server: string): Promise<ShareUploadResult> {
	if (!isTauriRuntime()) {
		throw new Error('Sharing is available in the desktop app only');
	}
	return invokeCommand<ShareUploadResult>('upload_share_file', { path, server });
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
