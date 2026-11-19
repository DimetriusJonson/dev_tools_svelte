<script lang="ts">
	import { tick } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { t } from '$lib/stores/i18n';
	import type { RestProject, RestRequest } from '$lib/services/rest';

	let {
		projects = [],
		requests = [],
		activeProjectId = null,
		activeId = null,
		onSelectProject,
		onCreateProject,
		onRenameProject,
		onDeleteProject,
		onCreateRequest,
		onImportCurl,
		onSelectRequest,
		onExecuteRequest,
		onCopyCurl,
		onRenameRequest,
		onDuplicateRequest,
		onDeleteRequest
	}: {
		projects?: RestProject[];
		requests?: RestRequest[];
		activeProjectId?: string | null;
		activeId?: string | null;
		onSelectProject: (id: string | null) => void;
		onCreateProject: (name: string) => void;
		onRenameProject: (id: string, name: string) => void;
		onDeleteProject: (id: string) => void;
		onCreateRequest: () => void;
		onImportCurl: () => void;
		onSelectRequest: (id: string) => void;
		onExecuteRequest: (id: string) => void;
		onCopyCurl: (id: string, shell: 'posix' | 'windows') => void;
		onRenameRequest: (id: string, name: string) => void;
		onDuplicateRequest: (id: string) => void;
		onDeleteRequest: (id: string) => void;
	} = $props();

	let projectMenu = $state<{ x: number; y: number } | null>(null);
	let requestMenu = $state<{ id: string; x: number; y: number } | null>(null);
	let creatingProject = $state(false);
	let newProjectName = $state('');
	let newProjectInput = $state<HTMLInputElement | null>(null);
	let renamingProject = $state(false);
	let projectDraft = $state('');
	let projectInput = $state<HTMLInputElement | null>(null);
	let renamingRequestId = $state<string | null>(null);
	let requestDraft = $state('');
	let requestInput = $state<HTMLInputElement | null>(null);

	const activeProject = $derived(projects.find((project) => project.id === activeProjectId) ?? null);
	const methodColors: Record<string, string> = {
		GET: 'text-emerald-500',
		POST: 'text-amber-500',
		PUT: 'text-sky-400',
		PATCH: 'text-violet-400',
		DELETE: 'text-red-500',
		HEAD: 'text-slate-400',
		OPTIONS: 'text-slate-400'
	};

	function methodClass(method: string): string {
		return methodColors[method.toUpperCase()] ?? 'text-[var(--text-muted)]';
	}

	function requestLabel(request: RestRequest): string {
		return request.name.trim() || request.url.trim() || $t('rest.untitled');
	}

	function selectProject(event: Event): void {
		const value = (event.currentTarget as HTMLSelectElement).value;
		onSelectProject(value || null);
	}

	function openProjectMenu(event: MouseEvent): void {
		event.stopPropagation();
		const target = event.currentTarget as HTMLElement;
		const rect = target.getBoundingClientRect();
		projectMenu = {
			x: Math.max(8, Math.min(rect.right - 196, window.innerWidth - 204)),
			y: Math.min(rect.bottom + 4, window.innerHeight - 140)
		};
	}

	function openRequestMenu(event: MouseEvent, id: string): void {
		event.stopPropagation();
		onSelectRequest(id);
		requestMenu = {
			id,
			x: Math.max(8, Math.min(event.clientX, window.innerWidth - 220)),
			y: Math.max(8, Math.min(event.clientY, window.innerHeight - 240))
		};
	}

	function closeMenus(): void {
		projectMenu = null;
		requestMenu = null;
	}

	function startCreateProject(): void {
		closeMenus();
		creatingProject = true;
		renamingProject = false;
		newProjectName = '';
		void tick().then(() => newProjectInput?.focus());
	}

	function startRenameProject(): void {
		if (!activeProject) return;
		closeMenus();
		creatingProject = false;
		renamingProject = true;
		projectDraft = activeProject.name;
		void tick().then(() => {
			projectInput?.focus();
			projectInput?.select();
		});
	}

	function commitCreateProject(): void {
		if (!creatingProject) return;
		const name = newProjectName.trim();
		creatingProject = false;
		newProjectName = '';
		if (name) onCreateProject(name);
	}

	function commitRenameProject(): void {
		if (!renamingProject || !activeProject) return;
		const name = projectDraft.trim();
		renamingProject = false;
		if (name && name !== activeProject.name) onRenameProject(activeProject.id, name);
	}

	function cancelProjectEditing(): void {
		creatingProject = false;
		renamingProject = false;
		newProjectName = '';
		projectDraft = '';
	}

	function startRenameRequest(id: string): void {
		const request = requests.find((item) => item.id === id);
		if (!request) return;
		closeMenus();
		renamingRequestId = id;
		requestDraft = request.name;
		void tick().then(() => {
			requestInput?.focus();
			requestInput?.select();
		});
	}

	function commitRenameRequest(): void {
		const id = renamingRequestId;
		if (!id) return;
		renamingRequestId = null;
		onRenameRequest(id, requestDraft.trim());
	}

	function onWindowKeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape') {
			closeMenus();
			cancelProjectEditing();
			renamingRequestId = null;
		}
	}

	function runMenuAction(action: () => void): void {
		closeMenus();
		action();
	}

	function withRequestMenu(action: (id: string) => void): void {
		const id = requestMenu?.id;
		if (!id) return;
		runMenuAction(() => action(id));
	}
</script>

<svelte:window onclick={closeMenus} onkeydown={onWindowKeydown} />

<aside
	class="flex h-full min-h-0 w-full flex-col gap-3 rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] p-3 shadow-[var(--shadow)]"
	aria-label={$t('rest.explorer')}
>
	<div class="flex shrink-0 items-center gap-1.5">
		{#if creatingProject}
			<input
				bind:this={newProjectInput}
				bind:value={newProjectName}
				onkeydown={(event) => {
					if (event.key === 'Enter') commitCreateProject();
					else if (event.key === 'Escape') cancelProjectEditing();
				}}
				onblur={commitCreateProject}
				placeholder={$t('rest.projectNamePlaceholder')}
				class="h-10 min-w-0 flex-1 rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
				aria-label={$t('rest.projectNamePlaceholder')}
			/>
		{:else if renamingProject}
			<input
				bind:this={projectInput}
				bind:value={projectDraft}
				onkeydown={(event) => {
					if (event.key === 'Enter') commitRenameProject();
					else if (event.key === 'Escape') cancelProjectEditing();
				}}
				onblur={commitRenameProject}
				placeholder={$t('rest.projectNamePlaceholder')}
				class="h-10 min-w-0 flex-1 rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
				aria-label={$t('rest.projectNamePlaceholder')}
			/>
		{:else}
			<div class="relative min-w-0 flex-1">
				<select
					value={activeProjectId ?? ''}
					onchange={selectProject}
					class="h-10 w-full appearance-none rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 pr-9 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
					aria-label={$t('rest.projects')}
				>
					<option value="">{$t('rest.noProject')}</option>
					{#each projects as project (project.id)}
						<option value={project.id}>{project.name}</option>
					{/each}
				</select>
				<Icon
					name="chevronDown"
					size={16}
					class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
				/>
			</div>
		{/if}

		<button
			type="button"
			onclick={openProjectMenu}
			class="flex h-10 w-9 shrink-0 items-center justify-center rounded-lg text-[var(--text-muted)] transition-all duration-150 hover:scale-105 hover:text-[var(--text)] active:scale-95"
			title={$t('rest.projectMenu')}
			aria-label={$t('rest.projectMenu')}
		>
			<Icon name="more" size={18} strokeWidth={3} />
		</button>
	</div>

	<div class="flex shrink-0 items-center gap-1.5">
		<button
			type="button"
			onclick={onCreateRequest}
			class="h-10 min-w-0 flex-1 rounded-full bg-[var(--accent)] px-4 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-[1.02] hover:bg-[var(--accent-hover)] active:scale-95"
		>
			{$t('rest.createRequest')}
		</button>
		<button
			type="button"
			onclick={onImportCurl}
			class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--secondary-bg)] text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
			title={$t('rest.importCurl')}
			aria-label={$t('rest.importCurl')}
		>
			<Icon name="clipboard" size={18} />
		</button>
	</div>

	<div class="min-h-0 flex-1 overflow-y-auto">
		{#if requests.length === 0}
			<p class="px-1 py-6 text-center text-xs font-semibold text-[var(--text-muted)]">
				{$t('rest.emptyRequests')}
			</p>
		{:else}
			<ul class="flex flex-col gap-0.5">
				{#each requests as request (request.id)}
					{@const active = activeId === request.id}
					<li class="relative">
						{#if renamingRequestId === request.id}
							<input
								bind:this={requestInput}
								bind:value={requestDraft}
								onkeydown={(event) => {
									if (event.key === 'Enter') commitRenameRequest();
									else if (event.key === 'Escape') renamingRequestId = null;
								}}
								onblur={commitRenameRequest}
								class="h-9 w-full rounded-md border border-[var(--accent)] bg-[var(--panel-bg-alt)] px-2 text-sm font-semibold text-[var(--text)] outline-none"
								aria-label={$t('rest.renameRequest')}
							/>
						{:else}
							<button
								type="button"
								onclick={() => onSelectRequest(request.id)}
								oncontextmenu={(event) => {
									event.preventDefault();
									openRequestMenu(event, request.id);
								}}
								class="flex h-9 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left transition-colors duration-150 {active
									? 'bg-[var(--row-active)] shadow-[inset_3px_0_0_var(--accent)]'
									: 'hover:bg-[var(--row-hover)]'}"
								title={requestLabel(request)}
							>
								<span class="w-11 shrink-0 text-[11px] font-black {methodClass(request.method)}">
									{request.method.toUpperCase()}
								</span>
								<span class="min-w-0 flex-1 truncate text-[13px] font-semibold text-[var(--text)]">
									{requestLabel(request)}
								</span>
							</button>
							{#if active}
								<button
									type="button"
									onclick={(event) => openRequestMenu(event, request.id)}
									class="absolute right-1 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md bg-[var(--panel-bg)] text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:text-[var(--text)]"
									title={$t('rest.requestMenu')}
									aria-label={$t('rest.requestMenu')}
								>
									<Icon name="more" size={16} strokeWidth={3} />
								</button>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</aside>

{#if projectMenu}
	<div
		class="fixed z-50 w-48 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] py-1 shadow-2xl"
		style="left: {projectMenu.x}px; top: {projectMenu.y}px;"
		role="menu"
	>
		<button
			type="button"
			onclick={() => runMenuAction(startCreateProject)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
			role="menuitem"
		>
			{$t('rest.createProject')}
		</button>
		<button
			type="button"
			disabled={!activeProject}
			onclick={() => runMenuAction(startRenameProject)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)] disabled:opacity-40"
			role="menuitem"
		>
			{$t('rest.renameProject')}
		</button>
		<button
			type="button"
			disabled={!activeProject}
			onclick={() => activeProject && runMenuAction(() => onDeleteProject(activeProject.id))}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-red-500 transition-colors hover:bg-red-500 hover:text-white disabled:opacity-40"
			role="menuitem"
		>
			{$t('rest.deleteProject')}
		</button>
	</div>
{/if}

{#if requestMenu}
	<div
		class="fixed z-50 w-52 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] py-1 shadow-2xl"
		style="left: {requestMenu.x}px; top: {requestMenu.y}px;"
		role="menu"
	>
		<button
			type="button"
			onclick={() => withRequestMenu(onExecuteRequest)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
			role="menuitem"
		>
			{$t('rest.executeRequest')}
		</button>
		<button
			type="button"
			onclick={() => withRequestMenu((id) => onCopyCurl(id, 'posix'))}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
			role="menuitem"
		>
			{$t('rest.copyCurlPosix')}
		</button>
		<button
			type="button"
			onclick={() => withRequestMenu((id) => onCopyCurl(id, 'windows'))}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
			role="menuitem"
		>
			{$t('rest.copyCurlWindows')}
		</button>
		<button
			type="button"
			onclick={() => withRequestMenu(startRenameRequest)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
			role="menuitem"
		>
			{$t('rest.renameRequest')}
		</button>
		<button
			type="button"
			onclick={() => withRequestMenu(onDuplicateRequest)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
			role="menuitem"
		>
			{$t('rest.duplicate')}
		</button>
		<button
			type="button"
			onclick={() => withRequestMenu(onDeleteRequest)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-red-500 transition-colors hover:bg-red-500 hover:text-white"
			role="menuitem"
		>
			{$t('rest.delete')}
		</button>
	</div>
{/if}
