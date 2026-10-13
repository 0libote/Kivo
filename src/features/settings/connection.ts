import type { ApiKeyStatus } from "../../types";

/** User-facing notices for settings persistence failures. Single-sourced so
 * the initial load and later retries report failures with identical copy. */
export const API_STATUS_UNAVAILABLE_NOTICE = "Saved key status isn’t available right now.";
export const PROVIDER_LIST_FALLBACK_NOTICE =
  "Provider list isn’t available right now. Showing the saved list.";

/**
 * Next connection state after a model-queue save. SettingsWindow's save
 * reports success instead of rejecting, so callers must only mark the
 * connection untested when the queue actually persisted: the old code chained
 * `.then(mark untested).catch(swallow)`, which marked untested even when the
 * save failed (save never rejects — it notices internally and resolves).
 * Kept free of React/StyleX so unit tests can import it without the StyleX
 * compiler.
 */
export function apiStatusAfterModelSave(current: ApiKeyStatus, saved: boolean): ApiKeyStatus {
  if (!saved) return current;
  return { ...current, connection: "untested" };
}

/** Map a failed Test connection to a terminal indicator state so the UI
 * never sticks at "testing". Mirrors the native CommandError codes
 * (see GeminiError::code in src-tauri/src/ai/mod.rs and AppCoreError::code
 * in src-tauri/src/commands/mod.rs). Kept free of React/StyleX so unit tests
 * can import it without the StyleX compiler. */
export function testFailureConnection(code: string): ApiKeyStatus["connection"] {
  if (code === "invalid_api_key" || code === "credential" || code === "ai_not_configured")
    return "invalid";
  if (code === "model_unavailable" || code === "model_not_found") return "model";
  if (code === "rate_limited") return "rate-limited";
  if (
    code === "region_unavailable" ||
    code === "account_disabled" ||
    code === "provider_forbidden" ||
    code === "provider_rejected" ||
    code === "api_error" ||
    code === "invalid_response" ||
    code === "incomplete" ||
    code === "empty_response"
  )
    return "blocked";
  if (code === "transport") return "offline";
  // insufficient_credits (empty Zen balance) keeps the neutral state: the
  // notice text carries the top-up guidance, not the indicator.
  return "untested";
}
