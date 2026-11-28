import { flushSync, mount, unmount } from 'svelte';
import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import VariablesPage from './+page.svelte';
import { setLocale } from '$lib/stores/i18n';
import { toasts } from '$lib/stores/toast';
import { REVEAL_MS } from '$lib/utils/vars';

interface Row {
	id: string;
	name: string;
	secure: boolean;
	value: string | null;
	created_at: string;
	updated_at: string;
	/** The credential-store entry of a safe row; never part of the list response. */
	secret?: string;
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
	secret: 's3cret',
	created_at: '2026-01-01 00:00:00',
	updated_at: '2026-01-01 00:00:00'
};

let stored: Row[];
let secrets: Record<string, string>;
let storeBroken = false;
/** Set to make `list_global_vars` fail; an empty string is a failure with no text. */
let listFailure: string | null = null;
let calls: Array<[string, Record<string, unknown>]>;
let component: ReturnType<typeof mount> | undefined;

/** A Rust command rejects with its error **string**, not an `Error`, so neither is wrapped. */
function failBackend(text: string): never {
	throw text;
}

/**
* `isTauriRuntime()` only looks for this key and the Tauri API forwards every command to it, so one
* stub turns the page into the desktop build. The switch mirrors the backend: a safe row keeps no
* value in the column and answers `reveal_global_var` from the credential store.
*/
function desktopBuild(rows: Row[]): void {
	stored = rows.map((row) => ({ ...row }));
	secrets = {};
	for (const row of stored) {
		if (row.secure && row.secret) secrets[row.id] = row.secret;
	}
	storeBroken = false;
	listFailure = null;
	calls = [];
	(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
		invoke: async (command: string, args: Record<string, unknown> = {}) => {
			calls.push([command, args]);
			if (command === 'list_global_vars') {
				if (listFailure !== null) failBackend(listFailure);
				return stored.map(({ secret: _secret, ...row }) => row);
			}
			if (command === 'create_global_var') {
				if (storeBroken && args.secure) failBackend('secret-store:keyring unavailable');
				const row: Row = {
					id: `new-${stored.length + 1}`,
					name: String(args.name),
					secure: Boolean(args.secure),
					value: args.secure ? null : String(args.value ?? ''),
					created_at: '2026-01-01 00:00:00',
					updated_at: '2026-01-01 00:00:00'
				};
				if (row.secure && args.value) secrets[row.id] = String(args.value);
				stored.push(row);
				return row;
			}
			if (command === 'update_global_var') {
				if (storeBroken && args.secure) failBackend('secret-store:keyring unavailable');
				const row = stored.find((item) => item.id === args.id);
				if (!row) failBackend('Global variable not found');
				const wasSecure = row.secure;
				row.name = String(args.name);
				row.secure = Boolean(args.secure);
				if (row.secure) {
					// A safe row keeps nothing in the column: `null` means "the
					// credential store entry is untouched".
					if (args.value !== null) {
						if (args.value === '') delete secrets[row.id];
						else secrets[row.id] = String(args.value);
					}
					row.value = null;
				} else {
					if (args.value === null) {
						// Turning a safe row plain reads its secret out of the store.
						row.value = wasSecure ? (secrets[row.id] ?? '') : (row.value ?? '');
					} else {
						row.value = String(args.value);
					}
					// The backend moves the secret out of the store either way.
					if (wasSecure) delete secrets[row.id];
				}
				return row;
			}
			if (command === 'delete_global_var') {
				stored = stored.filter((item) => item.id !== args.id);
				delete secrets[String(args.id)];
				return null;
			}
			if (command === 'reveal_global_var') {
				const row = stored.find((item) => item.id === args.id);
				return row?.secure ? (secrets[row.id] ?? '') : (row?.value ?? '');
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

/** A safe row whose credential store holds nothing says so instead of showing dots. */
function emptySecretButton(): HTMLButtonElement | null {
	return document.querySelector<HTMLButtonElement>('button[title="No value stored yet"]');
}

/** The plain/safe toggle of one row, the only `aria-pressed` control inside it. */
function typeToggle(row: HTMLElement): HTMLButtonElement {
	return row.querySelector<HTMLButtonElement>('button[aria-pressed]')!;
}

/** What the fake credential store holds; the variable list never carries a secret. */
function credentialOf(id: string): string | undefined {
	return secrets[id];
}

beforeEach(() => {
	setLocale('en');
	toasts.set([]);
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

	it('moves the value typed moments before the lock into the credential store', async () => {
		desktopBuild([plain]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		type(valueInput(rowOf('base')!)!, 'https://new.example.com');
		// Inside the debounce window: the column still holds the older text, so the
		// toggle has to flush the edit instead of securing what the row was before.
		typeToggle(rowOf('base')!).click();
		await vi.advanceTimersByTimeAsync(600);
		flushSync();

		expect(argsOf('update_global_var')).toEqual([
			{ id: 'v1', name: 'base', secure: false, value: 'https://new.example.com' },
			{ id: 'v1', name: 'base', secure: true, value: 'https://new.example.com' }
		]);
		expect(credentialOf('v1')).toBe('https://new.example.com');
	});

	it('writes a secret typed at the edge of the reveal window before hiding it', async () => {
		desktopBuild([safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		revealButton()?.click();
		await vi.advanceTimersByTimeAsync(50);
		expect(valueInput(rowOf('token')!)?.value).toBe('s3cret');

		// Typing 300 ms before the window closes leaves the 450 ms debounce pending:
		// the expiry has to write the text, not drop it with the field.
		await vi.advanceTimersByTimeAsync(REVEAL_MS - 300);
		type(valueInput(rowOf('token')!)!, 'rotated');
		await vi.advanceTimersByTimeAsync(REVEAL_MS + 200);
		flushSync();

		expect(argsOf('update_global_var')).toEqual([
			{ id: 'v2', name: 'token', secure: true, value: 'rotated' }
		]);
		expect(credentialOf('v2')).toBe('rotated');
		expect(rowOf('token')?.textContent).toContain('••••••');
	});

	it('keeps a refused secret on screen and in the store', async () => {
		desktopBuild([safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		revealButton()?.click();
		await vi.advanceTimersByTimeAsync(50);
		storeBroken = true;

		type(valueInput(rowOf('token')!)!, 'rotated');
		await vi.advanceTimersByTimeAsync(500);
		flushSync();

		expect(argsOf('update_global_var')).toHaveLength(1);
		// The localized reason names the credential store, and the text stays for retry.
		expect(get(toasts).map((toast) => toast.message).join(' ')).toContain(
			'Could not reach the credential store: keyring unavailable'
		);
		expect(valueInput(rowOf('token')!)?.value).toBe('rotated');
		expect(credentialOf('v2')).toBe('s3cret');
	});

	it('moves a secret typed into the revealed field into the database, not the older entry', async () => {
		desktopBuild([safe]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		revealButton()?.click();
		await vi.advanceTimersByTimeAsync(50);
		type(valueInput(rowOf('token')!)!, 'rotated');
		await vi.advanceTimersByTimeAsync(500);
		expect(credentialOf('v2')).toBe('rotated');

		typeToggle(rowOf('token')!).click();
		flushSync();
		buttonByText('Move the value into the database')?.click();
		await vi.advanceTimersByTimeAsync(50);
		flushSync();

		expect(argsOf('update_global_var').at(-1)).toEqual({
			id: 'v2',
			name: 'token',
			secure: false,
			value: 'rotated'
		});
		expect(valueInput(rowOf('token')!)?.value).toBe('rotated');
		expect(credentialOf('v2')).toBeUndefined();
	});

	it('says a safe variable has no value yet instead of masking an empty one', async () => {
		desktopBuild([]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		type(draftName(), 'token');
		buttonByLabel('Keep this value in the application database')?.click();
		flushSync();

		buttonByLabel('Add variable')?.click();
		await until(() => expect(argsOf('create_global_var')).toHaveLength(1));
		expect(argsOf('create_global_var')[0]).toEqual({ name: 'token', secure: true, value: '' });

		// The dots promise a hidden secret, which this row does not have.
		expect(rowOf('token')?.textContent).toContain('No value stored yet');
		expect(rowOf('token')?.textContent).not.toContain('••••••');
		expect(emptySecretButton()).toBeTruthy();
		expect(credentialOf('new-1')).toBeUndefined();
	});

	it('drops a queued write when the row is deleted', async () => {
		desktopBuild([plain]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		type(valueInput(rowOf('base')!)!, 'https://changed.example.com');
		buttonByLabel('base')?.click();
		flushSync();
		buttonByText('Delete')?.click();
		await vi.advanceTimersByTimeAsync(1000);
		flushSync();

		expect(argsOf('delete_global_var')).toEqual([{ id: 'v1' }]);
		expect(argsOf('update_global_var')).toHaveLength(0);
		expect(nameInputs()).toHaveLength(0);
	});

	it('copies the template of a row, not its value', async () => {
		desktopBuild([plain, safe]);
		const writeText = vi.fn(async () => {});
		// `copyText` only uses the clipboard in a secure context, which jsdom does not claim.
		Object.defineProperty(window, 'isSecureContext', { value: true, configurable: true });
		Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		buttonByLabel('Copy the template {{ base }}')?.click();
		await until(() => expect(writeText).toHaveBeenCalledWith('{{ base }}'));
		await until(() =>
			expect(get(toasts).map((toast) => toast.message)).toContain('Template copied')
		);
		// A safe row copies the same shape: the secret is never what goes to the clipboard.
		expect(writeText.mock.calls.flat()).not.toContain('s3cret');
	});

	it('names the load action when the list answers no reason', async () => {
		desktopBuild([plain]);
		listFailure = '';
		component = mount(VariablesPage, { target: document.body });
		flushSync();

		await until(() =>
			expect(get(toasts).map((toast) => toast.message)).toContain(
				'Could not load global variables'
			)
		);
		expect(document.querySelector('#var-draft-name')).toBeTruthy();
	});

	it('shows the database reason of a failed load as it came', async () => {
		desktopBuild([plain]);
		listFailure = 'no such table: global_vars';
		component = mount(VariablesPage, { target: document.body });
		flushSync();

		await until(() =>
			expect(get(toasts).map((toast) => toast.message)).toContain('no such table: global_vars')
		);
		expect(get(toasts).map((toast) => toast.message).join(' ')).not.toContain(
			'Could not load global variables'
		);
	});

	it('names the save action when a write answers no reason', async () => {
		desktopBuild([plain]);
		component = mount(VariablesPage, { target: document.body });
		flushSync();
		await loaded();

		vi.useFakeTimers();
		// The backend rejects with an empty text, which no code prefix can explain.
		(window as unknown as { __TAURI_INTERNALS__: { invoke: unknown } }).__TAURI_INTERNALS__ = {
			invoke: async (command: string) => {
				calls.push([command, {}]);
				if (command === 'update_global_var') failBackend('');
				return null;
			}
		};
		type(valueInput(rowOf('base')!)!, 'changed');
		await vi.advanceTimersByTimeAsync(500);
		flushSync();

		expect(get(toasts).map((toast) => toast.message)).toContain('Could not save the variable');
	});
});
