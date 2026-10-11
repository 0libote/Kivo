import { NATIVE_PROVIDERS } from "../platform/contracts.generated";
import type { AiProviderInfo } from "../types";

/** Offline metadata generated from the native serialization contract. */
export const FALLBACK_AI_PROVIDERS: AiProviderInfo[] = structuredClone([...NATIVE_PROVIDERS]);
