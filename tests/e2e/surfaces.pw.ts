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
  await expect(page.getByRole("heading", { name: "Home", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "General", exact: true }).click();
  await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  await page.getByRole("button", { name: "AI", exact: true }).click();
  await expect(page.getByRole("heading", { name: "AI" })).toBeVisible();
  await expect(page.getByLabel("Google AI Studio API key")).toBeVisible();
  const modelSelect = page.getByLabel("AI model", { exact: true });
  await expect(modelSelect).toBeVisible();
  const options = await modelSelect.locator("option").allTextContents();
  expect(options.length).toBeGreaterThan(0);
  for (const option of options) {
    expect(option.toLowerCase()).not.toContain("tts");
    expect(option.toLowerCase()).not.toContain("image");
    expect(option.toLowerCase()).not.toContain("banana");
    expect(option.toLowerCase()).not.toContain("live");
  }
  await modelSelect.selectOption("gemini-2.5-flash");
  await expect(modelSelect).toHaveValue("gemini-2.5-flash");
  const backupSelect = page.getByLabel("Backup AI model", { exact: true });
  await expect(backupSelect).toBeVisible();
  await backupSelect.selectOption("gemini-2.5-flash-lite");
  await expect(backupSelect).toHaveValue("gemini-2.5-flash-lite");
  assertNoErrors();
});

test("about installs stable updates in-app with restart", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "About", exact: true }).click();
  await expect(page.getByRole("heading", { name: "About" })).toBeVisible();
  await page.evaluate(async () => {
    const path = "/src/platform/native.ts";
    const { nativeBridge } = await import(path);
    nativeBridge.checkForUpdates = async () => ({
      currentVersion: "0.1.0",
      availableVersion: "0.2.0",
      available: true,
      downloadUrl: "https://github.com/0libote/Kivo/releases",
      channel: "stable",
    });
  });
  await page.getByRole("button", { name: "Check now", exact: true }).click();
  await page.getByRole("button", { name: "Download and Install", exact: true }).click();
  await expect(page.getByRole("button", { name: "Restart now", exact: true })).toBeVisible();
  await expect(page.locator(".settings-notice").getByText("Update installed. Restart Kivo to finish.", { exact: true })).toBeVisible();
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
  await expect(page.getByLabel("AI model", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Backup AI model", { exact: true })).toBeVisible();
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
  await page.getByText("More actions", { exact: true }).click();
  await page.getByRole("option", { name: "Summarize" }).click();
  await expect(page.getByText(/short greeting/)).toBeVisible({ timeout: 2_000 });
  await expect(page.getByRole("button", { name: "Copy" })).toBeVisible();
  assertNoErrors();
});

test("flow bar exposes calm listening, processing, and error states", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 260, height: 72 });
  for (const state of ["idle", "starting", "listening", "processing", "error"]) {
    await page.setViewportSize(state === "error" ? { width: 380, height: 96 } : { width: 164, height: 48 });
    await page.goto(`/?surface=flow-bar&state=${state}`);
    await expect(page.locator(".flow-bar")).toBeVisible();
  }
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.locator('.flow-bar[data-state="listening"]')).toBeVisible();
  assertNoErrors();
});
