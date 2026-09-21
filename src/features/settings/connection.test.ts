// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { testFailureConnection } from "./SettingsWindow";

describe("testFailureConnection", () => {
  it("maps key problems to invalid", () => {
    for (const code of ["invalid_api_key", "credential", "ai_not_configured"]) {
      expect(testFailureConnection(code)).toBe("invalid");
    }
  });

  it("maps retired models to the model state, never to invalid", () => {
    for (const code of ["model_unavailable", "model_not_found"]) {
      expect(testFailureConnection(code)).toBe("model");
    }
  });

  it("maps quota exhaustion to rate-limited", () => {
    expect(testFailureConnection("rate_limited")).toBe("rate-limited");
  });

  it("maps provider account and policy rejections to blocked", () => {
    for (const code of ["region_unavailable", "account_disabled", "provider_forbidden"]) {
      expect(testFailureConnection(code)).toBe("blocked");
    }
  });

  it("maps reachability and shape problems to offline", () => {
    for (const code of ["transport", "invalid_response", "api_error", "incomplete", "empty_response"]) {
      expect(testFailureConnection(code)).toBe("offline");
    }
  });

  it("falls back to untested for unknown codes, never testing", () => {
    for (const code of ["", "unknown", "no_result", "invalid_link", "settings_runtime"]) {
      expect(testFailureConnection(code)).toBe("untested");
    }
  });
});
