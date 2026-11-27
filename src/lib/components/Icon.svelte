<script module lang="ts">
	export const ICON_PATHS = {
		copy: [
			'M8 8V5a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-3',
			'M5 8h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2Z'
		],
		download: ['M12 3v12', 'm7 10 5 5 5-5', 'M5 21h14'],
		share: [
			'M4 12v7a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7',
			'M16 6l-4-4-4 4',
			'M12 2v13'
		],
		chevronDown: ['m6 9 6 6 6-6'],
		sun: [
			'M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8Z',
			'M12 2v2',
			'M12 20v2',
			'm4.93 4.93 1.41 1.41',
			'm17.66 17.66 1.41 1.41',
			'M2 12h2',
			'M20 12h2',
			'm6.34 17.66-1.41 1.41',
			'm19.07 4.93-1.41 1.41'
		],
		moon: ['M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z'],
		monitor: [
			'M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z',
			'M8 21h8',
			'M12 17v4'
		],
		arrowRight: ['M4 12h15', 'm13 6 6 6-6 6'],
		check: ['m5 12 5 5 9-11'],
		swap: ['M7 16V4', 'm3 8 4-4 4 4', 'M17 8v12', 'm13 16 4 4 4-4'],
		file: [
			'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z',
			'M14 2v6h6'
		],
		server: [
			'M4 4h16v6H4Z',
			'M4 14h16v6H4Z',
			'M8 7h.01',
			'M8 17h.01'
		],
		sparkles: [
			'M12 3l1.5 4.5L18 9l-4.5 1.5L12 15l-1.5-4.5L6 9l4.5-1.5Z',
			'M19 15l.7 2.3L22 18l-2.3.7L19 21l-.7-2.3L16 18l2.3-.7Z'
		],
		globe: [
			'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z',
			'M2 12h20',
			'M12 2a15.3 15.3 0 0 1 0 20 15.3 15.3 0 0 1 0-20Z'
		],
		search: ['M21 21l-4.35-4.35', 'M17 11a6 6 0 1 1-12 0 6 6 0 0 1 12 0Z'],
		more: ['M5 12h.01', 'M12 12h.01', 'M19 12h.01'],
		plus: ['M12 5v14', 'M5 12h14'],
		trash: [
			'M3 6h18',
			'M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2',
			'M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6',
			'M10 11v6',
			'M14 11v6'
		],
		clipboard: [
			'M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2',
			'M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2Z'
		],
		braces: [
			'M8 3H7a2 2 0 0 0-2 2v4a2 2 0 0 1-2 2 2 2 0 0 1 2 2v4a2 2 0 0 0 2 2h1',
			'M16 3h1a2 2 0 0 1 2 2v4a2 2 0 0 0 2 2 2 2 0 0 0-2 2v4a2 2 0 0 1-2 2h-1'
		],
		alignLeft: ['M4 6h16', 'M4 12h10', 'M4 18h13'],
		eraser: [
			'M7 21h10',
			'm5 15 6-6',
			'M14 6l4 4',
			'M3 15l6 6h3l9-9a2 2 0 0 0 0-3l-3-3a2 2 0 0 0-3 0Z'
		],
		eye: [
			'M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6Z',
			'M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z'
		],
		close: ['M6 6l12 12', 'M18 6L6 18'],
		refresh: ['M21 12a9 9 0 1 1-2.64-6.36', 'M21 3v6h-6'],
		help: [
			'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z',
			'M9.1 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3',
			'M12 17h.01'
		],
		warning: [
			'M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z',
			'M12 9v4',
			'M12 17h.01'
		],
		update: ['M12 3v10', 'm7 9 5 5 5-5', 'M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4'],
		lock: [
			'M15 11V7a4 4 0 0 0-8 0v4',
			'M3 13a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'
		],
		unlock: [
			'M7 11V7a5 5 0 0 1 10 0',
			'M3 13a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z'
		],
		variable: [
			'M8 4H5a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h3',
			'M16 4h3a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-3',
			'M12 9v6'
		]
	} satisfies Record<string, string[]>;

	export type IconName = keyof typeof ICON_PATHS;
</script>

<script lang="ts">
	let {
		name,
		size = 20,
		class: className = '',
		strokeWidth = 2
	}: { name: IconName; size?: number; class?: string; strokeWidth?: number } = $props();
</script>

<svg
	xmlns="http://www.w3.org/2000/svg"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width={strokeWidth}
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	class={className}
>
	{#each ICON_PATHS[name] as d, index (index)}
		<path {d} />
	{/each}
</svg>
