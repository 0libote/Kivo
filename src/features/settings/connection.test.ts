import { describe, expect, it } from "bun:test";
import type { ApiKeyStatus } from "../../types";
import {
  API_STATUS_UNAVAILABLE_NOTICE,
  apiStatusAfterModelSave,
  PROVIDER_LIST_FALLBACK_NOTICE,
  testFailureConnection,
} from "./connection";

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

  it("maps provider and response errors to blocked", () => {
    for (const code of [
      "region_unavailable",
      "account_disabled",
      "provider_forbidden",
      "provider_rejected",
      "api_error",
      "invalid_response",
      "incomplete",
      "empty_response",
    ]) {
      expect(testFailureConnection(code)).toBe("blocked");
    }
  });

  it("maps only transport failures to offline", () => {
    expect(testFailureConnection("transport")).toBe("offline");
  });

  it("falls back to untested for unknown codes, never testing", () => {
    for (const code of ["", "unknown", "no_result", "invalid_link", "settings_runtime"]) {
      expect(testFailureConnection(code)).toBe("untested");
    }
  });
});

describe("apiStatusAfterModelSave", () => {
  const connected: ApiKeyStatus = { configured: true, connection: "connected" };

  it("marks the connection untested when the queue persisted", () => {
    expect(apiStatusAfterModelSave(connected, true)).toEqual({
      configured: true,
      connection: "untested",
    });
  });

  it("leaves a failed save exactly as it was, never untested", () => {
    expect(apiStatusAfterModelSave(connected, false)).toBe(connected);
  });
});

describe("settings failure notices", () => {
  it("uses stable user-facing copy", () => {
    expect(API_STATUS_UNAVAILABLE_NOTICE).toBe("Saved key status isn’t available right now.");
    expect(PROVIDER_LIST_FALLBACK_NOTICE).toBe(
      "Provider list isn’t available right now. Showing the saved list.",
    );
  });
});
