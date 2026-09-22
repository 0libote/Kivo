import { expect, type Page, test } from "@playwright/test";

// Exercises the Linux test-bench frontend branches without a Tauri runner:
// the browser harness derives its platform from navigator.userAgent, so a
// Linux UA flips every platform branch (defaults, permission copy, shortcut
// labels) exactly as under `bun tauri dev` on Linux.
test.use({
  userAgent:
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
});

function failOnConsoleErrors(page: Page) {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  return () => expect(errors, "browser console errors").toEqual([]);
}

test("bench surface probes every bit over the harness bridge", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 900, height: 700 });
  await page.goto("/?surface=gallery&harness=1");
  await expect(page.getByRole("heading", { name: "Test bench" })).toBeVisible();
  // Contract matrix renders with the Linux portable defaults.
  await expect(page.getByText("Control+Alt+Space").first()).toBeVisible();
  await expect(page.getByText(/accessibility=not-determined/).first()).toBeVisible();
  await expect(page.getByText(/microphone=granted/).first()).toBeVisible();
  // Dictation smoke drives the mock start/stop path without errors.
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  // Writing smoke returns the canned summary/hardware path result.
  await page.getByRole("button", { name: "Run proofread", exact: true }).click();
  await expect(page.getByText("Write + restore").first()).toBeVisible();
  assertNoErrors();
});

test("settings shows the Linux bench permission copy", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 820, height: 600 });
  await page.goto("/?surface=settings&harness=1");
  await page.getByRole("button", { name: "Permissions", exact: true }).click();
  await expect(
    page.getByText("Linux test bench: microphone and speech are simulated"),
  ).toBeVisible();
  assertNoErrors();
});
