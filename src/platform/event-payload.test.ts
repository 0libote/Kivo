import { describe, expect, it } from "bun:test";
import { isNativeEventPayload } from "./bridge";

describe("isNativeEventPayload", () => {
  it("accepts the null lifecycle signals, rejects anything else", () => {
    for (const event of ["show-about", "recovery-changed"] as const) {
      expect(isNativeEventPayload(event, null)).toBe(true);
      expect(isNativeEventPayload(event, undefined)).toBe(true);
      expect(isNativeEventPayload(event, false)).toBe(false);
      expect(isNativeEventPayload(event, {})).toBe(false);
    }
  });

  it("checks primitive payloads by type", () => {
    expect(isNativeEventPayload("pause-changed", true)).toBe(true);
    expect(isNativeEventPayload("pause-changed", "true")).toBe(false);
    expect(isNativeEventPayload("pause-changed", 1)).toBe(false);
    expect(isNativeEventPayload("dictation-level", 0.5)).toBe(true);
    expect(isNativeEventPayload("dictation-level", Number.NaN)).toBe(false);
    expect(isNativeEventPayload("dictation-level", "loud")).toBe(false);
  });

  it("requires arrays for list payloads", () => {
    for (const event of ["permission-status-changed", "local-models-changed"] as const) {
      expect(isNativeEventPayload(event, [])).toBe(true);
      expect(isNativeEventPayload(event, {})).toBe(false);
      expect(isNativeEventPayload(event, null)).toBe(false);
    }
  });

  it("requires non-null records for object payloads", () => {
    for (const event of [
      "dictation-state",
      "writing-context",
      "writing-error",
      "settings-changed",
      "local-model-progress",
      "voz-model-status",
      "voz-worker-request",
    ] as const) {
      expect(isNativeEventPayload(event, { status: "idle" })).toBe(true);
      expect(isNativeEventPayload(event, null)).toBe(false);
      expect(isNativeEventPayload(event, undefined)).toBe(false);
      expect(isNativeEventPayload(event, [])).toBe(false);
      expect(isNativeEventPayload(event, "idle")).toBe(false);
    }
  });
});
