import { expect, test } from "@playwright/test";

for (const surface of ["settings", "onboarding", "flow-bar", "writing-tools"] as const) {
  test(`built Windows frontend renders ${surface} without runtime errors`, async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(message.text());
    });
    await page.goto(`/?surface=${surface}&harness=1`);
    await expect(page.locator("html")).toHaveAttribute("data-ready", "true", { timeout: 15_000 });
    if (surface === "settings") {
      await page.getByRole("button", { name: "Settings", exact: true }).click();
      await expect(page.getByRole("heading", { name: "Settings", exact: true })).toBeVisible();
    }
    expect(errors).toEqual([]);
  });
}
