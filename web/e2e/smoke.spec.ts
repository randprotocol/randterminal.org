import { expect, test, type Page } from '@playwright/test';

const stage = (page: Page, name: RegExp) => page.getByRole('list', { name: 'pipeline' }).getByRole('button', { name });

// The rail repeats the stage's figures, so these checks read the stage panel.
test('Compile waits for a sample on its way: the sample is what gets built, and the job survives', async ({ page }) => {
  await page.route('**/api/v1/samples/erc20', async (route) => { await new Promise((r) => setTimeout(r, 800)); await route.continue(); });
  const job = page.waitForRequest((r) => r.method() === 'POST' && r.url().endsWith('/api/v1/jobs'));
  await page.goto('/');
  await page.getByRole('button', { name: /ERC-20 token/ }).click();
  const compile = page.getByRole('button', { name: /Compile & Run|Working/ });
  await expect(compile).toBeDisabled();
  await page.keyboard.press('ControlOrMeta+Enter');
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  expect((await job).postDataJSON().files[0].name).toBe('ERC20.sol');
  await expect(page.locator('#panel-stage').getByText('11 686 words')).toBeVisible();
});

test('ERC-20: sample → five stages → a stepped run → the tables', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /ERC-20 token/ }).click();
  await expect(page.getByLabel('source editor')).toContainText('contract ERC20');
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  await expect(page.locator('#panel-stage').getByText('11 686 words')).toBeVisible();
  await stage(page, /Compile/).click();
  await expect(page.locator('#panel-stage').getByText('1 296 bytes', { exact: false })).toBeVisible();
  await stage(page, /Transpile/).click();
  await expect(page.getByRole('img', { name: /control-flow graph, 74 blocks/ })).toBeVisible();
  await stage(page, /Run on zkVM/).click();
  await page.getByRole('button', { name: 'Run on the zkVM' }).click();
  await expect(page.getByText('success')).toBeVisible();
  await expect(page.getByLabel('pipeline').getByText(/needs tier 18/)).toBeVisible();
  await expect(page.locator('#panel-stage').getByText('Proof tier')).toBeVisible();
  await expect(page.locator('#panel-stage').getByText('66 235', { exact: true })).toBeVisible();
  await expect(page.getByLabel('pipeline').getByText('66 235')).toBeVisible();
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
  await expect(page.locator('#panel-stage').getByText(/needs tier 18/)).toBeVisible();
});

test('the EVM fork: picked beside the language, sent with the job, remembered, shown in the compile facts', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /ERC-20 token/ }).click();
  await expect(page.getByLabel('source editor')).toContainText('contract ERC20');
  const fork = page.getByLabel('EVM fork');
  await expect(fork).toHaveValue('shanghai');
  await expect(page.getByRole('note')).toHaveCount(0);
  await fork.selectOption('cancun');
  await expect(page.getByRole('note')).toContainText('MCOPY, TLOAD/TSTORE, CLZ');
  const job = page.waitForRequest((r) => r.method() === 'POST' && r.url().endsWith('/api/v1/jobs'));
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  expect((await job).postDataJSON().options.evm_version).toBe('cancun');
  await stage(page, /Compile/).click();
  await expect(page.locator('#panel-stage').getByText('EVM fork')).toBeVisible();
  // The pick lasts for the session.
  await page.reload();
  await page.getByRole('button', { name: /ERC-20 token/ }).click();
  await expect(page.getByLabel('EVM fork')).toHaveValue('cancun');
  // Solana has no fork.
  await page.getByLabel('language').selectOption('solana-rust');
  await expect(page.getByLabel('EVM fork')).toHaveCount(0);
});

test('Solana programs are never uploaded: no upload control, and the bundled sample goes by id', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('input[type=file]')).toHaveCount(0);
  const langs = await page.getByLabel('language').locator('option').allTextContents();
  expect(langs).toEqual(['Solidity', 'Solana (Rust)']);
  await page.getByRole('button', { name: /SPL Token/ }).click();
  await expect(page.getByText(/a program bundled with the terminal/)).toBeVisible();
  await expect(page.locator('input[type=file]')).toHaveCount(0);
  const job = page.waitForRequest((r) => r.method() === 'POST' && r.url().endsWith('/api/v1/jobs'));
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  const body = (await job).postDataJSON();
  expect(body).toMatchObject({ lang: 'solana-elf', sample: 'spl-token', files: [] });
  expect(JSON.stringify(body)).not.toContain('f0VMRg');
});

test('no horizontal scroll at phone width', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/', { waitUntil: 'networkidle' });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});
