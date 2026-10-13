import { afterEach, describe, expect, it, spyOn } from "bun:test";
import {
  getErrorBoundaryFailures,
  reportErrorBoundary,
  resetErrorBoundaryFailuresForTests,
} from "./error-report";

describe("reportErrorBoundary", () => {
  const errors = spyOn(console, "error").mockImplementation(() => {});
  afterEach(() => {
    errors.mockClear();
    resetErrorBoundaryFailuresForTests();
  });

  it("counts failures so crashes stay visible without user text", () => {
    expect(getErrorBoundaryFailures()).toBe(0);
    reportErrorBoundary(new Error("boom"));
    reportErrorBoundary(Object.assign(new Error("bad"), { name: "NativeError" }));
    expect(getErrorBoundaryFailures()).toBe(2);
    expect(errors).toHaveBeenCalledTimes(2);
  });

  it("logs only the error name and message, never render content", () => {
    const userText = "secret dictated sentence";
    reportErrorBoundary(new TypeError("boom"));
    const logged = errors.mock.calls.map((call) => String(call[0])).join("\n");
    expect(logged).toContain("TypeError");
    expect(logged).toContain("boom");
    expect(logged).not.toContain(userText);
    expect(logged).not.toContain("componentStack");
  });
});
