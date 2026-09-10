import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath, URL } from 'node:url';

export default defineConfig({
  root: 'src',
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  css: {
    preprocessorOptions: {
      scss: {
        // Plan Step 1.1: make @/styles/_tokens.scss resolvable inside
        // <style scoped lang="scss"> blocks across all components.
        // Without this, `@use '@/styles/_tokens' as *;` fails with
        // "Can't find stylesheet to import" once a component is actually
        // pulled into the build graph.
        loadPaths: [fileURLToPath(new URL('./src', import.meta.url))]
      }
    }
  },
  server: {
    host: '127.0.0.1',
    port: 41234,
    strictPort: true
  },
  build: {
    outDir: '../dist',
    emptyOutDir: true
  }
});
