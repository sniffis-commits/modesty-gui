import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
  root: './src',
  publicDir: '../dist',
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: path.resolve(__dirname, 'src-tauri/src/dist'),
    emptyOutDir: true,
  },
  plugins: [],
});