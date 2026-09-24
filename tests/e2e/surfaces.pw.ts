import { expect, type Page, test } from "@playwright/test";
import type { NativeBridge } from "../../src/platform/native";

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
  await expect(page.getByRole("heading", { name: "AI", exact: true })).toBeVisible();
  await expect(page.getByLabel("Google AI Studio API key")).toBeVisible();
  await expect(page.getByText("Models", { exact: true })).toBeVisible();
  const reasoningSelect = page.getByLabel("AI reasoning mode", { exact: true });
  await expect(reasoningSelect).toHaveValue("fast");
  await reasoningSelect.selectOption("balanced");
  await expect(reasoningSelect).toHaveValue("balanced");
  const firstModel = page.getByLabel("Model 1 of 1 (main model)", { exact: true });
  await expect(firstModel).toBeVisible();
  const options = await firstModel.locator("option").allTextContents();
  expect(options.length).toBeGreaterThan(0);
  for (const option of options) {
    expect(option.toLowerCase()).not.toContain("tts");
    expect(option.toLowerCase()).not.toContain("image");
    expect(option.toLowerCase()).not.toContain("banana");
    expect(option.toLowerCase()).not.toContain("live");
  }
  await firstModel.selectOption("gemini-3.6-flash");
  await expect(firstModel).toHaveValue("gemini-3.6-flash");
  await page.getByRole("button", { name: "+ Add fallback", exact: true }).click();
  const queueFirst = page.getByLabel("Model 1 of 2 (main model)", { exact: true });
  const queueSecond = page.getByLabel("Model 2 of 2 (fallback 1)", { exact: true });
  await expect(queueFirst).toHaveValue("gemini-3.6-flash");
  await expect(queueSecond).toBeVisible();
  await page
    .getByRole("button", { name: "Move Gemini 3.6 Flash down (now main model)", exact: true })
    .click();
  await expect(page.getByLabel("Model 1 of 2 (main model)", { exact: true })).not.toHaveValue(
    "gemini-3.6-flash",
  );
  await expect(page.getByLabel("Model 2 of 2 (fallback 1)", { exact: true })).toHaveValue(
    "gemini-3.6-flash",
  );
  await page
    .getByRole("button", { name: "Remove Gemini 3.6 Flash (fallback 1)", exact: true })
    .click();
  await expect(page.getByLabel("Model 1 of 1 (main model)", { exact: true })).toBeVisible();
  assertNoErrors();
});

test("AI provider switch shows per-provider keys and model costs", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "AI", exact: true }).click();
  // Native settings persistence is slower than React's optimistic provider
  // switch. Recreate that timing so model discovery must use the provider
  // requested by the picker rather than whichever provider is persisted yet.
  await page.evaluate(async () => {
    const path = "/src/platform/native.ts";
    const { nativeBridge } = (await import(path)) as { nativeBridge: NativeBridge };
    const updateSettings = nativeBridge.updateSettings.bind(nativeBridge);
    nativeBridge.updateSettings = async (patch) => {
      if (patch.aiProvider !== undefined) {
        await new Promise((resolve) => window.setTimeout(resolve, 150));
      }
      return updateSettings(patch);
    };
  });
  const providerSelect = page.getByLabel("AI provider", { exact: true });
  await expect(providerSelect).toBeVisible();
  await providerSelect.selectOption("zen");
  await expect(page.getByLabel("OpenCode API key")).toBeVisible();
  const modelSelect = page.getByLabel("Model 1 of 1 (main model)", { exact: true });
  // Zen rows show their per-1M cost under the picker so the price is visible up front.
  await expect(
    page.locator('[data-testid="ai-model-cost"]', { hasText: "per 1M" }).first(),
  ).toBeVisible();
  await providerSelect.selectOption("go");
  await expect(modelSelect).toHaveValue("glm-5.3-flash");
  await expect(modelSelect.locator('option[value="gemini-3.8-flash"]')).toHaveCount(0);
  await providerSelect.selectOption("custom");
  await expect(page.getByLabel("Custom base URL")).toBeVisible();
  await expect(modelSelect).toHaveValue("llama3.1");
  await providerSelect.selectOption("gemini");
  await expect(modelSelect).toHaveValue("gemini-3.8-flash");
  assertNoErrors();
});

test("on-device dictation models and local AI setup work", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "Dictation", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Dictation" })).toBeVisible();
  await page.getByRole("radio", { name: "On-device", exact: true }).click();
  await expect(
    page.getByText("Models run entirely on this device", { exact: false }),
  ).toBeVisible();
  // Small is the recommended model and is pre-selected by the harness.
  await expect(page.getByText("Recommended", { exact: true })).toBeVisible();
  // Downloading Tiny marks it installed and selects it.
  const tinyRow = page.locator('[data-testid="local-model"]').filter({ hasText: "Whisper Tiny" });
  await expect(tinyRow.locator('[data-testid="local-model-bar"]')).toHaveCount(2);
  await tinyRow.getByRole("button", { name: "Download" }).click();
  await expect(tinyRow.getByRole("button", { name: "Delete" })).toBeVisible();
  // Dictation cleanup can be pinned to its own model instead of the queue.
  const cleanupSelect = page.getByLabel("Dictation cleanup model", { exact: true });
  await expect(cleanupSelect).toBeVisible();
  await cleanupSelect.selectOption("gemini-3.6-flash");
  await expect(cleanupSelect).toHaveValue("gemini-3.6-flash");
  await cleanupSelect.selectOption("");
  await expect(cleanupSelect).toHaveValue("");
  // Local AI detection lives under the Custom provider.
  await page.getByRole("button", { name: "AI", exact: true }).click();
  await page.getByLabel("AI provider", { exact: true }).selectOption("custom");
  await expect(page.getByText("Local servers", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Install Ollama", exact: true })).toBeVisible();
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
    const { nativeBridge } = (await import(path)) as { nativeBridge: NativeBridge };
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
  await expect(
    page
      .locator('[data-testid="settings-notice"]')
      .getByText("Update installed. Restart Kivo to finish.", { exact: true }),
  ).toBeVisible();
  assertNoErrors();
});

test("about installs rolling beta updates in-app", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "About", exact: true }).click();
  await page.evaluate(async () => {
    const path = "/src/platform/native.ts";
    const { nativeBridge } = (await import(path)) as {
      nativeBridge: NativeBridge;
    };
    nativeBridge.checkForUpdates = async () => ({
      currentVersion: "0.1.0",
      availableVersion: "0.1.0",
      available: true,
      channel: "beta",
      currentSha: "aaa",
      availableSha: "bbb",
    });
  });
  await page.getByRole("button", { name: "Check now", exact: true }).click();
  await expect(page.getByText("A newer beta build is available (bbb).")).toBeVisible();
  await page.getByRole("button", { name: "Download and Install", exact: true }).click();
  await expect(page.getByRole("button", { name: "Restart now", exact: true })).toBeVisible();
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
  await expect(
    page.locator('[data-testid="shortcut-demo"]').getByText("Writing Tools", { exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Model 1 of 1 (main model)", { exact: true })).toBeVisible();
  assertNoErrors();
});

test("writing tools supports keyboard custom instructions and informational results", async ({
  page,
}) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 344, height: 420 });
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.getByRole("dialog", { name: "Writing Tools" })).toBeVisible();
  await page.keyboard.type("Translate to French");
  await expect(page.getByLabel("Custom writing instruction")).toHaveValue("Translate to French");
  await page.keyboard.press("Escape");
  await page.getByRole("menuitem", { name: "Summarize" }).click();
  await expect(page.getByText(/short greeting/)).toBeVisible({ timeout: 2_000 });
  await expect(page.getByRole("button", { name: "Copy" })).toBeVisible();
  assertNoErrors();
});

test("writing presets can be added, edited, and reset", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 900, height: 720 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "Writing Tools", exact: true }).click();

  await page.getByRole("button", { name: "Add preset", exact: true }).click();
  await page.getByRole("textbox", { name: "Name", exact: true }).fill("Pirate");
  await page
    .getByRole("textbox", { name: "What it should do", exact: true })
    .fill("Rewrite the text like a pirate.");
  await page.getByRole("button", { name: "Save preset", exact: true }).click();
  await expect(page.getByText("Pirate", { exact: true })).toBeVisible();

  // The new preset reaches the popup menu in the harness.
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.getByRole("menuitem", { name: "Pirate", exact: true })).toBeVisible();

  // Editing a built-in renames it in the menu.
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "Writing Tools", exact: true }).click();
  await page.getByRole("button", { name: "Edit", exact: true }).first().click();
  await page.getByRole("textbox", { name: "Name", exact: true }).fill("Spellcheck");
  await page.getByRole("button", { name: "Save preset", exact: true }).click();
  await expect(page.getByText("Spellcheck", { exact: true })).toBeVisible();

  // Reset restores the built-in defaults and drops the custom preset.
  await page.getByRole("button", { name: "Reset all to defaults", exact: true }).click();
  await expect(page.getByText("Spellcheck", { exact: true })).toHaveCount(0);
  await expect(page.getByText("Proofread", { exact: true })).toBeVisible();
  await expect(page.getByText("Pirate", { exact: true })).toHaveCount(0);
  assertNoErrors();
});

test("flow bar exposes calm listening, processing, and error states", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 260, height: 72 });
  for (const state of ["idle", "starting", "listening", "processing", "error"]) {
    await page.setViewportSize(
      state === "error" ? { width: 380, height: 96 } : { width: 164, height: 48 },
    );
    await page.goto(`/?surface=flow-bar&state=${state}`);
    await expect(page.locator('[data-testid="flow-bar"]')).toBeVisible();
  }
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.locator('[data-testid="flow-bar"][data-state="listening"]')).toBeVisible();
  assertNoErrors();
});
