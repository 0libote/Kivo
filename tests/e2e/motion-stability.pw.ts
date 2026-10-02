import { expect, type Page, test } from "@playwright/test";

function failOnConsoleErrors(page: Page) {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return () => expect(errors, "browser console errors").toEqual([]);
}

async function expectNoHorizontalOverflow(page: Page) {
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
    "no horizontal overflow",
  ).toBe(true);
}

test("overlays open calmly with no overflow in either theme", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  // Writing Tools menu at its compact width.
  await page.setViewportSize({ width: 344, height: 420 });
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.getByRole("dialog", { name: "Writing Tools" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Proofread" })).toBeVisible();
  await expectNoHorizontalOverflow(page);
  // Menu -> summary choice -> back is an instant swap, not a spring.
  await page.getByRole("menuitem", { name: "Summarize" }).click();
  await expect(page.getByRole("button", { name: "Preview first" })).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await page.getByRole("button", { name: "Other actions" }).click();
  await expect(page.getByRole("menuitem", { name: "Proofread" })).toBeVisible();

  // Flow bar states never overflow their pill.
  await page.setViewportSize({ width: 200, height: 60 });
  await page.goto("/?surface=flow-bar&state=listening&harness=1");
  await expect(page.locator('[data-testid="flow-bar"][data-state="listening"]')).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await page.setViewportSize({ width: 400, height: 120 });
  await page.goto("/?surface=flow-bar&state=error&harness=1");
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  await expectNoHorizontalOverflow(page);
  assertNoErrors();
});

test("onboarding steps transition and settings sections fit at narrow widths", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 640, height: 560 });
  await page.goto("/?surface=onboarding&harness=1");
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Work with text everywhere" })).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Dictation uses system speech" })).toBeVisible();
  await expectNoHorizontalOverflow(page);

  await page.setViewportSize({ width: 620, height: 500 });
  await page.goto("/?surface=settings&harness=1");
  // Sequential on purpose: each click mutates the page, so the sections
  // cannot be checked in parallel. Listed explicitly (no loop) so the
  // sequencing is obvious to readers and async linters alike.
  async function expectSectionFits(section: string) {
    await page.getByRole("button", { name: section, exact: true }).click();
    await expectNoHorizontalOverflow(page);
  }
  await expectSectionFits("Home");
  await expectSectionFits("General");
  await expectSectionFits("Dictation");
  await expectSectionFits("Writing Tools");
  await expectSectionFits("AI");
  await expectSectionFits("About");
  assertNoErrors();
});

test("reduced-motion still opens every surface", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 344, height: 420 });
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.getByRole("menuitem", { name: "Proofread" })).toBeVisible();
  await page.setViewportSize({ width: 200, height: 60 });
  await page.goto("/?surface=flow-bar&state=listening&harness=1");
  await expect(page.locator('[data-testid="flow-bar"][data-state="listening"]')).toBeVisible();
  await page.setViewportSize({ width: 640, height: 560 });
  await page.goto("/?surface=onboarding&harness=1");
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Work with text everywhere" })).toBeVisible();
  assertNoErrors();
});
