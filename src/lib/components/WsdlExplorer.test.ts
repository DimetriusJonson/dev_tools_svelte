import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import WsdlExplorer from './WsdlExplorer.svelte';
import { setLocale } from '$lib/stores/i18n';
import type { WsdlFileSummary } from '$lib/services/wsdl';

const operation = {
	id: 'svc::Op',
	name: 'CloseSession',
	soap_version: 'soap11' as const,
	soap_action: 'urn:CloseSession',
	endpoint_count: 1
};

const file: WsdlFileSummary = {
	id: 'file-1',
	project_id: 'p-1',
	name: 'avia.wsdl',
	source_url: 'http://127.0.0.1:9000/avia?wsdl',
	warnings: [],
	services: [{ id: 'svc', name: 'AviaService', operations: [operation] }],
	imported_at: '2026-11-22 10:00:00'
};

function menuItems(): HTMLButtonElement[] {
	return Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menu"] [role="menuitem"]'));
}

function click(element: Element | null | undefined): void {
	expect(element, 'menu item is rendered').toBeTruthy();
	(element as HTMLElement).click();
	flushSync();
}

let component: ReturnType<typeof mount> | undefined;

function render(onDeleteFile: (id: string) => void): void {
	component = mount(WsdlExplorer, {
		target: document.body,
		props: {
			projects: [{ id: 'p-1', name: 'Avia', created_at: '2026-11-22 10:00:00' }],
			files: [file],
			activeProjectId: 'p-1',
			activeOperationId: operation.id,
			onSelectProject: () => {},
			onCreateProject: () => {},
			onRenameProject: () => {},
			onDeleteProject: () => {},
			onImport: () => {},
			onCancelImport: () => {},
			onSelectOperation: () => {},
			onExecuteFile: () => {},
			onDeleteFile
		}
	});
	flushSync();
}

beforeEach(() => {
	setLocale('en');
	document.body.innerHTML = '';
});

afterEach(() => {
	if (component) unmount(component);
	component = undefined;
});

describe('WsdlExplorer file menu', () => {
	it('asks before deleting and calls the callback once confirmed', () => {
		const onDeleteFile = vi.fn();
		render(onDeleteFile);

		const trigger = document.querySelector<HTMLButtonElement>(`[aria-label="File actions"]`);
		click(trigger);
		expect(menuItems().map((item) => item.textContent?.trim())).toEqual(['Execute', 'Delete']);

		click(menuItems().find((item) => item.textContent?.trim() === 'Delete'));
		expect(onDeleteFile).not.toHaveBeenCalled();
		expect(document.body.textContent).toContain('Delete “avia.wsdl” with its services');
		expect(menuItems().map((item) => item.textContent?.trim())).toEqual(['Cancel', 'Delete']);

		click(menuItems().find((item) => item.textContent?.trim() === 'Delete'));
		expect(onDeleteFile).toHaveBeenCalledWith(file.id);
	});

	it('keeps the file when the confirmation is cancelled', () => {
		const onDeleteFile = vi.fn();
		render(onDeleteFile);

		click(document.querySelector<HTMLButtonElement>('[aria-label="File actions"]'));
		click(menuItems().find((item) => item.textContent?.trim() === 'Delete'));
		click(menuItems().find((item) => item.textContent?.trim() === 'Cancel'));

		expect(onDeleteFile).not.toHaveBeenCalled();
		expect(menuItems()).toHaveLength(0);
	});

	it('opens the same menu on right click of any file row', () => {
		const onDeleteFile = vi.fn();
		render(onDeleteFile);

		const row = document.querySelector<HTMLButtonElement>(`[title="${file.source_url}"]`);
		row?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 40 }));
		flushSync();

		expect(menuItems().map((item) => item.textContent?.trim())).toEqual(['Execute', 'Delete']);
		click(menuItems().find((item) => item.textContent?.trim() === 'Delete'));
		click(menuItems().find((item) => item.textContent?.trim() === 'Delete'));
		expect(onDeleteFile).toHaveBeenCalledWith(file.id);
	});
});
