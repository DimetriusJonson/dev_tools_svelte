<script lang="ts">
	import { tick } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { t } from '$lib/stores/i18n';
	import type { WsdlFileSummary, WsdlProject } from '$lib/services/wsdl';
	import { describeWarningText } from '$lib/utils/wsdl';

	let {
		projects = [],
		files = [],
		activeProjectId = null,
		activeOperationId = null,
		loading = false,
		importing = false,
		onSelectProject,
		onCreateProject,
		onRenameProject,
		onDeleteProject,
		onImport,
		onCancelImport,
		onSelectOperation,
		onExecuteFile,
		onDeleteFile
	}: {
		projects?: WsdlProject[];
		files?: WsdlFileSummary[];
		activeProjectId?: string | null;
		activeOperationId?: string | null;
		/** The tree is being read from the backend. */
		loading?: boolean;
		/** A WSDL is being fetched and parsed; the import button turns into Cancel. */
		importing?: boolean;
		onSelectProject: (id: string | null) => void;
		onCreateProject: (name: string) => void;
		onRenameProject: (id: string, name: string) => void;
		onDeleteProject: (id: string) => void;
		onImport: () => void;
		onCancelImport: () => void;
		onSelectOperation: (id: string) => void;
		onExecuteFile: (fileId: string) => void;
		onDeleteFile: (fileId: string) => void;
	} = $props();

	let projectMenu = $state<{ x: number; y: number } | null>(null);
	let fileMenu = $state<{ id: string; x: number; y: number } | null>(null);
	let helpOpen = $state<{ x: number; y: number } | null>(null);
	let confirmingFileId = $state<string | null>(null);
	let creatingProject = $state(false);
	let newProjectName = $state('');
	let newProjectInput = $state<HTMLInputElement | null>(null);
	let renamingProject = $state(false);
	let projectDraft = $state('');
	let projectInput = $state<HTMLInputElement | null>(null);

	/** Manual expansion wins over the auto-open of the selected operation's branch. */
	let expanded = $state<Record<string, boolean>>({});

	const activeProject = $derived(projects.find((project) => project.id === activeProjectId) ?? null);

	// The branch that holds the selected operation opens itself, so a restored
	// selection is visible without the user hunting for it in a collapsed tree.
	const activePath = $derived.by(() => {
		if (!activeOperationId) return null;
		for (const file of files) {
			for (const service of file.services) {
				if (service.operations.some((operation) => operation.id === activeOperationId)) {
					return { fileId: file.id, serviceId: service.id };
				}
			}
		}
		return null;
	});
	const activeFileId = $derived(activePath?.fileId ?? null);

	function isOpen(id: string): boolean {
		if (id in expanded) return Boolean(expanded[id]);
		return activePath?.fileId === id || activePath?.serviceId === id;
	}

	function toggle(id: string): void {
		expanded[id] = !isOpen(id);
	}

	function selectOperation(fileId: string, serviceId: string, operationId: string): void {
		expanded[fileId] = true;
		expanded[serviceId] = true;
		onSelectOperation(operationId);
	}

	function warningText(file: WsdlFileSummary): string {
		return file.warnings.map((warning) => {
			const described = describeWarningText(warning);
			return described.detail
				? $t(described.key, { detail: described.detail })
				: $t(described.key);
		}).join('\n');
	}

	/**
	 * Chevron rows announce "name · localized action". Building the text in the script
	 * keeps the attribute a single expression: `${name} · $t(key)}` inside a template
	 * literal renders `$t(key)}` as literal text instead of the translation.
	 */
	function rowLabel(name: string, actionKey: string): string {
		return `${name} · ${$t(actionKey)}`;
	}

	function selectProject(event: Event): void {
		const value = (event.currentTarget as HTMLSelectElement).value;
		onSelectProject(value || null);
	}

	function openProjectMenu(event: MouseEvent): void {
		event.stopPropagation();
		if (projectMenu) {
			projectMenu = null;
			return;
		}
		closeMenus();
		const target = event.currentTarget as HTMLElement;
		const rect = target.getBoundingClientRect();
		projectMenu = {
			x: Math.max(8, Math.min(rect.right - 196, window.innerWidth - 204)),
			y: Math.min(rect.bottom + 4, window.innerHeight - 140)
		};
	}

	function openFileMenu(event: MouseEvent, id: string): void {
		event.preventDefault();
		event.stopPropagation();
		if (fileMenu?.id === id) {
			closeMenus();
			return;
		}
		closeMenus();
		fileMenu = {
			id,
			x: Math.max(8, Math.min(event.clientX, window.innerWidth - 220)),
			y: Math.max(8, Math.min(event.clientY, window.innerHeight - 150))
		};
	}

	function openHelp(event: MouseEvent): void {
		event.stopPropagation();
		if (helpOpen) {
			helpOpen = null;
			return;
		}
		closeMenus();
		const target = event.currentTarget as HTMLElement;
		const rect = target.getBoundingClientRect();
		helpOpen = {
			x: Math.max(8, Math.min(rect.left, window.innerWidth - 300)),
			y: Math.min(rect.bottom + 6, window.innerHeight - 190)
		};
	}

	function closeMenus(): void {
		projectMenu = null;
		fileMenu = null;
		helpOpen = null;
		confirmingFileId = null;
	}

	/**
	 * A click outside closes every floating panel. A click inside one of them
	 * belongs to its own buttons: closing here on the bubbled event used to wipe
	 * `confirmingFileId` in the same tick the delete item set it, so the
	 * confirmation never appeared and the file stayed.
	 */
	function onWindowClick(event: MouseEvent): void {
		const target = event.target;
		if (target instanceof Element && target.closest('[role="menu"], [role="dialog"]')) return;
		closeMenus();
	}

	function onWindowKeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape') {
			closeMenus();
			cancelProjectEditing();
		}
	}

	function runMenuAction(action: () => void): void {
		closeMenus();
		action();
	}

	function withFileMenu(action: (id: string) => void): void {
		const id = fileMenu?.id;
		if (!id) return;
		closeMenus();
		action(id);
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

	const rowButton =
		'flex h-8 min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 text-left transition-colors duration-150';
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} />

<aside
	class="flex h-full min-h-0 w-full flex-col gap-3 rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] p-3 shadow-[var(--shadow)]"
	aria-label={$t('wsdl.explorer')}
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
				placeholder={$t('wsdl.projectNamePlaceholder')}
				class="h-10 min-w-0 flex-1 rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
				aria-label={$t('wsdl.projectNamePlaceholder')}
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
				placeholder={$t('wsdl.projectNamePlaceholder')}
				class="h-10 min-w-0 flex-1 rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 text-sm font-semibold text-[var(--text)] outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
				aria-label={$t('wsdl.projectNamePlaceholder')}
			/>
		{:else}
			<div class="relative min-w-0 flex-1">
				<select
					value={activeProjectId ?? ''}
					onchange={selectProject}
					class="h-10 w-full appearance-none rounded-lg border border-[var(--border-strong)] bg-[var(--panel-bg)] px-3 pr-9 text-sm font-semibold text-[var(--text)] outline-none transition-colors duration-150 hover:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
					aria-label={$t('wsdl.projects')}
				>
					<option value="">{$t('wsdl.noProject')}</option>
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
			title={$t('wsdl.projectMenu')}
			aria-label={$t('wsdl.projectMenu')}
		>
			<Icon name="more" size={18} strokeWidth={3} />
		</button>
	</div>

	<div class="flex shrink-0 items-center gap-1.5">
		<button
			type="button"
			onclick={importing ? onCancelImport : onImport}
			class="flex h-10 min-w-0 flex-1 items-center justify-center gap-2 rounded-full bg-[var(--accent)] px-4 text-sm font-bold text-[var(--accent-text)] transition-all duration-150 hover:scale-[1.02] hover:bg-[var(--accent-hover)] active:scale-95"
			title={importing ? $t('wsdl.cancelImport') : $t('wsdl.import')}
			aria-label={importing ? $t('wsdl.cancelImport') : $t('wsdl.import')}
		>
			{#if importing}
				<Icon name="close" size={16} />
				<span class="min-w-0 truncate">{$t('wsdl.cancelImport')}</span>
			{:else}
				<Icon name="clipboard" size={16} />
				<span class="min-w-0 truncate">{$t('wsdl.import')}</span>
			{/if}
		</button>
		<button
			type="button"
			onclick={openHelp}
			class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--secondary-bg)] text-[var(--secondary-text)] transition-all duration-150 hover:scale-105 active:scale-95"
			title={$t('wsdl.importHelp')}
			aria-label={$t('wsdl.importHelp')}
			aria-expanded={helpOpen !== null}
		>
			<Icon name="help" size={18} />
		</button>
	</div>

	<div class="min-h-0 flex-1 overflow-y-auto">
		{#if loading}
			<p class="px-1 py-6 text-center text-xs font-semibold text-[var(--text-muted)]">
				{$t('wsdl.loading')}
			</p>
		{:else if files.length === 0}
			<div class="flex flex-col items-center gap-1 px-1 py-6 text-center">
				<p class="text-xs font-semibold text-[var(--text-muted)]">{$t('wsdl.noFiles')}</p>
				<p class="text-[11px] leading-snug text-[var(--text-muted)]">{$t('wsdl.emptyHint')}</p>
			</div>
		{:else}
			<ul class="flex flex-col gap-0.5">
				{#each files as file (file.id)}
					{@const open = isOpen(file.id)}
					{@const activeFile = file.id === activeFileId}
					<li>
						<div class="flex items-center gap-1">
							<button
								type="button"
								onclick={() => toggle(file.id)}
								oncontextmenu={(event) => openFileMenu(event, file.id)}
								class="{rowButton} {activeFile
									? 'bg-[var(--row-active)] shadow-[inset_2px_0_0_var(--accent)]'
									: 'hover:bg-[var(--row-hover)]'}"
								aria-expanded={open}
								aria-label={rowLabel(
									file.name,
									open ? 'wsdl.collapse' : 'wsdl.expand'
								)}
								title={file.source_url}
							>
								<Icon
									name="chevronDown"
									size={13}
									class="shrink-0 text-[var(--text-muted)] transition-transform duration-150 {open ? '' : '-rotate-90'}"
								/>
								<Icon name="file" size={14} class="shrink-0 text-[var(--text-muted)]" />
								<span class="min-w-0 flex-1 truncate text-[13px] font-bold text-[var(--text)]">
									{file.name}
								</span>
								{#if file.warnings.length > 0}
									<span title={warningText(file)} class="shrink-0 text-[var(--accent)]">
										<Icon name="warning" size={14} />
									</span>
								{/if}
							</button>
							{#if activeFile}
								<button
									type="button"
									onclick={(event) => openFileMenu(event, file.id)}
									class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-[var(--text-muted)] transition-all duration-150 hover:scale-110 hover:text-[var(--text)]"
									title={$t('wsdl.fileMenu')}
									aria-label={$t('wsdl.fileMenu')}
								>
									<Icon name="more" size={16} strokeWidth={3} />
								</button>
							{/if}
						</div>

						{#if open}
							<ul class="ml-2 flex flex-col gap-0.5 border-l border-[var(--border)] pl-2">
								{#each file.services as service (service.id)}
									{@const serviceOpen = isOpen(service.id)}
									<li>
										<button
											type="button"
											onclick={() => toggle(service.id)}
											class="{rowButton} hover:bg-[var(--row-hover)]"
											aria-expanded={serviceOpen}
											aria-label={rowLabel(
												service.name,
												serviceOpen ? 'wsdl.collapseOperations' : 'wsdl.expandOperations'
											)}
											title={service.name}
										>
											<Icon
												name="chevronDown"
												size={12}
												class="shrink-0 text-[var(--text-muted)] transition-transform duration-150 {serviceOpen ? '' : '-rotate-90'}"
											/>
											<Icon name="server" size={13} class="shrink-0 text-[var(--text-muted)]" />
											<span class="min-w-0 flex-1 truncate text-[12px] font-semibold text-[var(--text)]">
												{service.name}
											</span>
										</button>
										{#if serviceOpen}
											<ul class="ml-2 flex flex-col gap-0.5 border-l border-[var(--border)] pl-2">
												{#each service.operations as operation (operation.id)}
													{@const activeOperation = operation.id === activeOperationId}
													<li>
														<button
															type="button"
															onclick={() => selectOperation(file.id, service.id, operation.id)}
															class="{rowButton} {activeOperation
																? 'bg-[var(--row-active)] shadow-[inset_2px_0_0_var(--accent)]'
																: 'hover:bg-[var(--row-hover)]'}"
															title={operation.name}
														>
															<Icon
																name="arrowRight"
																size={12}
																class="shrink-0 text-[var(--text-muted)]"
															/>
															<span
																class="min-w-0 flex-1 truncate text-[12px] {activeOperation
																	? 'font-bold text-[var(--text)]'
																	: 'font-medium text-[var(--text)]'}"
															>
																{operation.name}
															</span>
														</button>
													</li>
												{/each}
											</ul>
										{/if}
									</li>
								{/each}
							</ul>
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
			{$t('wsdl.createProject')}
		</button>
		<button
			type="button"
			disabled={!activeProject}
			onclick={() => runMenuAction(startRenameProject)}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)] disabled:opacity-40"
			role="menuitem"
		>
			{$t('wsdl.renameProject')}
		</button>
		<button
			type="button"
			disabled={!activeProject}
			onclick={() => activeProject && runMenuAction(() => onDeleteProject(activeProject.id))}
			class="block w-full px-3 py-2 text-left text-sm font-semibold text-red-500 transition-colors hover:bg-red-500 hover:text-white disabled:opacity-40"
			role="menuitem"
		>
			{$t('wsdl.deleteProject')}
		</button>
	</div>
{/if}

{#if fileMenu}
	{@const menu = fileMenu}
	{@const target = files.find((file) => file.id === menu.id)}
	<div
		class="fixed z-50 w-52 overflow-hidden rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] py-1 shadow-2xl"
		style="left: {menu.x}px; top: {menu.y}px;"
		role="menu"
	>
		{#if confirmingFileId === menu.id}
			<p class="px-3 py-2 text-[12px] font-semibold leading-snug text-[var(--text)]">
				{$t('wsdl.deleteConfirm', { name: target?.name ?? '' })}
			</p>
			<button
				type="button"
				onclick={closeMenus}
				class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--row-hover)]"
				role="menuitem"
			>
				{$t('common.cancel')}
			</button>
			<button
				type="button"
				onclick={() => withFileMenu(onDeleteFile)}
				class="block w-full px-3 py-2 text-left text-sm font-semibold text-red-500 transition-colors hover:bg-red-500 hover:text-white"
				role="menuitem"
			>
				{$t('wsdl.deleteFile')}
			</button>
		{:else}
			<button
				type="button"
				onclick={() => withFileMenu(onExecuteFile)}
				class="block w-full px-3 py-2 text-left text-sm font-semibold text-[var(--text)] transition-colors hover:bg-[var(--accent)] hover:text-[var(--accent-text)]"
				role="menuitem"
			>
				{$t('wsdl.executeFile')}
			</button>
			<button
				type="button"
				onclick={() => { confirmingFileId = menu.id; }}
				class="block w-full px-3 py-2 text-left text-sm font-semibold text-red-500 transition-colors hover:bg-red-500 hover:text-white"
				role="menuitem"
			>
				{$t('wsdl.deleteFile')}
			</button>
		{/if}
	</div>
{/if}

{#if helpOpen}
	<div
		role="dialog"
		aria-label={$t('wsdl.importHelp')}
		class="fixed z-50 w-72 rounded-lg border border-[var(--border)] bg-[var(--panel-bg)] p-3 shadow-2xl"
		style="left: {helpOpen.x}px; top: {helpOpen.y}px;"
	>
		<div class="flex items-start gap-2">
			<Icon name="help" size={16} class="mt-0.5 shrink-0 text-[var(--accent)]" />
			<p class="text-[12px] leading-relaxed text-[var(--text)]">{$t('wsdl.importHelpText')}</p>
		</div>
	</div>
{/if}
