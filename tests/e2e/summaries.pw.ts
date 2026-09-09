import { expect, test, type Page } from "@playwright/test";
import type { NativeBridge } from "../../src/platform/native";
import type { SelectionContext, WritingRequest, WritingResponse } from "../../src/types";

declare global {
  interface Window {
    summaryTest: {
      requests: WritingRequest[];
      pending: { resolve: (response: WritingResponse) => void; reject: (error: unknown) => void }[];
      open: (context: SelectionContext) => void;
    };
  }
}

const selection = { hasSelection: true, applicationName: "Editor", canReplace: true, initialText: "https://example.com/article" };
const chat = { hasSelection: false, applicationName: "Quick chat", canReplace: false, initialText: "" };

async function installHarness(page: Page, context = chat) {
  await page.goto("/?surface=writing-tools");
  await expect(page.getByRole("option", { name: "Summarize", exact: true })).toBeVisible();
  await page.evaluate(async (context) => {
    const path = "/src/platform/native.ts";
    const { nativeBridge } = await import(path) as { nativeBridge: NativeBridge & { emit: (event: string, context: SelectionContext) => void } };
    const harness: Window["summaryTest"] = {
      requests: [],
      pending: [],
      open: (context) => nativeBridge.emit("writing-context", context),
    };
    window.summaryTest = harness;
    nativeBridge.runWritingAction = (request) => {
      harness.requests.push(request);
      return new Promise((resolve, reject) => harness.pending.push({ resolve, reject }));
    };
    harness.open(context);
  }, context);
}

for (const platform of ["macos", "windows"] as const) {
  for (const theme of ["light", "dark"] as const) {
    test(`${platform} ${theme}: text and link summaries fit a compact popup`, async ({ browser }, testInfo) => {
      const context = await browser.newContext({ viewport: { width: 300, height: 320 }, userAgent: platform === "windows" ? "Windows" : "Macintosh" });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
      await page.addInitScript((theme) => localStorage.setItem("kivo-dev-settings", JSON.stringify({ theme })), theme);
      await installHarness(page);
      await expect(page.getByLabel("Chat message")).toBeVisible();
      await page.getByRole("button", { name: "Summarize text…", exact: true }).click();
      await expect(page.getByRole("button", { name: "Summarize", exact: true })).toBeDisabled();
      await page.getByLabel("Webpage text or video transcript").fill("A transcript describing the project and its next steps.");
      await page.getByRole("button", { name: "Summarize", exact: true }).click();
      await expect.poll(() => page.evaluate(() => window.summaryTest.requests)).toEqual([{ action: "summarize", text: "A transcript describing the project and its next steps.", sourceKind: "text" }]);
      await page.evaluate(() => window.summaryTest.pending[0].resolve({ kind: "result", text: "The project has clear next steps.", canReplace: false }));
      await expect(page.getByRole("button", { name: "Copy", exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Replace", exact: true })).toHaveCount(0);
      await page.keyboard.press("Escape");
      await page.getByRole("button", { name: "Summarize link…", exact: true }).click();
      await page.getByLabel("Webpage or YouTube URL").fill("javascript:alert(1)");
      await expect(page.getByRole("button", { name: "Summarize", exact: true })).toBeDisabled();
      await page.getByLabel("Webpage or YouTube URL").fill("https://www.youtube.com/watch?v=example1234");
      await page.screenshot({ path: testInfo.outputPath("link-input.png") });
      await page.getByRole("button", { name: "Summarize", exact: true }).click();
      await page.evaluate(() => window.summaryTest.pending[1].resolve({ kind: "result", text: "A video overview.\n\n" + "- A useful detail.\n".repeat(30), source: { kind: "youtube", url: "https://www.youtube.com/watch?v=example1234" }, canReplace: false }));
      await expect(page.getByText("YouTube", { exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Copy", exact: true })).toBeInViewport();
      expect(await page.locator(".writing-result__body").evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath("link-result.png") });
      expect(errors).toEqual([]);
      await context.close();
    });
  }
}

test("selected links prefill, cannot replace, and recover through retry or pasted text", async ({ page }) => {
  await page.setViewportSize({ width: 300, height: 260 });
  await installHarness(page, selection);
  await page.getByRole("button", { name: "Summarize link…", exact: true }).click();
  await expect(page.getByLabel("Webpage or YouTube URL")).toHaveValue(selection.initialText);
  await page.getByRole("button", { name: "Summarize", exact: true }).click();
  await page.evaluate(async () => {
    const path = "/src/types.ts";
    const { NativeError } = await import(path);
    window.summaryTest.pending[0].reject(new NativeError({ code: "unavailable", message: "Content unavailable.", recoverable: true }));
  });
  await expect(page.getByRole("button", { name: "Paste text instead", exact: true })).toBeInViewport();
  await expect(page.getByRole("button", { name: "Retry", exact: true })).toBeInViewport();
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.getByText("Retrieving and summarizing…", { exact: true })).toBeVisible();
  await page.evaluate(() => window.summaryTest.pending[1].resolve({ kind: "result", text: "A summary", canReplace: true }));
  await expect(page.getByText("A summary", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Replace", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Summarize link…", exact: true }).click();
  await page.getByRole("button", { name: "Summarize", exact: true }).click();
  await page.evaluate(() => window.summaryTest.pending[2].reject(new Error("Unavailable")));
  await page.getByRole("button", { name: "Paste text instead", exact: true }).click();
  await expect(page.getByLabel("Webpage text or video transcript")).toBeVisible();
});

test("cancel and reopen discard late successes and failures without duplicate submissions", async ({ page }) => {
  await installHarness(page);
  await page.getByRole("button", { name: "Summarize text…", exact: true }).click();
  await page.getByLabel("Webpage text or video transcript").fill("A transcript");
  await page.locator(".writing-summary").evaluate((form) => {
    form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  });
  expect(await page.evaluate(() => window.summaryTest.requests.length)).toBe(1);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.evaluate((context) => window.summaryTest.open(context), chat);
  await page.getByLabel("Chat message").fill("New question");
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await page.evaluate(() => window.summaryTest.pending[0].resolve({ kind: "result", text: "Stale summary" }));
  await expect(page.getByText("Stale summary")).toHaveCount(0);
  await expect(page.locator('.writing-popup[data-mode="processing"]')).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.evaluate((context) => window.summaryTest.open(context), chat);
  await page.evaluate(() => window.summaryTest.pending[1].reject(new Error("Stale error")));
  await expect(page.getByLabel("Chat message")).toBeVisible();
  await expect(page.locator(".writing-error")).toHaveCount(0);
});

test("disabling Summarize hides both new entry points", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("kivo-dev-settings", JSON.stringify({ enabledWritingActions: ["proofread", "custom"] })));
  await page.goto("/?surface=writing-tools");
  await expect(page.getByRole("option", { name: "Proofread", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /Summarize/ })).toHaveCount(0);
  await page.evaluate(async (context) => {
    const path = "/src/platform/native.ts";
    const { nativeBridge } = await import(path);
    nativeBridge.emit("writing-context", context);
  }, chat);
  await expect(page.getByLabel("Chat message")).toBeVisible();
  await expect(page.getByRole("button", { name: /Summarize/ })).toHaveCount(0);
});
