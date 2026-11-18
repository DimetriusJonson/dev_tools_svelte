import { writable } from 'svelte/store';

export interface ShareDraft {
	content: string;
	filename: string;
	contentType: string;
	language: 'xml' | 'json';
	readOnly?: boolean;
}

export const draftShare = writable<ShareDraft | null>(null);
