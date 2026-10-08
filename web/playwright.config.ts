import { defineConfig, devices } from '@playwright/test';
const live = !!process.env.TERMINAL_E2E_LIVE;
export default defineConfig({
  testDir: './e2e',
  testIgnore: live ? ['smoke.spec.ts'] : ['live.spec.ts'],
  timeout: live ? 600_000 : 60_000,
  forbidOnly: !!process.env.CI,
  reporter: 'list',
  use: { baseURL: 'http://localhost:3002', trace: 'retain-on-failure' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    ...(live ? [] : [{ command: 'node e2e/mock-api.mjs', port: 8799, reuseExistingServer: !process.env.CI }]),
    {
      // The standalone server the droplet runs; rewrites are fixed at build time, so the API URL is a build input.
      command: 'npm run build && cp -r .next/static .next/standalone/.next/ && node .next/standalone/server.js',
      port: 3002,
      env: { TERMINAL_API_URL: live ? (process.env.TERMINAL_API_URL ?? 'http://127.0.0.1:8080') : 'http://127.0.0.1:8799', PORT: '3002', HOSTNAME: '127.0.0.1' },
      timeout: 300_000,
      // Live must never reuse a server built against the mock (rewrites are baked at build time).
      reuseExistingServer: !live && !process.env.CI,
    },
  ],
});
