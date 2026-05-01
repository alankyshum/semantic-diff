import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

declare const process: { env: Record<string, string | undefined> };

const rustPort = process.env.SEMANTIC_DIFF_PORT ?? '8765';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		proxy: {
			'/api': `http://localhost:${rustPort}`,
		},
	},
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		environment: 'jsdom',
		setupFiles: ['./src/test-setup.ts', '@testing-library/svelte/vitest'],
	},
	resolve: {
		// vitest needs the browser entry of `svelte` so testing-library can
		// mount components in jsdom. Without this it picks the SSR build and
		// `mount()` throws lifecycle_function_unavailable.
		conditions: process.env.VITEST ? ['browser'] : [],
	},
});
