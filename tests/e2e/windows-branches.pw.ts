import { expect, test } from "@playwright/test";

// Exercises the Windows frontend branches without a Windows runner: the
// browser harness derives its platform from navigator.userAgent, so a
// Windows UA flips every platform branch (defaults, permission copy,
// shortcut labels) exactly as on a real Windows WebView.
test.use({
  userAgent:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
});

test("onboarding shows the Windows no-permission-prompt note", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 560 });
  await page.goto("/?surface=onboarding&harness=1");
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByText("No permission prompt is normally required.")).toBeVisible();
});

test("settings shows the Windows dictation language and shortcut defaults", async ({ page }) => {
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "Dictation", exact: true }).click();
  await expect(
    page.getByText("Only installed languages can start dictation."),
  ).toBeVisible();
  // Ctrl+Meta renders with the Win label, never a bare Meta.
  await expect(page.getByText("Win", { exact: true }).first()).toBeVisible();
});
