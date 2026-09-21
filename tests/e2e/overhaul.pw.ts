import { expect, test } from "@playwright/test";

for (const platform of ["windows", "macos"] as const) {
  test(`${platform}: Home, recovery and every settings section work in both themes`, async ({ browser }, testInfo) => {
    const context = await browser.newContext({ viewport: { width: 900, height: 650 }, userAgent: platform === "windows" ? "Windows" : "Macintosh" });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.goto("/?surface=settings");
    await expect(page.getByRole("heading", { name: "Home", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Change dictation shortcut", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Dictation", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Home", exact: true }).click();
    await page.getByRole("button", { name: "Change Writing Tools shortcut", exact: true }).click();
    await expect(page.getByLabel("Popup width")).toHaveCount(0);
    await expect(page.getByLabel("Editable selected text")).toHaveCount(0);
    await page.getByRole("button", { name: "Home", exact: true }).click();
    await page.getByRole("button", { name: "Try dictation" }).click();
    await expect(page.getByLabel("Dictation practice")).toBeFocused();
    for (const theme of ["light", "dark"] as const) {
      await page.evaluate(async theme => {
        const path = "/src/platform/native.ts";
        const { nativeBridge } = await import(path);
        await nativeBridge.updateSettings({ theme });
      }, theme);
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await page.screenshot({ path: testInfo.outputPath(`home-${theme}.png`) });
    }
    await page.evaluate(async () => {
      const path = "/src/platform/native.ts";
      const { nativeBridge } = await import(path);
      nativeBridge.getDictationRecovery = async () => "A sentence recovered after the original field changed.";
      nativeBridge.emit("recovery-changed", null);
      await nativeBridge.setPaused(true);
    });
    await expect(page.getByRole("region", { name: "Last dictation" })).toBeVisible();
    await expect(page.getByText("Kivo is paused", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Resume Kivo" }).click();
    await expect(page.getByText("Kivo is paused", { exact: true })).toHaveCount(0);
    await page.getByRole("button", { name: "Clear", exact: true }).click();
    await expect(page.getByRole("region", { name: "Last dictation" })).toHaveCount(0);
    await page.setViewportSize({ width: 620, height: 500 });
    for (const section of ["General", "Dictation", "Writing Tools", "AI", "About"]) {
      await page.getByRole("button", { name: section, exact: true }).click();
      await expect(page.getByRole("heading", { name: section, exact: true }).first()).toBeVisible();
      expect(await page.locator(".settings-main").evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    }
    await page.emulateMedia({ forcedColors: "active", reducedMotion: "reduce" });
    await page.getByRole("button", { name: "Home", exact: true }).click();
    await page.getByRole("button", { name: "Try dictation" }).click();
    await expect(page.getByLabel("Dictation practice")).toBeFocused();
    await page.screenshot({ path: testInfo.outputPath("home-high-contrast.png") });
    expect(errors).toEqual([]);
    await context.close();
  });
}

test("Every writing action is visible and recording can finish from its indicator", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 380, height: 460 });
  await page.goto("/?surface=writing-tools");
  await expect(page.getByRole("menuitem", { name: "Proofread", exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("writing-menu.png") });
  await expect(page.getByRole("menuitem", { name: "Summarize", exact: true })).toBeVisible();
  for (let index = 0; index < 5; index++) await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("menuitem", { name: "Summarize", exact: true })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Copy", exact: true })).toBeVisible();
  await page.setViewportSize({ width: 164, height: 48 });
  await page.goto("/?surface=flow-bar&state=listening");
  await expect(page.getByRole("button", { name: "Finish dictation" })).toBeInViewport();
  await page.screenshot({ path: testInfo.outputPath("listening.png") });
  await page.getByRole("button", { name: "Finish dictation" }).click();
  await expect(page.locator('.flow-bar[data-state="processing"]')).toBeVisible();
  await page.setViewportSize({ width: 380, height: 96 });
  await page.goto("/?surface=flow-bar&state=error");
  await expect(page.getByRole("button", { name: "Dismiss dictation error" })).toBeInViewport();
  await page.screenshot({ path: testInfo.outputPath("dictation-error.png") });
  await page.getByRole("button", { name: "Dismiss dictation error" }).click();
  await expect(page.locator('.flow-bar[data-state="hidden"]')).toHaveCount(1);
});

test("writing settings stay focused on actions and navigation resets scroll", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 900, height: 650 });
  await page.goto("/?surface=settings");
  await page.getByRole("button", { name: "Writing Tools", exact: true }).click();
  await expect(page.getByRole("button", { name: "Reset all to defaults", exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("writing-settings.png") });
  await expect(page.getByText("Popup appearance", { exact: true })).toHaveCount(0);
  await page.setViewportSize({ width: 620, height: 500 });
  await page.getByRole("button", { name: "Reset all to defaults", exact: true }).scrollIntoViewIfNeeded();
  await page.getByRole("button", { name: "AI", exact: true }).click();
  await expect(page.getByRole("heading", { name: "AI", exact: true })).toBeInViewport();
});
