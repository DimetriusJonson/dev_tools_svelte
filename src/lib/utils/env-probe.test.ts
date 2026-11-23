import { it } from 'vitest';

// Temporary CI diagnostic: the failure annotation carries the environment facts.
it('reports the test environment', () => {
	const w = globalThis as unknown as { window?: Window & typeof globalThis };
	throw new Error(
		`probe window=${typeof w.window} localStorage=${typeof w.window?.localStorage} ` +
			`storage=${typeof globalThis.localStorage} url=${w.window?.location?.href ?? '-'} ` +
			`navigator=${w.window?.navigator?.language ?? '-'} node=${process.version} ` +
			`CI=${process.env.CI ?? '-'} pool=${process.env.VITEST_POOL_ID ?? '-'} ` +
			`NODE_OPTIONS=${process.env.NODE_OPTIONS ?? '-'} pid=${process.pid}`
	);
});
