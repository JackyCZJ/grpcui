import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: [path.resolve(__dirname, './tests/setup.ts')],
    include: [
      path.resolve(__dirname, './tests/**/*.{test,spec}.{js,mjs,cjs,ts,mts,cts,jsx,tsx}'),
    ],
    exclude: [
      path.resolve(__dirname, './tests/e2e/**'),
      '**/node_modules/**',
    ],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'tests/',
        '**/*.d.ts',
        '**/*.config.*',
        '**/src-tauri/**',
      ],
    },
    testTimeout: 30000,
    hookTimeout: 15000,
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
