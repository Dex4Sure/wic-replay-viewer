import tailwindcss from '@tailwindcss/vite';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  clearScreen: false,
  build: { sourcemap: 'hidden' },
  optimizeDeps: {
    entries: ['index.html'],
  },
  test: {
    include: ['frontend/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      reportsDirectory: 'coverage/frontend',
      include: ['frontend/**/*.{ts,vue}'],
      exclude: [
        'frontend/env.d.ts',
        'frontend/main.ts',
        'frontend/test/**',
        'frontend/**/*.test.ts',
      ],
      thresholds: {
        lines: 85,
        statements: 85,
        functions: 85,
        branches: 75,
      },
    },
  },
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: {
      followSymlinks: false,
      ignored: [
        '**/local/**',
        '**/research/**',
        '**/target/**',
        '**/.flatpak-build/**',
        '**/.flatpak-builder/**',
        '**/.flatpak-repo/**',
      ],
    },
  },
});
