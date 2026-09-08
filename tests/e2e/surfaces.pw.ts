import { expect, test, type Page } from "@playwright/test";

function failOnConsoleErrors(page: Page) {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  return () => expect(errors, "browser console errors").toEqual([]);
}

test("settings navigation and controls work", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  await page.getByRole("button", { name: "AI", exact: true }).click();
  await expect(page.getByRole("heading", { name: "AI" })).toBeVisible();
  await expect(page.getByLabel("Google AI Studio API key")).toBeVisible();
  assertNoErrors();
});

test("onboarding completes the concise four-screen flow", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 640, height: 560 });
  await page.goto("/?surface=onboarding&harness=1");
  await expect(page.getByRole("heading", { name: /Write naturally/ })).toBeVisible();
  await page.getByRole("button", { name: "Get started" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Dictation uses system speech" })).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: /Add Gemini/ })).toBeVisible();
  await expect(page.locator(".shortcut-demo").getByText("Writing Tools", { exact: true })).toBeVisible();
  assertNoErrors();
});

test("writing tools supports keyboard custom instructions and informational results", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 344, height: 420 });
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.getByRole("dialog", { name: "Writing Tools" })).toBeVisible();
  await page.keyboard.type("Translate to French");
  await expect(page.getByLabel("Custom writing instruction")).toHaveValue("Translate to French");
  await page.keyboard.press("Escape");
  await page.getByRole("option", { name: "Summarize" }).click();
  await expect(page.getByText(/short greeting/)).toBeVisible({ timeout: 2_000 });
  await expect(page.getByRole("button", { name: "Copy" })).toBeVisible();
  assertNoErrors();
});

test("flow bar exposes calm listening, processing, and error states", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 260, height: 72 });
  for (const state of ["idle", "listening", "processing", "error"]) {
    await page.goto(`/?surface=flow-bar&state=${state}`);
    await expect(page.locator(".flow-bar")).toBeVisible();
  }
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  assertNoErrors();
});
