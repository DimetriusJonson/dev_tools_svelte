/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		port: 1420,
		strictPort: true,
		host: 'localhost'
	},
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		environment: 'jsdom',
		server: {
			deps: {
				inline: [/svelte/]
			}
		}
	},
	resolve: {
		conditions: ['browser']
	}
});
