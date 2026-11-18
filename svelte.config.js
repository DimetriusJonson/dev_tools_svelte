import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		version: { name: pkg.version },
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: 'index.html',
			preload: false
		})
	}
};

export default config;
