<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';

	interface SearchHit {
		path: string;
		title: string;
		summary: string;
		diataxis: string;
		tags: string;
		snippet: string;
		score: number;
	}

	interface SearchResponse {
		hits: SearchHit[];
		limit: number;
		offset: number;
	}

	let isOpen = $state(false);
	let query = $state('');
	let results = $state<SearchHit[]>([]);
	let selectedIndex = $state(0);
	let loading = $state(false);
	let inputRef = $state<HTMLInputElement | null>(null);
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;

	onMount(() => {
		function handleKeydown(e: KeyboardEvent) {
			if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
				e.preventDefault();
				isOpen = !isOpen;
				if (isOpen) {
					setTimeout(() => inputRef?.focus(), 0);
				}
			}

			if (e.key === 'Escape' && isOpen) {
				close();
			}
		}

		document.addEventListener('keydown', handleKeydown);
		return () => document.removeEventListener('keydown', handleKeydown);
	});

	async function search() {
		const q = query.trim();
		if (!q) {
			results = [];
			return;
		}

		loading = true;
		try {
			const res = await fetch(`/docs/search?q=${encodeURIComponent(q)}&limit=10`);
			if (res.ok) {
				const data: SearchResponse = await res.json();
				results = data.hits;
				selectedIndex = 0;
			} else {
				results = [];
			}
		} catch (e) {
			console.error('Search failed:', e);
			results = [];
		} finally {
			loading = false;
		}
	}

	function debouncedSearch() {
		if (debounceTimer) clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => {
			search();
		}, 200);
	}

	function handleInputKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, results.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter' && results[selectedIndex]) {
			e.preventDefault();
			navigateTo(results[selectedIndex].path);
		}
	}

	function navigateTo(path: string) {
		close();
		goto(path);
	}

	function close() {
		isOpen = false;
		query = '';
		results = [];
		selectedIndex = 0;
	}

	$effect(() => {
		if (query) {
			debouncedSearch();
		} else {
			results = [];
		}
	});

	const diataxisLabels: Record<string, string> = {
		tutorial: i18n._('docs.search.category.tutorial'),
		'how-to': i18n._('docs.search.category.howTo'),
		reference: i18n._('docs.search.category.reference'),
		explanation: i18n._('docs.search.category.explanation'),
	};
</script>

<button class="search-trigger" onclick={() => (isOpen = true)}>
	<span class="search-icon">⌕</span>
	<span class="search-text">{i18n._('docs.search.placeholder')}</span>
	<kbd class="search-kbd">⌘K</kbd>
</button>

{#if isOpen}
	<div class="search-overlay" role="dialog" aria-modal="true" aria-label="Search documentation">
		<button class="search-backdrop" onclick={close} aria-label={i18n._('docs.search.close')}></button>

		<div class="search-modal">
			<div class="search-header">
				<span class="search-icon-large">⌕</span>
				<input
					bind:this={inputRef}
					bind:value={query}
					onkeydown={handleInputKeydown}
					type="text"
					class="search-input"
					placeholder={i18n._('docs.search.placeholderFull')}
					aria-label="Search query"
				/>
				{#if loading}
					<span class="search-loading">...</span>
				{/if}
				<button class="search-close" onclick={close}>
					<kbd>Esc</kbd>
				</button>
			</div>

			{#if results.length > 0}
				<ul class="search-results" role="listbox">
					{#each results as result, i}
						<li role="option" aria-selected={i === selectedIndex}>
							<button
								class="search-result"
								class:selected={i === selectedIndex}
								onclick={() => navigateTo(result.path)}
								onmouseenter={() => (selectedIndex = i)}
							>
								<div class="result-header">
									<span class="result-title">{result.title}</span>
									<span class="result-badge">{diataxisLabels[result.diataxis] ?? result.diataxis}</span>
								</div>
								<span class="result-excerpt">{@html result.snippet}</span>
							</button>
						</li>
					{/each}
				</ul>
			{:else if query.trim() && !loading}
				<div class="search-empty">
					<p>{i18n._('docs.search.noResults')} "{query}"</p>
				</div>
			{:else if !query.trim()}
				<div class="search-empty">
					<p>{i18n._('docs.search.hint')}</p>
				</div>
			{/if}

			<div class="search-footer">
				<span class="search-hint">
					<kbd>↑</kbd><kbd>↓</kbd> {i18n._('docs.search.key.navigate')}
					<kbd>↵</kbd> {i18n._('docs.search.key.select')}
					<kbd>Esc</kbd> {i18n._('docs.search.key.close')}
				</span>
			</div>
		</div>
	</div>
{/if}

<style>
	.search-trigger {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		background: var(--color-bg-subtle);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		cursor: pointer;
		transition: all 0.15s ease;
		width: 100%;
	}

	.search-trigger:hover {
		border-color: var(--color-accent);
		color: var(--color-fg);
	}

	.search-icon {
		font-size: var(--text-base);
	}

	.search-text {
		flex: 1;
		text-align: left;
	}

	.search-kbd {
		font-size: var(--text-xs);
		padding: 2px 6px;
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
	}

	.search-overlay {
		position: fixed;
		inset: 0;
		z-index: 1000;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 10vh;
	}

	.search-backdrop {
		position: absolute;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		border: none;
		cursor: pointer;
	}

	.search-modal {
		position: relative;
		width: 90%;
		max-width: 600px;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-lg);
		overflow: hidden;
	}

	.search-header {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-4);
		border-bottom: 1px solid var(--color-border);
	}

	.search-icon-large {
		font-size: var(--text-xl);
		color: var(--color-fg-muted);
	}

	.search-input {
		flex: 1;
		background: transparent;
		border: none;
		font-family: var(--font-mono);
		font-size: var(--text-base);
		color: var(--color-fg);
		outline: none;
	}

	.search-input::placeholder {
		color: var(--color-fg-subtle);
	}

	.search-loading {
		color: var(--color-fg-muted);
		animation: pulse 1s infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}

	.search-close {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-fg-muted);
	}

	.search-close kbd {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		padding: 2px 6px;
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
	}

	.search-results {
		list-style: none;
		padding: 0;
		margin: 0;
		max-height: 400px;
		overflow-y: auto;
	}

	.search-result {
		display: block;
		width: 100%;
		padding: var(--space-3) var(--space-4);
		background: transparent;
		border: none;
		text-align: left;
		cursor: pointer;
		font-family: var(--font-mono);
		transition: background 0.1s ease;
	}

	.search-result:hover,
	.search-result.selected {
		background: var(--color-bg-muted);
	}

	.result-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-1);
	}

	.result-title {
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
	}

	.result-badge {
		font-size: var(--text-xs);
		padding: 1px 6px;
		background: var(--color-accent-soft);
		color: var(--color-accent);
		border-radius: var(--radius-sm);
	}

	.result-excerpt {
		display: block;
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		line-height: 1.5;
	}

	.result-excerpt :global(mark) {
		background: var(--color-warning-soft);
		color: var(--color-warning);
		padding: 0 2px;
		border-radius: 2px;
	}

	.search-empty {
		padding: var(--space-8);
		text-align: center;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.search-footer {
		padding: var(--space-3) var(--space-4);
		border-top: 1px solid var(--color-border);
		background: var(--color-bg-muted);
	}

	.search-hint {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--color-fg-subtle);
	}

	.search-hint kbd {
		display: inline-block;
		padding: 2px 4px;
		margin: 0 2px;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-sm);
	}
</style>
