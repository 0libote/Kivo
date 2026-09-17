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
  await expect(page.getByText("Models in order")).toBeVisible();
  const firstModel = page.getByLabel("Model 1 of 1", { exact: true });
  await expect(firstModel).toBeVisible();
  const options = await firstModel.locator("option").allTextContents();
  expect(options.length).toBeGreaterThan(0);
  for (const option of options) {
    expect(option.toLowerCase()).not.toContain("tts");
    expect(option.toLowerCase()).not.toContain("image");
    expect(option.toLowerCase()).not.toContain("banana");
    expect(option.toLowerCase()).not.toContain("live");
  }
  await firstModel.selectOption("gemini-2.5-flash");
  await expect(firstModel).toHaveValue("gemini-2.5-flash");
  await page.getByRole("button", { name: "+ Add model", exact: true }).click();
  const queueFirst = page.getByLabel("Model 1 of 2", { exact: true });
  const queueSecond = page.getByLabel("Model 2 of 2", { exact: true });
  await expect(queueFirst).toHaveValue("gemini-2.5-flash");
  await expect(queueSecond).toBeVisible();
  await page.getByRole("button", { name: "Move Gemini 2.5 Flash down", exact: true }).click();
  await expect(page.getByLabel("Model 1 of 2", { exact: true })).not.toHaveValue("gemini-2.5-flash");
  await expect(page.getByLabel("Model 2 of 2", { exact: true })).toHaveValue("gemini-2.5-flash");
  await page.getByRole("button", { name: "Remove Gemini 2.5 Flash", exact: true }).click();
  await expect(page.getByLabel("Model 1 of 1", { exact: true })).toBeVisible();
  assertNoErrors();
});

test("AI provider switch shows per-provider keys and model costs", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "AI", exact: true }).click();
  const providerSelect = page.getByLabel("AI provider", { exact: true });
  await expect(providerSelect).toBeVisible();
  await providerSelect.selectOption("zen");
  await expect(page.getByLabel("OpenCode API key")).toBeVisible();
  const modelSelect = page.getByLabel("Model 1 of 1", { exact: true });
  // Zen options carry their per-1M cost so the price is visible up front.
  await expect(modelSelect.locator("option", { hasText: "per 1M" }).first()).toBeAttached();
  await providerSelect.selectOption("custom");
  await expect(page.getByLabel("Custom base URL")).toBeVisible();
  await expect(page.getByLabel("Model 1 of 1", { exact: true })).toBeVisible();
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
  await expect(page.getByRole("heading", { name: /Add AI/ })).toBeVisible();
  await expect(page.locator(".shortcut-demo").getByText("Writing Tools", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Model 1 of 1", { exact: true })).toBeVisible();
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
