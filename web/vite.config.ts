import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

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
	},
});
