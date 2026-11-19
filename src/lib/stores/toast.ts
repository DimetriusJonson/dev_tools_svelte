import { writable } from 'svelte/store';

export type ToastKind = 'success' | 'error' | 'info';

export interface ToastItem {
	id: number;
	kind: ToastKind;
	message: string;
}

const MAX_VISIBLE = 4;
const DEFAULT_DURATION: Record<ToastKind, number> = {
	success: 3200,
	info: 10000,
	error: 30000
};
let nextId = 1;

export const toasts = writable<ToastItem[]>([]);

export function dismissToast(id: number): void {
	toasts.update((items) => items.filter((item) => item.id !== id));
}

export function notify(
	message: string,
	kind: ToastKind = 'info',
	duration = DEFAULT_DURATION[kind]
): number {
	const id = nextId;
	nextId += 1;
	const item: ToastItem = { id, kind, message };
	toasts.update((items) => [...items.slice(-(MAX_VISIBLE - 1)), item]);
	if (typeof window !== 'undefined') {
		window.setTimeout(() => dismissToast(id), duration);
	}
	return id;
}

export function notifyError(error: unknown, fallback = 'Something went wrong'): number {
	const message =
		typeof error === 'string'
			? error
			: error instanceof Error
				? error.message
				: fallback;
	return notify(message || fallback, 'error');
}
