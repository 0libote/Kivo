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

  it("does not log private text from error details or extra fields", () => {
    const userText = "secret dictated sentence";
    reportErrorBoundary(
      Object.assign(new TypeError(userText), {
        name: userText,
        stack: userText,
        componentStack: userText,
      }),
    );
    expect(errors).toHaveBeenCalledWith("Kivo window failed.");
    const logged = JSON.stringify(errors.mock.calls);
    expect(logged).not.toContain(userText);
    expect(logged).not.toContain("componentStack");
  });
});
