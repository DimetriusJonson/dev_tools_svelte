import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import VariablesPage from './+page.svelte';
import { setLocale } from '$lib/stores/i18n';
import { REVEAL_MS } from '$lib/utils/vars';

interface Row {
	id: string;
	name: string;
	secure: boolean;
	value: string | null;
	created_at: string;
	updated_at: string;
}

const plain: Row = {
	id: 'v1',
	name: 'base',
	secure: false,
	value: 'https://api.example.com',
	created_at: '2026-01-01 00:00:00',
	updated_at: '2026-01-01 00:00:00'
};

const safe: Row = {
	id: 'v2',
	name: 'token',
	secure: true,
	value: null,
	created_at: '2026-01-01 00:00:00',
	updated_at: '2026-01-01 00:00:00'
};

let stored: Row[];
let calls: Array<[string, Record<string, unknown>]>;
let component: ReturnType<typeof mount> | undefined;

/**
* `isTauriRuntime()` only looks for this key and the Tauri API forwards every command to it, so one
* stub turns the page into the desktop build. The switch mirrors the backend: a safe row keeps no
* value in the column and answers `reveal_global_var` from the credential store.
*/
function desktopBuild(rows: Row[]): void {
	stored = rows.map((row) => ({ ...row }));
	calls = [];
	(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
		invoke: async (command: string, args: Record<string, unknown> = {}) => {
			calls.push([command, args]);
			if (command === 'list_global_vars') return stored;
			if (command === 'create_global_var') {
				const row: Row = {
					id: `new-${stored.length + 1}`,
					name: String(args.name),
					secure: Boolean(args.secure),
					value: args.secure ? null : String(args.value ?? ''),
					created_at: '2026-01-01 00:00:00',
					updated_at: '2026-01-01 00:00:00'
				};
				stored.push(row);
				return row;
			}
			if (command === 'update_global_var') {
				const row = stored.find((item) => item.id === args.id);
				if (!row) throw new Error('Global variable not found');
				const wasSecure = row.secure;
				row.name = String(args.name);
				row.secure = Boolean(args.secure);
				if (row.secure) {
					// A safe row keeps nothing in the column: `null` means "the
					// credential store entry is untouched".
					row.value = null;
				} else if (args.value === null) {
					// Turning a safe row plain reads its secret out of the store.
					row.value = wasSecure ? 's3cret' : (row.value ?? '');
				} else {
					row.value = String(args.value);
				}
				return row;
			}
			if (command === 'delete_global_var') {
				stored = stored.filter((item) => item.id !== args.id);
				return null;
			}
			if (command === 'reveal_global_var') {
				const row = stored.find((item) => item.id === args.id);
				return row?.secure ? 's3cret' : (row?.value ?? '');
			}
			return null;
		}
	};
}

/** The list reaches the page through IPC, so every assertion waits for that promise. */
async function until(assertion: () => void): Promise<void> {
	const started = Date.now();
	let last: unknown;
	while (Date.now() - started < 3000) {
		try {
			assertion();
			return;
		} catch (error) {
			last = error;
			await new Promise((resolve) => setTimeout(resolve, 20));
		}
	}
	throw new Error(
		`${(last as Error)?.message ?? 'assertion failed'}\nCommands sent: ${sentCommands().join(', ') || 'none'}`
	);
}

async function loaded(): Promise<void> {
	await until(() => expect(document.body.textContent).not.toContain('Global variables…'));
}

function sentCommands(): string[] {
	return calls.map(([command]) => command);
}

function argsOf(command: string): Record<string, unknown>[] {
	return calls.filter(([name]) => name === command).map(([, args]) => args);
}

function buttonByLabel(label: string): HTMLButtonElement | undefined {
	return [...document.querySelectorAll<HTMLButtonElement>('button')].find(
		(button) => button.getAttribute('aria-label') === label
	);
}

function buttonByText(label: string): HTMLButtonElement | undefined {
	return [...document.querySelectorAll<HTMLButtonElement>('button')].find(
		(button) => button.textContent?.trim() === label
	);
}

/**
* Svelte attaches `oninput` through a delegated listener on the root, so a typed event has to
* bubble for the page to see it.
*/
function type(input: HTMLInputElement, value: string): void {
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
}

function nameInputs(): HTMLInputElement[] {
	return [...document.querySelectorAll<HTMLInputElement>('input[aria-label="Name"]')];
}

function valueInput(row: HTMLElement): HTMLInputElement | null {
	return row.querySelector<HTMLInputElement>('input[aria-label="Value"]');
}

function rowOf(name: string): HTMLLIElement | undefined {
	return nameInputs()
		.map((input) => input.closest('li') as HTMLLIElement | null)
		.find((row) => row?.querySelector<HTMLInputElement>('input[aria-label="Name"]')?.value === name)
		?? undefined;
}

function draftName(): HTMLInputElement {
	return document.querySelector<HTMLInputElement>('#var-draft-name')!;
}

function draftValue(): HTMLInputElement {
	return document.querySelector<HTMLInputElement>('#var-draft-value')!;
}

function revealButton(): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>('button[title="Reveal the value"]');
}

beforeEach(() => {
	setLocale('en');
});

afterEach(() => {
	if (component) unmount(component);
	component = undefined;
	delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
	vi.useRealTimers();
	localStorage.clear();
});

describe('Global variables page', () => {
	it('shows only the desktop notice in the browser build', async () => {
		component = mount(VariablesPage, { target: document.body });
		flushSync();

		await until(() =>
			expect(document.body.textContent).toContain(
				'Global variables are available in the desktop version'
			)
		);
		// Nothing is loaded in the browser, so neither the loading line nor the draft may stay.
		expect(document.body.textContent).not.toContain('Global variables…');
		expect(document.querySelector('#var-draft-name')).toBeNull();
		expect(buttonByLabel('Add variable')).toBeUndefined();
	});

	it('lists a plain value and masks a safe one', async () => {
		desktopBuild([plain, safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		expect(nameInputs().map((input) => input.value)).toEqual(['base', 'token']);

	 const hidden = rowOf('token');
		expect(hidden?.querySelector('button[title="Reveal the value"]')).toBeTruthy();
		// The secret never reaches the list, so the safe row carries no value input.
		expect(valueInput(hidden!)).toBeNull();
		expect(hidden?.textContent).toContain('••••••');
		expect(document.body.textContent).not.toContain('s3cret');

		expect(valueInput(rowOf('base')!)?.value).toBe('https://api.example.com');
	});

	it('reveals a safe value on demand and hides it again after the timeout', async () => {
		desktopBuild([safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		// Fake timers make the 15 s reveal window observable without waiting for it.
		vi.useFakeTimers();
		revealButton()?.click();
		for (let step = 0; step < 20; step++) {
			await vi.advanceTimersByTimeAsync(10);
			if (valueInput(rowOf('token')!)?.value === 's3cret') break;
		}
		expect(valueInput(rowOf('token')!)?.value).toBe('s3cret');
		expect(argsOf('reveal_global_var')).toEqual([{ id: 'v2' }]);

		vi.advanceTimersByTime(REVEAL_MS + 50);
		flushSync();

		expect(valueInput(rowOf('token')!)).toBeNull();
		expect(rowOf('token')?.textContent).toContain('••••••');
	});

	it('writes a secret typed while revealed and hides it without saving', async () => {
		desktopBuild([safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		revealButton()?.click();
		for (let step = 0; step < 20; step++) {
			await vi.advanceTimersByTimeAsync(10);
			if (valueInput(rowOf('token')!)?.value === 's3cret') break;
		}

		type(valueInput(rowOf('token')!)!, 'rotated');
		// The rename debounce is 450 ms, so half a second is enough for the write.
		await vi.advanceTimersByTimeAsync(500);
		expect(argsOf('update_global_var')).toEqual([
			{ id: 'v2', name: 'token', secure: true, value: 'rotated' }
		]);

		// Hiding the field is not a write: the store keeps what was typed.
		buttonByLabel('Hide the value')?.click();
		flushSync();
		expect(valueInput(rowOf('token')!)).toBeNull();
		expect(rowOf('token')?.textContent).toContain('••••••');
		expect(argsOf('update_global_var')).toHaveLength(1);
	});

	it('creates a variable from the draft and keeps the lock state for the next one', async () => {
		desktopBuild([]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		type(draftName(), 'token');
		type(draftValue(), 's3cret');
		buttonByLabel('Keep this value in the application database')?.click();
		flushSync();

		buttonByLabel('Add variable')?.click();
		await until(() => expect(argsOf('create_global_var')).toHaveLength(1));
		expect(argsOf('create_global_var')[0]).toEqual({ name: 'token', secure: true, value: 's3cret' });

		// The name is cleared, the lock is not: the next safe variable does not start plain.
		await until(() => expect(draftName().value).toBe(''));
		expect(
			buttonByLabel('Keep this value in the system credential store')?.getAttribute('aria-pressed')
		).toBe('true');
	});

	it('refuses an illegal name before any command reaches the backend', async () => {
		desktopBuild([]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		type(draftName(), 'bad name');
		buttonByLabel('Add variable')?.click();
		flushSync();

		// The reason is in the input's title, and the typed text stays for correction.
		expect(draftName().getAttribute('title')).toContain('is not a valid name');
		expect(argsOf('create_global_var')).toHaveLength(0);
		expect(draftName().value).toBe('bad name');
	});

	it('saves a rename after the debounce and refuses a name another row owns', async () => {
		desktopBuild([plain, safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		type(nameInputs()[0], 'Base');
		await until(() => expect(argsOf('update_global_var')).toHaveLength(1));
		expect(argsOf('update_global_var')[0]).toEqual({
			id: 'v1',
			name: 'Base',
			secure: false,
			value: 'https://api.example.com'
		});

		// No write happens while the name duplicates an existing row.
		type(nameInputs()[1], 'BASE');
		await until(() => expect(nameInputs()[1].getAttribute('aria-invalid')).toBe('true'));
		expect(nameInputs()[1].getAttribute('title')).toContain('already exists');
		expect(argsOf('update_global_var')).toHaveLength(1);
	});

	it('asks before deleting and only then removes the row', async () => {
		desktopBuild([plain]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		buttonByLabel('base')?.click();
		flushSync();

		expect(argsOf('delete_global_var')).toHaveLength(0);
		expect(document.body.textContent).toContain('Delete “base” and its stored value?');

		buttonByText('Delete')?.click();
		await until(() => expect(argsOf('delete_global_var')).toEqual([{ id: 'v1' }]));
		await until(() => expect(nameInputs()).toHaveLength(0));
		expect(document.body.textContent).toContain('No variables yet');
	});

	it('confirms before a safe value moves into the database', async () => {
		desktopBuild([safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		buttonByLabel('Keep this value in the system credential store')?.click();
		flushSync();

		expect(argsOf('update_global_var')).toHaveLength(0);
		expect(document.body.textContent).toContain(
		'Store “token” in the application database instead of the credential store?'
		);

		buttonByText('Move the value into the database')?.click();
		await until(() => expect(argsOf('update_global_var')).toHaveLength(1));
		await until(() => expect(valueInput(rowOf('token')!)?.value).toBe('s3cret'));
		expect(revealButton()).toBeNull();
	});
});
