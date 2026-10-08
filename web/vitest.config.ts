import { defineConfig } from 'vitest/config';
export default defineConfig({
  test: { environment: 'node', include: ['test/**/*.test.ts'] },
  oxc: { jsx: { runtime: 'automatic' } }, // tsconfig has jsx: preserve (Next compiles it); vitest must transform a .tsx a test imports
  resolve: { alias: { '@': import.meta.dirname } },
});
