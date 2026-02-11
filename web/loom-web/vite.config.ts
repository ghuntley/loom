/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		proxy: {
			'/v1': {
				target: 'http://127.0.0.1:8080',
				changeOrigin: true,
			},
			'/proxy': {
				target: 'http://127.0.0.1:8080',
				changeOrigin: true,
			},
			'/health': {
				target: 'http://127.0.0.1:8080',
				changeOrigin: true,
			},
			'/api': {
				target: 'http://127.0.0.1:8080',
				changeOrigin: true,
			},
			'/auth': {
				target: 'http://127.0.0.1:8080',
				changeOrigin: true,
			},
		},
	},
	test: {
		include: ['src/**/*.test.ts'],
		globals: true,
		environment: 'jsdom',
	},
});
