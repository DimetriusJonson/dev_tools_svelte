import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';
import WsdlExplorer from './WsdlExplorer.svelte';
import { setLocale } from '$lib/stores/i18n';
import type { WsdlFileSummary, WsdlProject } from '$lib/services/wsdl';

const operation = {
	id: 'svc::Op',
	name: 'CloseSession',
	soap_version: 'soap11' as const,
	soap_action: 'urn:CloseSession',
	endpoint_count: 1
};

const stockOperation = {
	id: 'svc-2::Op',
	name: 'GetOrders',
	soap_version: 'soap12' as const,
	soap_action: '',
	endpoint_count: 2
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

/** Carries two warnings so the row's tooltip text is checkable. */
const stockFile: WsdlFileSummary = {
	id: 'file-2',
	project_id: 'p-1',
	name: 'stock.wsdl',
	source_url: 'http://127.0.0.1:9000/stock?wsdl',
	warnings: ['no-binding:StockService::StockPort', 'unknown-type:tns:Address'],
	services: [{ id: 'svc-2', name: 'StockService', operations: [stockOperation] }],
	imported_at: '2026-11-22 11:00:00'
};

const project: WsdlProject = { id: 'p-1', name: 'Avia', created_at: '2026-11-22 10:00:00' };

interface Props {
	projects?: WsdlProject[];
	files?: WsdlFileSummary[];
	activeProjectId?: string | null;
	activeOperationId?: string | null;
	loading?: boolean;
	importing?: boolean;
	onSelectProject: (id: string | null) => void;
	onCreateProject: (name: string) => void;
	onRenameProject: (id: string, name: string) => void;
	onDeleteProject: (id: string) => void;
	onImport: () => void;
	onCancelImport: () => void;
	onSelectOperation: (id: string) => void;
	onExecuteFile: (fileId: string) => void;
	onDeleteFile: (fileId: string) => void;
}

interface Spies {
	onSelectProject: Mock<(id: string | null) => void>;
	onCreateProject: Mock<(name: string) => void>;
	onRenameProject: Mock<(id: string, name: string) => void>;
	onDeleteProject: Mock<(id: string) => void>;
	onImport: Mock<() => void>;
	onCancelImport: Mock<() => void>;
	onSelectOperation: Mock<(id: string) => void>;
	onExecuteFile: Mock<(fileId: string) => void>;
	onDeleteFile: Mock<(fileId: string) => void>;
}

let component: ReturnType<typeof mount> | undefined;

function render(props: Partial<Props> = {}): Spies {
	const spies: Spies = {
		onSelectProject: vi.fn((_id: string | null) => {}),
		onCreateProject: vi.fn((_name: string) => {}),
		onRenameProject: vi.fn((_id: string, _name: string) => {}),
		onDeleteProject: vi.fn((_id: string) => {}),
		onImport: vi.fn(() => {}),
		onCancelImport: vi.fn(() => {}),
		onSelectOperation: vi.fn((_id: string) => {}),
		onExecuteFile: vi.fn((_fileId: string) => {}),
		onDeleteFile: vi.fn((_fileId: string) => {})
	};
	component = mount(WsdlExplorer, {
		target: document.body,
		props: {
			projects: props.projects ?? [project],
			files: props.files ?? [file],
			activeProjectId: 'activeProjectId' in props ? props.activeProjectId ?? null : 'p-1',
			activeOperationId: 'activeOperationId' in props ? props.activeOperationId ?? null : operation.id,
			loading: props.loading ?? false,
			importing: props.importing ?? false,
			onSelectProject: props.onSelectProject ?? spies.onSelectProject,
			onCreateProject: props.onCreateProject ?? spies.onCreateProject,
			onRenameProject: props.onRenameProject ?? spies.onRenameProject,
			onDeleteProject: props.onDeleteProject ?? spies.onDeleteProject,
			onImport: props.onImport ?? spies.onImport,
			onCancelImport: props.onCancelImport ?? spies.onCancelImport,
			onSelectOperation: props.onSelectOperation ?? spies.onSelectOperation,
			onExecuteFile: props.onExecuteFile ?? spies.onExecuteFile,
			onDeleteFile: props.onDeleteFile ?? spies.onDeleteFile
		}
	});
	flushSync();
	return spies;
}

function menuItems(): HTMLButtonElement[] {
	return Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menu"] [role="menuitem"]'));
}

function item(label: string): HTMLButtonElement | undefined {
	return menuItems().find((menuItem) => menuItem.textContent?.trim() === label);
}

function click(element: Element | null | undefined): void {
	expect(element, 'menu item is rendered').toBeTruthy();
	(element as HTMLElement).click();
	flushSync();
}

function fileRow(url: string): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>(`[title="${url}"]`);
}

function serviceRow(name: string): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>(`[title="${name}"]`);
}

/** Rows carry their own name in `title`; the chevron rows additionally have an aria-label. */
function operationRow(name: string): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>(`button[title="${name}"]`);
}

function fileActions(): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>('[aria-label="File actions"]');
}

function projectActions(): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>('[aria-label="Project actions"]');
}

function projectInput(): HTMLInputElement | null {
	return document.querySelector<HTMLInputElement>('input[aria-label="Project name"]');
}

function filterInput(): HTMLInputElement {
	const input = document.querySelector<HTMLInputElement>('input[aria-label="Filter operations by name"]');
	expect(input, 'the filter input is rendered').toBeTruthy();
	return input as HTMLInputElement;
}

function filterClearButton(): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>('[aria-label="Clear filter"]');
}

function typeText(input: HTMLInputElement, value: string): void {
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

function pressKey(input: HTMLInputElement, key: string): void {
	input.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
	flushSync();
}

function blur(input: HTMLInputElement): void {
	input.dispatchEvent(new Event('blur'));
	flushSync();
}

function rightClick(element: Element | null, x = 40, y = 40): void {
	expect(element, 'tree row is rendered').toBeTruthy();
	element?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: x, clientY: y }));
	flushSync();
}

function pressEscape(): void {
	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
	flushSync();
}

function clickOutside(): void {
	document.body.click();
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
		const spies = render();

		click(fileActions());
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Execute', 'Delete']);

		click(item('Delete'));
		expect(spies.onDeleteFile).not.toHaveBeenCalled();
		expect(document.body.textContent).toContain('Delete “avia.wsdl” with its services');
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Cancel', 'Delete']);

		click(item('Delete'));
		expect(spies.onDeleteFile).toHaveBeenCalledWith(file.id);
		expect(menuItems()).toHaveLength(0);
	});

	it('keeps the file when the confirmation is cancelled', () => {
		const spies = render();

		click(fileActions());
		click(item('Delete'));
		click(item('Cancel'));

		expect(spies.onDeleteFile).not.toHaveBeenCalled();
		expect(menuItems()).toHaveLength(0);
	});

	it('executes the file without a confirmation', () => {
		const spies = render();

		click(fileActions());
		click(item('Execute'));

		expect(spies.onExecuteFile).toHaveBeenCalledWith(file.id);
		expect(spies.onDeleteFile).not.toHaveBeenCalled();
		expect(menuItems()).toHaveLength(0);
	});

	it('opens the same menu on right click of any file row', () => {
		const spies = render({ files: [file, stockFile], activeOperationId: operation.id });

		rightClick(fileRow(stockFile.source_url));
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Execute', 'Delete']);

		click(item('Delete'));
		expect(spies.onDeleteFile).not.toHaveBeenCalled();
		expect(document.body.textContent).toContain('Delete “stock.wsdl” with its services');

		click(item('Delete'));
		expect(spies.onDeleteFile).toHaveBeenCalledWith(stockFile.id);
	});

	it('leaves the actions button on the file of the selected operation only', () => {
		render({ files: [file, stockFile], activeOperationId: operation.id });

		const actions = fileActions();
		const row = fileRow(file.source_url);
		expect(actions?.parentElement).toBe(row?.parentElement);
		expect(document.querySelectorAll('[aria-label="File actions"]')).toHaveLength(1);
	});

	it('drops the pending confirmation when the menu opens again', () => {
		render({ files: [file, stockFile], activeOperationId: operation.id });

		click(fileActions());
		click(item('Delete'));
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Cancel', 'Delete']);

		rightClick(fileRow(stockFile.source_url));
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Execute', 'Delete']);

		clickOutside();
		click(fileActions());
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Execute', 'Delete']);
	});

	it('closes the menu on Escape and on a click outside it', () => {
		render();

		click(fileActions());
		pressEscape();
		expect(menuItems()).toHaveLength(0);

		click(fileActions());
		clickOutside();
		expect(menuItems()).toHaveLength(0);
	});

	it('closes the menu when its trigger is clicked again', () => {
		render();

		click(fileActions());
		expect(menuItems().length).toBeGreaterThan(0);
		click(fileActions());
		expect(menuItems()).toHaveLength(0);
	});

	it('never lets a menu click reach the window dismiss handler', () => {
		const spies = render();

		const reached: string[] = [];
		const watch = (event: Event) => {
			reached.push((event.target as HTMLElement).textContent?.trim() ?? '');
		};
		window.addEventListener('click', watch);

		click(fileActions());
		click(item('Delete'));
		click(item('Cancel'));
		click(projectActions());
		click(item('Delete project'));

		window.removeEventListener('click', watch);

		expect(reached).toEqual([]);
		expect(spies.onDeleteFile).not.toHaveBeenCalled();
		expect(spies.onDeleteProject).toHaveBeenCalledWith(project.id);
	});

	it('keeps the confirmation when the browser detaches the item mid-propagation', () => {
		render();

		click(fileActions());
		click(item('Delete'));
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Cancel', 'Delete']);

		// A browser applies the state change inside the item handler, so by the time
		// the click bubbles up the button is gone from the document and
		// `closest('[role="menu"]')` finds nothing. The window handler must not read
		// that as a click outside and wipe the confirmation.
		const detached = document.createElement('button');
		detached.setAttribute('role', 'menuitem');
		detached.textContent = 'Delete';
		const event = new MouseEvent('click', { bubbles: true });
		Object.defineProperty(event, 'target', { value: detached });
		document.body.dispatchEvent(event);
		flushSync();

		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual(['Cancel', 'Delete']);
	});

	it('renders the localized confirmation in Russian', () => {
		setLocale('ru');
		const spies = render();

		click(document.querySelector('[aria-label="Действия с файлом"]'));
		click(item('Удалить'));
		expect(spies.onDeleteFile).not.toHaveBeenCalled();
		expect(document.body.textContent).toContain(
			'Удалить «avia.wsdl» со службами, операциями и ответами?'
		);

		click(item('Удалить'));
		expect(spies.onDeleteFile).toHaveBeenCalledWith(file.id);
	});
});

describe('WsdlExplorer tree', () => {
	it('opens the branch that holds the selected operation', () => {
		render({ files: [file, stockFile], activeOperationId: stockOperation.id });

		expect(fileRow(stockFile.source_url)?.getAttribute('aria-expanded')).toBe('true');
		expect(serviceRow('StockService')?.getAttribute('aria-expanded')).toBe('true');
		expect(operationRow('GetOrders')).toBeTruthy();
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('false');
		expect(operationRow('CloseSession')).toBeNull();
	});

	it('names the chevron row with the localized expand and collapse label', () => {
		render({ files: [file] });

		expect(fileRow(file.source_url)?.getAttribute('aria-label')).toBe('avia.wsdl · Hide services');
		expect(serviceRow('AviaService')?.getAttribute('aria-label')).toBe(
			'AviaService · Hide operations'
		);

		click(fileRow(file.source_url));
		expect(fileRow(file.source_url)?.getAttribute('aria-label')).toBe('avia.wsdl · Show services');
	});

	it('keeps a manually collapsed branch of the selected operation closed', () => {
		render({ files: [file] });

		click(fileRow(file.source_url));
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('false');
		expect(operationRow('CloseSession')).toBeNull();

		click(fileRow(file.source_url));
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('true');
		expect(operationRow('CloseSession')).toBeTruthy();
	});

	it('keeps a manually collapsed service branch closed', () => {
		render({ files: [file] });

		click(serviceRow('AviaService'));
		expect(serviceRow('AviaService')?.getAttribute('aria-expanded')).toBe('false');
		expect(operationRow('CloseSession')).toBeNull();
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('true');
	});

	it('selecting an operation opens both branches and reports the id', () => {
		const spies = render({ activeOperationId: null });

		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('false');
		click(fileRow(file.source_url));
		click(serviceRow('AviaService'));
		click(operationRow('CloseSession'));

		expect(spies.onSelectOperation).toHaveBeenCalledWith(operation.id);
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('true');
		expect(serviceRow('AviaService')?.getAttribute('aria-expanded')).toBe('true');
	});

	it('lists the import warnings in the row tooltip', () => {
		render({ files: [file, stockFile], activeOperationId: stockOperation.id });

		expect(fileRow(file.source_url)?.querySelector('[title^="A port has no SOAP binding"]')).toBeNull();
		const warning = fileRow(stockFile.source_url)?.querySelector<HTMLSpanElement>(
			'[title^="A port has no SOAP binding"]'
		);
		expect(warning?.getAttribute('title')).toBe(
			'A port has no SOAP binding: StockService::StockPort\n' +
				'A type reference stays unresolved: tns:Address'
		);
	});

	it('shows the loading and empty states', () => {
		render({ loading: true });
		expect(document.body.textContent).toContain('Loading WSDL files…');
		unmount(component as ReturnType<typeof mount>);
		component = undefined;

		render({ files: [] });
		expect(document.body.textContent).toContain('No WSDL files yet');
		expect(document.body.textContent).toContain('Create a project and import a WSDL file');
		expect(document.body.textContent).not.toContain('Loading WSDL files…');
	});
});

describe('WsdlExplorer operation filter', () => {
	it('keeps the matching operations with their service and file rows', () => {
		render({ files: [file, stockFile], activeOperationId: operation.id });

		click(fileRow(stockFile.source_url));
		click(serviceRow('StockService'));
		expect(operationRow('CloseSession')).toBeTruthy();
		expect(operationRow('GetOrders')).toBeTruthy();

		typeText(filterInput(), 'order');

		expect(operationRow('GetOrders')).toBeTruthy();
		expect(operationRow('CloseSession')).toBeNull();
		expect(serviceRow('StockService')).toBeTruthy();
		expect(serviceRow('AviaService')).toBeNull();
		expect(fileRow(file.source_url)).toBeNull();
		expect(fileRow(stockFile.source_url)).toBeTruthy();
	});

	it('matches case-insensitively and ignores the surrounding spaces', () => {
		render({ files: [file, stockFile] });

		typeText(filterInput(), '  ORD  ');
		expect(operationRow('GetOrders')).toBeTruthy();
		expect(operationRow('CloseSession')).toBeNull();
	});

	it('reveals matches inside a manually collapsed branch without touching the toggle', () => {
		render({ files: [file] });

		click(fileRow(file.source_url));
		expect(operationRow('CloseSession')).toBeNull();

		typeText(filterInput(), 'close');
		expect(operationRow('CloseSession')).toBeTruthy();
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('true');

		// The manual collapse belongs to the unfiltered view, so it survives the filter.
		typeText(filterInput(), '');
		expect(operationRow('CloseSession')).toBeNull();
		expect(fileRow(file.source_url)?.getAttribute('aria-expanded')).toBe('false');
	});

	it('says when no operation matches', () => {
		render({ files: [file, stockFile], activeOperationId: operation.id });

		typeText(filterInput(), 'zzz');

		expect(document.body.textContent).toContain('No operation matches the filter');
		expect(document.body.textContent).not.toContain('No WSDL files yet');
		expect(operationRow('CloseSession')).toBeNull();
		expect(fileRow(file.source_url)).toBeNull();
	});

	it('clears through the button and Escape, and drops the file actions meanwhile', () => {
		render({ files: [file, stockFile], activeOperationId: operation.id });

		click(fileRow(stockFile.source_url));
		click(serviceRow('StockService'));
		expect(operationRow('GetOrders')).toBeTruthy();
		expect(document.querySelectorAll('[aria-label="File actions"]')).toHaveLength(1);
		expect(filterClearButton()).toBeNull();

		typeText(filterInput(), 'order');
		expect(operationRow('CloseSession')).toBeNull();
		// The selected operation is filtered out, so no row owns it any more.
		expect(document.querySelectorAll('[aria-label="File actions"]')).toHaveLength(0);
		click(filterClearButton());
		expect(filterInput().value).toBe('');
		expect(operationRow('CloseSession')).toBeTruthy();
		expect(document.querySelectorAll('[aria-label="File actions"]')).toHaveLength(1);

		typeText(filterInput(), 'close');
		expect(operationRow('GetOrders')).toBeNull();
		pressKey(filterInput(), 'Escape');
		expect(filterInput().value).toBe('');
		expect(operationRow('GetOrders')).toBeTruthy();
	});

	it('renders the localized label and empty state in Russian', () => {
		setLocale('ru');
		render({ files: [file] });

		const input = document.querySelector('input[aria-label="Фильтр операций по названию"]');
		expect(input?.getAttribute('placeholder')).toBe('Фильтр операций…');

		typeText(input as HTMLInputElement, 'нет');
		expect(document.body.textContent).toContain('Ни одна операция не подходит под фильтр');
	});
});

describe('WsdlExplorer project menu', () => {
	it('disables rename and delete without a selected project', () => {
		render({ projects: [], activeProjectId: null });

		click(projectActions());
		expect(menuItems().map((menuItem) => menuItem.textContent?.trim())).toEqual([
			'Create project',
			'Rename project',
			'Delete project'
		]);
		expect(item('Rename project')?.disabled).toBe(true);
		expect(item('Delete project')?.disabled).toBe(true);

		click(item('Create project'));
		expect(projectInput()).toBeTruthy();
	});

	it('creates a trimmed project name on Enter and on blur', () => {
		const spies = render();

		click(projectActions());
		click(item('Create project'));
		typeText(projectInput() as HTMLInputElement, '  Stock  ');
		pressKey(projectInput() as HTMLInputElement, 'Enter');

		expect(spies.onCreateProject).toHaveBeenCalledWith('Stock');
		expect(projectInput()).toBeNull();

		click(projectActions());
		click(item('Create project'));
		typeText(projectInput() as HTMLInputElement, 'Extra');
		blur(projectInput() as HTMLInputElement);

		expect(spies.onCreateProject).toHaveBeenCalledTimes(2);
		expect(spies.onCreateProject).toHaveBeenLastCalledWith('Extra');
		expect(projectInput()).toBeNull();
	});

	it('does not create a project from an empty name', () => {
		const spies = render();

		click(projectActions());
		click(item('Create project'));
		pressKey(projectInput() as HTMLInputElement, 'Enter');

		expect(spies.onCreateProject).not.toHaveBeenCalled();
		expect(projectInput()).toBeNull();
	});

	it('cancels the create input on Escape', () => {
		const spies = render();

		click(projectActions());
		click(item('Create project'));
		typeText(projectInput() as HTMLInputElement, 'Stock');
		pressKey(projectInput() as HTMLInputElement, 'Escape');

		expect(spies.onCreateProject).not.toHaveBeenCalled();
		expect(projectInput()).toBeNull();
	});

	it('renames the active project on Enter only when the name changed', () => {
		const spies = render();

		click(projectActions());
		click(item('Rename project'));
		const input = projectInput() as HTMLInputElement;
		expect(input.value).toBe('Avia');
		pressKey(input, 'Enter');
		expect(spies.onRenameProject).not.toHaveBeenCalled();

		click(projectActions());
		click(item('Rename project'));
		typeText(projectInput() as HTMLInputElement, 'Avia EU');
		pressKey(projectInput() as HTMLInputElement, 'Enter');

		expect(spies.onRenameProject).toHaveBeenCalledWith(project.id, 'Avia EU');
		expect(projectInput()).toBeNull();
	});

	it('renames on blur and cancels on Escape', () => {
		const spies = render();

		click(projectActions());
		click(item('Rename project'));
		typeText(projectInput() as HTMLInputElement, 'Avia EU');
		blur(projectInput() as HTMLInputElement);
		expect(spies.onRenameProject).toHaveBeenCalledWith(project.id, 'Avia EU');

		click(projectActions());
		click(item('Rename project'));
		typeText(projectInput() as HTMLInputElement, 'Avia US');
		pressKey(projectInput() as HTMLInputElement, 'Escape');
		expect(spies.onRenameProject).toHaveBeenCalledTimes(1);
		expect(projectInput()).toBeNull();
	});

	it('deletes the selected project', () => {
		const spies = render();

		click(projectActions());
		click(item('Delete project'));

		expect(spies.onDeleteProject).toHaveBeenCalledWith(project.id);
		expect(menuItems()).toHaveLength(0);
	});

	it('selects a project from the dropdown and the no-project option', () => {
		const spies = render({ projects: [project], activeProjectId: null });

		const select = document.querySelector<HTMLSelectElement>('select');
		expect(select?.value).toBe('');
		expect(select?.querySelector(`option[value="${project.id}"]`)).toBeTruthy();

		select!.value = project.id;
		select!.dispatchEvent(new Event('change', { bubbles: true }));
		flushSync();
		expect(spies.onSelectProject).toHaveBeenCalledWith(project.id);

		select!.value = '';
		select!.dispatchEvent(new Event('change', { bubbles: true }));
		flushSync();
		expect(spies.onSelectProject).toHaveBeenLastCalledWith(null);
	});
});

describe('WsdlExplorer import controls', () => {
	it('imports from the clipboard while idle', () => {
		const spies = render();

		click(document.querySelector('[aria-label="Import WSDL from clipboard"]'));

		expect(spies.onImport).toHaveBeenCalled();
		expect(spies.onCancelImport).not.toHaveBeenCalled();
	});

	it('turns the import button into Cancel while a fetch runs', () => {
		const spies = render({ importing: true });

		expect(document.querySelector('[aria-label="Import WSDL from clipboard"]')).toBeNull();
		click(document.querySelector('[aria-label="Cancel import"]'));

		expect(spies.onCancelImport).toHaveBeenCalled();
		expect(spies.onImport).not.toHaveBeenCalled();
	});

	it('opens the help popover and keeps it open for clicks inside it', () => {
		render();

		const help = document.querySelector<HTMLButtonElement>('[aria-label="Import help"]');
		click(help);
		const dialog = document.querySelector('[role="dialog"]');
		expect(dialog).toBeTruthy();
		expect(dialog?.textContent).toContain('Copy a WSDL url to the clipboard');
		expect(help?.getAttribute('aria-expanded')).toBe('true');

		click(dialog);
		expect(document.querySelector('[role="dialog"]')).toBeTruthy();

		clickOutside();
		expect(document.querySelector('[role="dialog"]')).toBeNull();
		expect(document.querySelector('[aria-label="Import help"]')?.getAttribute('aria-expanded')).toBe(
			'false'
		);
	});

	it('closes the help popover on Escape and on a second click of the button', () => {
		render();

		click(document.querySelector('[aria-label="Import help"]'));
		pressEscape();
		expect(document.querySelector('[role="dialog"]')).toBeNull();

		click(document.querySelector('[aria-label="Import help"]'));
		expect(document.querySelector('[role="dialog"]')).toBeTruthy();
		click(document.querySelector('[aria-label="Import help"]'));
		expect(document.querySelector('[role="dialog"]')).toBeNull();
	});

	it('does not open a second floating panel at once', () => {
		render();

		click(document.querySelector('[aria-label="Import help"]'));
		click(projectActions());

		expect(document.querySelectorAll('[role="menu"], [role="dialog"]')).toHaveLength(1);
	});
});
