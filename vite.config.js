import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  root: 'src',
  plugins: [vue()],
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
