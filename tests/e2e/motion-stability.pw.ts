import { expect, type Page, test } from "@playwright/test";
import type { NativeBridge } from "../../src/platform/native";

test("writing menu moves keyboard focus and ignores animation-only resize mutations", async ({
  page,
}) => {
  await page.setViewportSize({ width: 344, height: 420 });
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  const first = page.getByRole("menuitem", { name: "Proofread", exact: true });
  await expect(first).toBeFocused();
  await page.keyboard.press("ArrowDown");
  const selected = page.locator('[data-writing-action][data-selected="true"]');
  await expect(selected).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(first).toBeFocused();
  const calls = await page.evaluate(async () => {
    const path = "/src/platform/native.ts";
    const { nativeBridge } = (await import(path)) as { nativeBridge: NativeBridge };
    const original = nativeBridge.setSurfaceMode;
    let calls = 0;
    nativeBridge.setSurfaceMode = () => {
      calls += 1;
      return Promise.resolve();
    };
    try {
      const surface = document.querySelector<HTMLDialogElement>("dialog")?.firstElementChild;
      if (!(surface instanceof HTMLElement)) throw new Error("Missing writing surface");
      // Change styles on successive frames to exercise animation mutations
      // without introducing concurrent work or awaiting inside a loop.
      await new Promise<void>((resolve) => {
        let index = 0;
        const animateFrame = () => {
          if (index === 12) {
            surface.style.opacity = "1";
            resolve();
            return;
          }
          surface.style.opacity = String(index % 2 ? 1 : 0.95);
          index += 1;
          requestAnimationFrame(animateFrame);
        };
        requestAnimationFrame(animateFrame);
      });
      return calls;
    } finally {
      nativeBridge.setSurfaceMode = original;
    }
  });
  expect(calls).toBeLessThanOrEqual(1);
});

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
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
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
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  await expect(page.locator('[data-testid="flow-bar"][data-state="listening"]')).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await page.setViewportSize({ width: 400, height: 120 });
  await page.goto("/?surface=flow-bar&state=error&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  await expectNoHorizontalOverflow(page);
  assertNoErrors();
});

test("onboarding steps transition and settings sections fit at narrow widths", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.setViewportSize({ width: 640, height: 560 });
  await page.goto("/?surface=onboarding&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Work with text everywhere" })).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Dictation uses system speech" })).toBeVisible();
  await expectNoHorizontalOverflow(page);

  await page.setViewportSize({ width: 620, height: 500 });
  await page.goto("/?surface=settings&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  // Sequential on purpose: each click mutates the page, so the sections
  // cannot be checked in parallel. Listed explicitly (no loop) so the
  // sequencing is obvious to readers and async linters alike.
  async function expectSectionFits(section: string) {
    await page.getByRole("button", { name: section, exact: true }).click();
    await expectNoHorizontalOverflow(page);
  }
  await expectSectionFits("Home");
  await expectSectionFits("Settings");
  await expectSectionFits("Dictation");
  await expectSectionFits("Writing Tools");
  await expectSectionFits("AI");
  assertNoErrors();
});

test("reduced-motion still opens every surface", async ({ page }) => {
  const assertNoErrors = failOnConsoleErrors(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 344, height: 420 });
  await page.goto("/?surface=writing-tools&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  await expect(page.getByRole("menuitem", { name: "Proofread" })).toBeVisible();
  await page.setViewportSize({ width: 200, height: 60 });
  await page.goto("/?surface=flow-bar&state=listening&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  await expect(page.locator('[data-testid="flow-bar"][data-state="listening"]')).toBeVisible();
  await page.setViewportSize({ width: 640, height: 560 });
  await page.goto("/?surface=onboarding&harness=1");
  await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Work with text everywhere" })).toBeVisible();
  assertNoErrors();
});
