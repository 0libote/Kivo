/** Validate frontend/backend contracts and the Linux development bench. */
import assert from "node:assert/strict";
import { extractGenerated, renderGeneratedAiModels } from "./ai-model-codegen.ts";

const root = new URL("../", import.meta.url);
let failures = 0;

function reportFailure(name: string, detail: string) {
  failures += 1;
  console.error(`FAIL - ${name}: ${detail}`);
}

function check(name: string, condition: boolean, detail: string) {
  try {
    assert(condition, detail);
  } catch {
    reportFailure(name, detail);
    return;
  }
  console.info(`ok - ${name}`);
}

function read(relative: string): Promise<string> {
  return Bun.file(new URL(relative, root)).text();
}

/** Slice a top-level `fn name ... \n}\n` body so literal checks stay scoped. */
function fnBody(source: string, name: string): string {
  // Line-ending agnostic: Windows checkouts use CRLF, so a literal "\n}\n"
  // search never matches there (and the ubuntu-only run would not catch it).
  const start = source.indexOf(`fn ${name}`);
  if (start === -1) return "";
  const tail = source.slice(start);
  const end = tail.search(/\r?\n\}\r?\n/);
  return end === -1 ? "" : tail.slice(0, end);
}

const [
  configRs,
  typesTs,
  shellRs,
  commandsRs,
  nativeTs,
  aiRs,
  aiProvidersRs,
  aiModelsTs,
  vozRs,
  vozWorker,
  appTs,
  libRs,
] = await Promise.all([
  read("src-tauri/src/config/mod.rs"),
  read("src/types.ts"),
  read("src-tauri/src/shell.rs"),
  read("src-tauri/src/commands/mod.rs"),
  read("src/platform/native.ts"),
  read("src-tauri/src/ai/mod.rs"),
  read("src-tauri/src/ai/providers.rs"),
  read("src/ai/models.ts"),
  read("src-tauri/src/speech/voz.rs"),
  read("src/features/dictation/voz.worker.ts"),
  read("src/App.tsx"),
  read("src-tauri/src/lib.rs"),
]);

/** `HostPlatform::Windows => ShortcutBinding::new("...")` inside `fnName`. */
function rustDefault(fnName: string, host: "Windows" | "Linux"): string | null {
  const match = new RegExp(
    // Arms may share alternatives (`HostPlatform::Linux | ... => ...`) and
    // may break across lines, so allow anything up to the constructor.
    String.raw`${fnName}[\s\S]*?HostPlatform::${host}\b[^;]*?ShortcutBinding::new\("([^"]+)"\)`,
  ).exec(configRs);
  return match?.[1] ?? null;
}

/** Per-platform defaults out of the `*_SHORTCUTS` records in src/types.ts. */
function tsDefault(
  key: "DICTATION_SHORTCUTS" | "WRITING_SHORTCUTS",
): [windows: string, linux: string] | null {
  const anchor = typesTs.indexOf(`const ${key}`);
  if (anchor === -1) return null;
  const tail = typesTs.slice(anchor, anchor + 400);
  const value = (platform: "windows" | "linux"): string | null =>
    new RegExp(`${platform}: "([^"]+)"`).exec(tail)?.[1] ?? null;
  const windows = value("windows");
  const linux = value("linux");
  if (!windows || !linux) return null;
  return [windows, linux];
}

// --- 1. Shortcut defaults agree on both sides --------------------------------
const rustDictationWindows = rustDefault("dictation_default_for", "Windows");
const rustDictationLinux = rustDefault("dictation_default_for", "Linux");
const rustWritingWindows = rustDefault("writing_tools_default_for", "Windows");
const rustWritingLinux = rustDefault("writing_tools_default_for", "Linux");
const tsDictation = tsDefault("DICTATION_SHORTCUTS");
const tsWriting = tsDefault("WRITING_SHORTCUTS");

check(
  "Rust dictation defaults parse",
  rustDictationWindows !== null && rustDictationLinux !== null,
  "dictation_default_for arms not found in config/mod.rs",
);
check(
  "Rust writing defaults parse",
  rustWritingWindows !== null && rustWritingLinux !== null,
  "writing_tools_default_for arms not found in config/mod.rs",
);
check(
  "TS dictation default parses",
  tsDictation !== null,
  "DICTATION_SHORTCUTS record not found in src/types.ts",
);
check(
  "TS writing default parses",
  tsWriting !== null,
  "WRITING_SHORTCUTS record not found in src/types.ts",
);

if (tsDictation) {
  check(
    "dictation default matches on Windows",
    rustDictationWindows === tsDictation[0],
    `Rust "${rustDictationWindows}" vs TS "${tsDictation[0]}"`,
  );
  check(
    "dictation default matches on Linux",
    rustDictationLinux === tsDictation[1],
    `Rust "${rustDictationLinux}" vs TS "${tsDictation[1]}"`,
  );
}
if (tsWriting) {
  check(
    "writing default matches on Windows",
    rustWritingWindows === tsWriting[0],
    `Rust "${rustWritingWindows}" vs TS "${tsWriting[0]}"`,
  );
  check(
    "writing default matches on Linux",
    rustWritingLinux === tsWriting[1],
    `Rust "${rustWritingLinux}" vs TS "${tsWriting[1]}"`,
  );
}
check(
  "Linux dictation and writing defaults differ",
  rustDictationLinux !== rustWritingLinux && tsDictation?.[1] !== tsWriting?.[1],
  "Linux dictation and writing shortcuts must not share one accelerator",
);

// --- 2. Foreign-default migration covers both spellings -----------------------
const migration = fnBody(configRs, "foreign_default_replacement");
check(
  "migration helper exists",
  migration !== "",
  "foreign_default_replacement missing in config/mod.rs",
);
for (const literal of ['"Fn"', '"Ctrl+Meta"', '"Control+Super"']) {
  check(
    `migration handles ${literal}`,
    migration.includes(literal),
    `${literal} not found in foreign_default_replacement; a settings file carried across platforms would keep a dead shortcut`,
  );
}

// --- 3. Native-shortcut check matches the stored defaults ---------------------
const nativeCheck = fnBody(shellRs, "is_native_dictation_shortcut_for");
check(
  "native-shortcut helper exists",
  nativeCheck !== "",
  "is_native_dictation_shortcut_for missing in shell.rs",
);
check(
  "Windows native shortcut is the normalized Control+Super",
  nativeCheck.includes('(HostPlatform::Windows, "Control+Super")'),
  'expected (HostPlatform::Windows, "Control+Super"); shell.rs normalizes Ctrl→Control / Meta→Super before this check',
);

// --- 4. Permission requirements stay per-desktop -------------------------------
const permissionFn = fnBody(commandsRs, "permission_required_for");
check(
  "permission helper exists",
  permissionFn !== "",
  "permission_required_for missing in commands/mod.rs",
);
check(
  "accessibility stays required everywhere",
  /Accessibility => true/.test(permissionFn),
  "accessibility must gate text replacement on both desktops",
);
check(
  "microphone stays required everywhere",
  /Microphone => true/.test(permissionFn),
  "microphone must gate dictation on both desktops",
);
check(
  "optional legacy permissions do not gate Windows",
  permissionFn.includes("SpeechRecognition => false"),
  "speech recognition and input monitoring require no Windows consent prompt",
);
check(
  "harness detects the Linux bench",
  nativeTs.includes("navigator.userAgent") && /Linux\|X11/.test(nativeTs),
  "Linux user agents must select the development bench",
);

// --- 6. AI model default + suggestions + blocklist stay in sync ----------------
const rustDefaultModel = /DEFAULT_GEMINI_MODEL:\s*&str\s*=\s*"([^"]+)"/.exec(aiRs)?.[1] ?? null;
const tsDefaultModel = /DEFAULT_AI_MODEL\s*=\s*"([^"]+)"/.exec(typesTs)?.[1] ?? null;
check(
  "Rust default model parses",
  rustDefaultModel !== null,
  "DEFAULT_GEMINI_MODEL not found in ai/mod.rs",
);
check(
  "TS default model parses",
  tsDefaultModel !== null,
  "DEFAULT_AI_MODEL not found in src/types.ts",
);
if (rustDefaultModel && tsDefaultModel) {
  check(
    "AI default model matches",
    rustDefaultModel === tsDefaultModel,
    `Rust "${rustDefaultModel}" vs TS "${tsDefaultModel}"`,
  );
}
const allowlistBlock = aiRs.slice(
  aiRs.indexOf("SUPPORTED_GEMINI_MODELS"),
  aiRs.indexOf("];", aiRs.indexOf("SUPPORTED_GEMINI_MODELS")) + 2,
);
const allowlistIds = [...allowlistBlock.matchAll(/id:\s*"([^"]+)"/g)].map((m) => m[1]);
check(
  "suggestions block parses",
  allowlistIds.length > 0,
  "SUPPORTED_GEMINI_MODELS ids not found in ai/mod.rs",
);
for (const excluded of [
  "tts",
  "live",
  "audio",
  "-image",
  "banana",
  "transcribe",
  "embed",
  "veo-",
  "omni",
  "lyria-",
  "computer-use",
  "deep-research",
  "robotics",
]) {
  check(
    `suggestions exclude "${excluded}"`,
    allowlistIds.every((id) => !id.includes(excluded)),
    `"${excluded}"-like model id found in SUPPORTED_GEMINI_MODELS; only text models may be suggested`,
  );
}
check(
  "frontend suggestions mirror backend suggestions",
  generatedCatalogIsFresh().fallback,
  "src/ai/models.ts FALLBACK_ROWS drifted from SUPPORTED_GEMINI_MODELS; run `bun run generate:models`",
);
check(
  "frontend blocklist mirrors backend blocklist",
  generatedCatalogIsFresh().blocked,
  "src/ai/models.ts BLOCKED_AI_MODEL_PATTERNS drifted from BLOCKED_MODEL_SUBSTRINGS; run `bun run generate:models`",
);
/** Compare the checked-in mirror against a fresh render of the Rust source. */
function generatedCatalogIsFresh(): { fallback: boolean; blocked: boolean } {
  try {
    const generated = renderGeneratedAiModels(aiRs);
    return {
      fallback: extractGenerated(aiModelsTs, 0) === generated.fallbackRows,
      blocked: extractGenerated(aiModelsTs, 1) === generated.blockedPatterns,
    };
  } catch {
    return { fallback: false, blocked: false };
  }
}
check(
  "model queue is plumbed end to end",
  configRs.includes("pub models") &&
    commandsRs.includes("ai_models") &&
    typesTs.includes("aiModels"),
  "AiSettings.models / FrontendSettings.ai_models / AppSettings.aiModels missing",
);
check(
  "ordered failover tries each queued model",
  aiRs.includes("generate_in_order") &&
    aiRs.includes("is_failover_terminal") &&
    commandsRs.includes("summarize_link_in_order"),
  "generate_in_order / summarize_link_in_order / is_failover_terminal missing",
);
check(
  "native bridge exposes list_ai_models",
  nativeTs.includes("listAiModels") &&
    nativeTs.includes("list_ai_models") &&
    commandsRs.includes("list_ai_models"),
  "NativeBridge.listAiModels / list_ai_models command missing",
);
check(
  "provider choice is plumbed end to end",
  configRs.includes("pub provider") &&
    commandsRs.includes("ai_provider") &&
    typesTs.includes("aiProvider"),
  "AiSettings.provider / FrontendSettings.ai_provider / AppSettings.aiProvider missing",
);
check(
  "custom endpoint is plumbed end to end",
  configRs.includes("custom_base_url") &&
    commandsRs.includes("ai_custom_base_url") &&
    typesTs.includes("aiCustomBaseUrl"),
  "AiSettings.custom_base_url / FrontendSettings.ai_custom_base_url / AppSettings.aiCustomBaseUrl missing",
);
check(
  "provider metadata reaches the UI",
  nativeTs.includes("listAiProviders") &&
    nativeTs.includes("list_ai_providers") &&
    commandsRs.includes("list_ai_providers"),
  "NativeBridge.listAiProviders / list_ai_providers command missing",
);
check(
  "model costs reach the picker",
  aiProvidersRs.includes("cost_label_for") &&
    aiModelsTs.includes("per 1M") &&
    nativeTs.includes("AiModelInfo"),
  "pricing (cost_label_for / per-1M fallbacks) missing from the model pipeline",
);

// --- 7. Voz runtime parity and language safety --------------------------------
check(
  "Voz remains a third shared speech engine",
  configRs.includes("Voz,") && typesTs.includes('"voz"') && vozRs.includes("SpeechBackend::Voz"),
  "the persisted, frontend, and shared Rust speech-engine identifiers must all include Voz",
);
check(
  "Windows loads the pinned browser SDK on demand in the flow-bar worker",
  vozWorker.includes("/* @vite-ignore */") &&
    vozWorker.includes("@desert-ant-labs/voz@3.5.0/+esm") &&
    vozWorker.includes("@desert-ant-labs/ear@3.5.0/+esm") &&
    appTs.includes('context.surface !== "flow-bar"'),
  "Voz/Ear runtime URLs should remain version-pinned, runtime-only imports confined to the Windows flow-bar worker",
);
check(
  "Windows Voz worker validates message origin without rejecting WebView2 worker traffic",
  vozWorker.includes('event.origin !== "" && event.origin !== self.location.origin') &&
    vozWorker.includes("isVozWorkerRequest(event.data)"),
  "dedicated-worker messages may expose an empty origin in WebView2; reject every other foreign origin before validating the payload",
);
check(
  "Windows Voz readiness requires completed model loads",
  vozWorker.includes('const vozModelReadyPath = "/voz-model-ready"') &&
    vozWorker.includes('const languageCheckReadyPath = "/voz-language-check-ready"') &&
    vozWorker.includes("await setVozModelReady(true)") &&
    vozWorker.includes("vozInstalled !== undefined && languageCheckInstalled !== undefined"),
  "status must use explicit completion markers instead of treating a partial Desert Ant cache as a finished install",
);
check(
  "Windows verifies language before Voz inference",
  vozRs.includes("validate_detected_language") &&
    vozWorker.includes("ear.identify(samples, 16000)"),
  "Voz must only transcribe after a reliable supported-language match",
);
check(
  "Linux keeps Voz unavailable in the native harness",
  vozRs.includes("UnavailableVozRuntime") && libRs.includes("not(windows)"),
  "Linux must retain the simulated speech engine without claiming Voz support",
);

if (failures > 0) {
  console.error(`\n${failures} contract check(s) failed.`);
  process.exit(1);
}
console.info("\nAll application contract checks passed.");
