import { expect, test, type Page } from '@playwright/test';

const stage = (page: Page, name: RegExp) => page.getByRole('list', { name: 'pipeline' }).getByRole('button', { name });

test('live: ERC-20 end to end', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /ERC-20 token/ }).click();
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  await expect(page.getByText(/words of a/)).toBeVisible({ timeout: 300_000 });
  await stage(page, /Run on zkVM/).click();
  await page.getByRole('button', { name: 'Run on the zkVM' }).click();
  await expect(page.getByText('success')).toBeVisible({ timeout: 120_000 });
});

test('live: a syntax error is an editor diagnostic', async ({ page }) => {
  await page.goto('/');
  const ed = page.getByLabel('source editor').locator('.cm-content');
  await ed.click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.type('pragma solidity ^0.8.20;\ncontract A { function f() public { uint x = } }\n');
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  await expect(page.getByRole('alert')).toContainText('compile', { timeout: 60_000 });
  await expect(page.locator('.cm-lintRange-error')).toHaveCount(1);
});

test('live: Solana counter from Rust source', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /Solana counter/ }).click();
  await page.getByRole('button', { name: 'Compile & Run' }).click();
  await expect(page.getByText(/words of a/)).toBeVisible({ timeout: 600_000 });
  await stage(page, /Run on zkVM/).click();
  await page.getByRole('button', { name: 'Run on the zkVM' }).click();
  await expect(page.getByText('success')).toBeVisible({ timeout: 120_000 });
});
