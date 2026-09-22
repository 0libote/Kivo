import { expect, type Page, test } from "@playwright/test";
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

const textSelection = {
  hasSelection: true,
  applicationName: "Editor",
  canReplace: true,
  initialText: "A transcript describing the project.",
};
const linkSelection = { ...textSelection, initialText: "https://example.com/article" };
const emptySelection = {
  hasSelection: false,
  applicationName: "Editor",
  canReplace: false,
  initialText: "",
};

async function installHarness(page: Page, context: SelectionContext) {
  await page.goto("/?surface=writing-tools");
  await page.evaluate(
    async ({ context }) => {
      const path = "/src/platform/native.ts";
      const { nativeBridge } = (await import(path)) as {
        nativeBridge: NativeBridge & { emit: (event: string, context: SelectionContext) => void };
      };
      window.summaryTest = {
        requests: [],
        pending: [],
        open: (next) => nativeBridge.emit("writing-context", next),
      };
      nativeBridge.runWritingAction = (request) => {
        window.summaryTest.requests.push(request);
        return new Promise((resolve, reject) =>
          window.summaryTest.pending.push({ resolve, reject }),
        );
      };
      window.summaryTest.open(context);
    },
    { context },
  );
}

for (const platform of ["macos", "windows"] as const) {
  for (const theme of ["light", "dark"] as const) {
    test(`${platform} ${theme}: selected text and links use the compact Flow surface`, async ({
      browser,
    }, testInfo) => {
      const context = await browser.newContext({
        viewport: { width: 360, height: 420 },
        userAgent: platform === "windows" ? "Windows" : "Macintosh",
      });
      const page = await context.newPage();
      await page.addInitScript(
        (value) => localStorage.setItem("kivo-dev-settings", JSON.stringify({ theme: value })),
        theme,
      );
      await installHarness(page, textSelection);

      await page.getByRole("menuitem", { name: "Summarize", exact: true }).click();
      await expect
        .poll(() => page.evaluate(() => window.summaryTest.requests))
        .toMatchObject([
          { action: "summarize", text: textSelection.initialText, sourceKind: "text" },
        ]);
      await page.evaluate(() =>
        window.summaryTest.pending[0].resolve({
          kind: "result",
          text: "A useful summary.",
          canReplace: false,
        }),
      );
      await expect(page.getByRole("button", { name: "Copy", exact: true })).toBeVisible();

      await page.evaluate((selection) => window.summaryTest.open(selection), linkSelection);
      await expect(page.getByText("example.com", { exact: true })).toBeVisible();
      await expect(page.getByRole("textbox")).toHaveCount(0);
      await page.screenshot({ path: testInfo.outputPath("link-confirmation.png") });
      await page.getByRole("button", { name: "Summarize", exact: true }).click();
      await expect
        .poll(() => page.evaluate(() => window.summaryTest.requests[1]))
        .toMatchObject({
          action: "summarize",
          text: linkSelection.initialText,
          sourceKind: "link",
        });
      await page.evaluate(
        (url) =>
          window.summaryTest.pending[1].resolve({
            kind: "result",
            text: "A link summary.",
            source: { kind: "website", url },
            canReplace: true,
          }),
        linkSelection.initialText,
      );
      await expect(page.getByText("A link summary.", { exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Replace", exact: true })).toHaveCount(0);
      await context.close();
    });
  }
}

test("empty selections stop at guidance instead of becoming a general AI textbox", async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 180 });
  await installHarness(page, emptySelection);
  await expect(page.getByText("Select some text first.", { exact: true })).toBeVisible();
  await expect(page.getByRole("textbox")).toHaveCount(0);
});

test("link summary retry keeps the captured URL without editable fallback", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 240 });
  await installHarness(page, linkSelection);
  await page.getByRole("button", { name: "Summarize", exact: true }).click();
  await page.evaluate(async () => {
    const path = "/src/types.ts";
    const { NativeError } = await import(path);
    window.summaryTest.pending[0].reject(
      new NativeError({ code: "unavailable", message: "Content unavailable.", recoverable: true }),
    );
  });
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.getByText("Summarizing link", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /Paste text/ })).toHaveCount(0);
});

test("disabling Summarize leaves highlighted links in the normal preset menu", async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem(
      "kivo-dev-settings",
      JSON.stringify({ enabledWritingActions: ["proofread", "custom"] }),
    ),
  );
  await installHarness(page, linkSelection);
  await expect(page.getByRole("menuitem", { name: "Proofread", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Summarize", exact: true })).toHaveCount(0);
});
