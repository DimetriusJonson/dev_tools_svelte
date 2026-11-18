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
		compare: ['M8 3v18', 'M16 3v18', 'M3 8h5', 'M16 16h5'],
		server: [
			'M4 4h16v6H4Z',
			'M4 14h16v6H4Z',
			'M8 7h.01',
			'M8 17h.01'
		],
		sparkles: [
			'M12 3l1.5 4.5L18 9l-4.5 1.5L12 15l-1.5-4.5L6 9l4.5-1.5Z',
			'M19 15l.7 2.3L22 18l-2.3.7L19 21l-.7-2.3L16 18l2.3-.7Z'
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
