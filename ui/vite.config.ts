import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  base: './',
  // 给 jsdom 补上浏览器有、jsdom 没有的全局对象（见 src/testSetup.ts）。
  test: { setupFiles: ['./src/testSetup.ts'] },
});
