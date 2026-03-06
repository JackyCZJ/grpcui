import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, 'index.html'),
      },
      output: {
        manualChunks: {
          vendor: ['react', 'react-dom', 'zustand', '@tanstack/react-query'],
          editor: ['monaco-editor', '@monaco-editor/react'],
          ui: ['lucide-react', 'class-variance-authority', 'clsx', 'tailwind-merge'],
        },
        chunkFileNames: 'assets/js/[name]-[hash].js',
        entryFileNames: 'assets/js/[name]-[hash].js',
        assetFileNames: (assetInfo) => {
          const info = assetInfo.name?.split('.') || []
          const ext = info[info.length - 1]
          if (/\.(png|jpe?g|gif|svg|webp|ico)$/i.test(assetInfo.name || '')) {
            return 'assets/images/[name]-[hash][extname]'
          }
          if (/\.(woff2?|ttf|otf|eot)$/i.test(assetInfo.name || '')) {
            return 'assets/fonts/[name]-[hash][extname]'
          }
          return 'assets/[ext]/[name]-[hash][extname]'
        },
      },
    },
    target: 'es2020',
    minify: 'terser',
    terserOptions: {
      compress: {
        drop_console: true,
        drop_debugger: true,
      },
    },
    chunkSizeWarningLimit: 1000,
    sourcemap: true,
  },
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'zustand',
      '@tanstack/react-query',
      'lucide-react',
    ],
    exclude: ['@tauri-apps/api'],
  },
}))
