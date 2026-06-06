import { test, expect } from "@playwright/test";

test("visits pricing page", async ({ page }) => {
  await page.goto("/pricing");
  await expect(page.locator("h1")).toContainText("Transparent Pricing");
});

test("clicking Start Pro Trial redirects to checkout API or returns 400", async ({ page }) => {
  await page.goto("/pricing");
  const [response] = await Promise.all([
    page.waitForResponse((resp) => resp.url().includes("/api/billing/create-checkout")),
    page.click("text=Start Pro Trial"),
  ]);
  expect([303, 400, 401]).toContain(response.status());
});

test("billing dashboard loads for authenticated users", async ({ page }) => {
  const email = `test-${Date.now()}@example.com`;
  const password = "password123";

  await page.goto("/auth/signup");
  await page.fill('input[type="text"]', "Test User");
  await page.fill('input[type="email"]', email);
  await page.fill('input[type="password"]', password);
  await page.click('button[type="submit"]');

  await page.waitForURL("/dashboard");

  await page.goto("/dashboard/billing");
  await expect(page.locator("h1")).toContainText("Billing");
});
