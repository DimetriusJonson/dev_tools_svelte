// Vitest setup: keeps browser storage available on every Node version.
//
// Node 25+ installs `localStorage` on `globalThis` as a lazy accessor that answers
// `undefined` unless the process got `--localstorage-file`. Vitest's jsdom
// environment re-exposes only the window keys that are absent from `globalThis`
// (`getWindowKeys` keeps an already-present key when it is not in its own list), so
// jsdom's storage never reaches the test global and every file reading
// `window.localStorage` dies with "Cannot read properties of undefined (reading
// 'getItem')". That is what broke the release preflight when `lts/*` moved the
// runner to Node 26. The shim prefers the environment's own jsdom window and falls
// back to an in-memory Storage, so the suite runs on Node 20, 24 and 26 alike.
export class MemoryStorage {
	private entries = new Map<string, string>();

	get length(): number {
		return this.entries.size;
	}

	key(index: number): string | null {
		return [...this.entries.keys()][index] ?? null;
	}

	getItem(key: string): string | null {
		return this.entries.has(key) ? (this.entries.get(key) as string) : null;
	}

	setItem(key: string, value: string): void {
		this.entries.set(key, String(value));
	}

	removeItem(key: string): void {
		this.entries.delete(key);
	}

	clear(): void {
		this.entries.clear();
	}
}

type EnvironmentWindow = {
	jsdom?: {
		window?: { localStorage?: Storage; sessionStorage?: Storage };
	};
};

const globals = globalThis as { localStorage?: Storage; sessionStorage?: Storage };
const jsdomWindow = (globalThis as EnvironmentWindow).jsdom?.window;

for (const key of ['localStorage', 'sessionStorage'] as const) {
	if (globals[key] === undefined) {
		Object.defineProperty(globalThis, key, {
			configurable: true,
			writable: true,
			value: jsdomWindow?.[key] ?? new MemoryStorage()
		});
	}
}
