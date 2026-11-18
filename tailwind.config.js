/** @type {import('tailwindcss').Config} */
export default {
	darkMode: 'class',
	content: ['./src/**/*.{html,js,svelte,ts}'],
	theme: {
		extend: {
			fontFamily: {
				sans: ['"Segoe UI"', 'system-ui', '-apple-system', 'BlinkMacSystemFont', 'sans-serif'],
				mono: ['"Cascadia Code"', '"Fira Code"', 'Consolas', '"Courier New"', 'monospace']
			}
		}
	},
	plugins: []
};
