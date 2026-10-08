import { expect, test, type Page } from '@playwright/test';

const stage = (page: Page, name: RegExp) => page.getByRole('list', { name: 'pipeline' }).getByRole('button', { name });

test('ERC-20: sample → five stages → a stepped run → the tables', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /ERC-20 token/ }).click();
  await expect(page.getByLabel('source editor')).toContainText('contract ERC20');
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  await expect(page.getByText('11 686 words')).toBeVisible();
  await stage(page, /Compile/).click();
  await expect(page.getByText('1 296 bytes', { exact: false })).toBeVisible();
  await stage(page, /Transpile/).click();
  await expect(page.getByRole('img', { name: /control-flow graph, 74 blocks/ })).toBeVisible();
  await stage(page, /Run on zkVM/).click();
  await page.getByRole('button', { name: 'Run on the zkVM' }).click();
  await expect(page.getByText('success')).toBeVisible();
  await expect(page.getByText(/^tier 18$/)).toBeVisible();
  await expect(page.getByText('66 235')).toBeVisible();
  // Keys are handled on the focused debugger, not on window.
  await page.getByLabel('debugger').focus();
  const state1 = page.waitForResponse((r) => /\/runs\/[^/]+\/state\?at=1$/.test(r.url()) && r.status() === 200);
  await page.keyboard.press('ArrowRight');
  await state1;
  await expect(page).toHaveURL(/#clk=1$/);
  await expect(page.getByText('RV32IM · cycle 1', { exact: true })).toBeVisible();
  await expect(page.getByLabel('registers')).toBeVisible();
  await expect(page.getByRole('alert').filter({ hasText: 'Could not load' })).toHaveCount(0);
  await page.getByRole('tab', { name: 'STARK tables' }).click();
  await expect(page.getByText(/needs tier 18/)).toBeVisible();
});

test('no horizontal scroll at phone width', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/', { waitUntil: 'networkidle' });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});
