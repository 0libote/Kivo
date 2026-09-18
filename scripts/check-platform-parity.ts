/**
 * Cross-platform parity gate: fails fast (seconds, any OS, no compile) when
 * the macOS and Windows branches drift apart.
 *
 * A recurring failure mode is a change built for one desktop that is never
 * adapted to the other: a shortcut default edited in `src/types.ts` but not
 * in `src-tauri/src/config/mod.rs` (or vice versa), a native-shortcut string
 * renamed in one place, or a permission requirement flipped for one OS.
 * The per-OS `cargo test` / `vitest` runs only execute their own host's
 * `#[cfg]` branch, and the full bundle builds only run on main/tags, so
 * without this check the breakage merges and fails late.
 *
 * Run: `bun run check:platform-parity`
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  extractGenerated,
  renderGeneratedAiModels,
} from "./ai-model-codegen.ts";

const root = fileURLToPath(new URL("..", import.meta.url));
let failures = 0;

function check(name: string, ok: boolean, detail: string) {
  if (ok) {
    console.info(`ok - ${name}`);
  } else {
    failures += 1;
    console.error(`FAIL - ${name}: ${detail}`);
  }
}

function read(relative: string): string {
  return readFileSync(join(root, relative), "utf8");
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

const configRs = read("src-tauri/src/config/mod.rs");
const typesTs = read("src/types.ts");
const shellRs = read("src-tauri/src/shell.rs");
const commandsRs = read("src-tauri/src/commands/mod.rs");
const nativeTs = read("src/platform/native.ts");
const aiRs = read("src-tauri/src/ai/mod.rs");
const aiProvidersRs = read("src-tauri/src/ai/providers.rs");
const aiModelsTs = read("src/ai/models.ts");

/** `HostPlatform::Macos => ShortcutBinding::new("...")` inside `fnName`. */
function rustDefault(fnName: string, host: "Macos" | "Windows" | "Linux"): string | null {
  const match = new RegExp(
    // Arms may share alternatives (`HostPlatform::Linux | ... => ...`) and
    // may break across lines, so allow anything up to the constructor.
    String.raw`${fnName}[\s\S]*?HostPlatform::${host}\b[^;]*?ShortcutBinding::new\("([^"]+)"\)`,
  ).exec(configRs);
  return match?.[1] ?? null;
}

/** Per-platform defaults out of the `*_SHORTCUTS` records in src/types.ts. */
function tsDefault(key: "DICTATION_SHORTCUTS" | "WRITING_SHORTCUTS"): [macos: string, windows: string, linux: string] | null {
  const anchor = typesTs.indexOf(`const ${key}`);
  if (anchor === -1) return null;
  const tail = typesTs.slice(anchor, anchor + 400);
  const value = (platform: "macos" | "windows" | "linux"): string | null =>
    new RegExp(`${platform}: "([^"]+)"`).exec(tail)?.[1] ?? null;
  const macos = value("macos");
  const windows = value("windows");
  const linux = value("linux");
  if (!macos || !windows || !linux) return null;
  return [macos, windows, linux];
}

// --- 1. Shortcut defaults agree on both sides --------------------------------
const rustDictationMacos = rustDefault("dictation_default_for", "Macos");
const rustDictationWindows = rustDefault("dictation_default_for", "Windows");
const rustDictationLinux = rustDefault("dictation_default_for", "Linux");
const rustWritingMacos = rustDefault("writing_tools_default_for", "Macos");
const rustWritingWindows = rustDefault("writing_tools_default_for", "Windows");
const rustWritingLinux = rustDefault("writing_tools_default_for", "Linux");
const tsDictation = tsDefault("DICTATION_SHORTCUTS");
const tsWriting = tsDefault("WRITING_SHORTCUTS");

check("Rust dictation defaults parse", rustDictationMacos !== null && rustDictationWindows !== null && rustDictationLinux !== null, "dictation_default_for arms not found in config/mod.rs");
check("Rust writing defaults parse", rustWritingMacos !== null && rustWritingWindows !== null && rustWritingLinux !== null, "writing_tools_default_for arms not found in config/mod.rs");
check("TS dictation default parses", tsDictation !== null, "DICTATION_SHORTCUTS record not found in src/types.ts");
check("TS writing default parses", tsWriting !== null, "WRITING_SHORTCUTS record not found in src/types.ts");

if (tsDictation) {
  check(
    "dictation default matches on macOS",
    rustDictationMacos === tsDictation[0],
    `Rust "${rustDictationMacos}" vs TS "${tsDictation[0]}"`,
  );
  check(
    "dictation default matches on Windows",
    rustDictationWindows === tsDictation[1],
    `Rust "${rustDictationWindows}" vs TS "${tsDictation[1]}"`,
  );
  check(
    "dictation default matches on Linux",
    rustDictationLinux === tsDictation[2],
    `Rust "${rustDictationLinux}" vs TS "${tsDictation[2]}"`,
  );
}
if (tsWriting) {
  check(
    "writing default matches on macOS",
    rustWritingMacos === tsWriting[0],
    `Rust "${rustWritingMacos}" vs TS "${tsWriting[0]}"`,
  );
  check(
    "writing default matches on Windows",
    rustWritingWindows === tsWriting[1],
    `Rust "${rustWritingWindows}" vs TS "${tsWriting[1]}"`,
  );
  check(
    "writing default matches on Linux",
    rustWritingLinux === tsWriting[2],
    `Rust "${rustWritingLinux}" vs TS "${tsWriting[2]}"`,
  );
}
check(
  "Linux dictation and writing defaults differ",
  rustDictationLinux !== rustWritingLinux && tsDictation?.[2] !== tsWriting?.[2],
  "Linux dictation and writing shortcuts must not share one accelerator",
);

// --- 2. Foreign-default migration covers both spellings -----------------------
const migration = fnBody(configRs, "foreign_default_replacement");
check("migration helper exists", migration !== "", "foreign_default_replacement missing in config/mod.rs");
for (const literal of ['"Fn"', '"Ctrl+Meta"', '"Control+Super"']) {
  check(
    `migration handles ${literal}`,
    migration.includes(literal),
    `${literal} not found in foreign_default_replacement; a settings file carried across platforms would keep a dead shortcut`,
  );
}

// --- 3. Native-shortcut check matches the stored defaults ---------------------
const nativeCheck = fnBody(shellRs, "is_native_dictation_shortcut_for");
check("native-shortcut helper exists", nativeCheck !== "", "is_native_dictation_shortcut_for missing in shell.rs");
check(
  "macOS native shortcut is Fn",
  nativeCheck.includes('(HostPlatform::Macos, "Fn")'),
  'expected (HostPlatform::Macos, "Fn") in is_native_dictation_shortcut_for',
);
check(
  "Windows native shortcut is the normalized Control+Super",
  nativeCheck.includes('(HostPlatform::Windows, "Control+Super")'),
  'expected (HostPlatform::Windows, "Control+Super"); shell.rs normalizes Ctrl→Control / Meta→Super before this check',
);

// --- 4. Permission requirements stay per-desktop -------------------------------
const permissionFn = fnBody(commandsRs, "permission_required_for");
check("permission helper exists", permissionFn !== "", "permission_required_for missing in commands/mod.rs");
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
  "input monitoring + speech recognition stay macOS-only",
  permissionFn.includes("InputMonitoring") &&
    permissionFn.includes("SpeechRecognition") &&
    permissionFn.includes("HostPlatform::Macos"),
  "input-monitoring / speech-recognition must be required only on macOS (desktop SAPI needs no prompt on Windows)",
);

// --- 5. Browser harness mirrors the same matrix --------------------------------
check(
  "harness derives platform from the user agent",
  nativeTs.includes("navigator.userAgent") && nativeTs.includes("detectedPlatform"),
  "native.ts must keep deriving its platform from navigator.userAgent so e2e UA spoofing covers both branches",
);
check(
  "harness detects the Linux bench instead of mislabeling it macOS",
  nativeTs.includes('"linux"') && /Linux\|X11/.test(nativeTs),
  'native.ts detectedPlatform must return "linux" for Linux user agents',
);
check(
  "harness marks input-monitoring unavailable off macOS",
  nativeTs.includes('this.platform === "macos" ? "not-determined" : "unavailable"'),
  "MockBridge input-monitoring availability drifted",
);
check(
  "harness requires speech recognition only on macOS",
  nativeTs.includes('required: this.platform === "macos"'),
  "MockBridge speech-recognition requirement drifted",
);

// --- 6. AI model default + suggestions + blocklist stay in sync ----------------
const rustDefaultModel = /DEFAULT_GEMINI_MODEL:\s*&str\s*=\s*"([^"]+)"/.exec(aiRs)?.[1] ?? null;
const tsDefaultModel = /DEFAULT_AI_MODEL\s*=\s*"([^"]+)"/.exec(typesTs)?.[1] ?? null;
check("Rust default model parses", rustDefaultModel !== null, "DEFAULT_GEMINI_MODEL not found in ai/mod.rs");
check("TS default model parses", tsDefaultModel !== null, "DEFAULT_AI_MODEL not found in src/types.ts");
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
const allowlistIds = [...allowlistBlock.matchAll(/id:\s*"([^"]+)"/g)].map(m => m[1]);
check("suggestions block parses", allowlistIds.length > 0, "SUPPORTED_GEMINI_MODELS ids not found in ai/mod.rs");
for (const excluded of ["tts", "live", "audio", "-image", "banana", "transcribe", "embed", "veo-", "omni", "lyria-", "computer-use", "deep-research", "robotics"]) {
  check(
    `suggestions exclude "${excluded}"`,
    allowlistIds.every(id => !id.includes(excluded)),
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
  configRs.includes("pub models") && commandsRs.includes("ai_models") && typesTs.includes("aiModels"),
  "AiSettings.models / FrontendSettings.ai_models / AppSettings.aiModels missing",
);
check(
  "ordered failover tries each queued model",
  aiRs.includes("generate_in_order") && aiRs.includes("is_failover_terminal") && commandsRs.includes("summarize_link_in_order"),
  "generate_in_order / summarize_link_in_order / is_failover_terminal missing",
);
check(
  "native bridge exposes list_ai_models",
  nativeTs.includes("listAiModels") && nativeTs.includes("list_ai_models") && commandsRs.includes("list_ai_models"),
  "NativeBridge.listAiModels / list_ai_models command missing",
);
check(
  "provider choice is plumbed end to end",
  configRs.includes("pub provider") && commandsRs.includes("ai_provider") && typesTs.includes("aiProvider"),
  "AiSettings.provider / FrontendSettings.ai_provider / AppSettings.aiProvider missing",
);
check(
  "custom endpoint is plumbed end to end",
  configRs.includes("custom_base_url") && commandsRs.includes("ai_custom_base_url") && typesTs.includes("aiCustomBaseUrl"),
  "AiSettings.custom_base_url / FrontendSettings.ai_custom_base_url / AppSettings.aiCustomBaseUrl missing",
);
check(
  "provider metadata reaches the UI",
  nativeTs.includes("listAiProviders") && nativeTs.includes("list_ai_providers") && commandsRs.includes("list_ai_providers"),
  "NativeBridge.listAiProviders / list_ai_providers command missing",
);
check(
  "model costs reach the picker",
  aiProvidersRs.includes("cost_label_for") && aiModelsTs.includes("per 1M") && nativeTs.includes("AiModelInfo"),
  "pricing (cost_label_for / per-1M fallbacks) missing from the model pipeline",
);

if (failures > 0) {
  console.error(`\n${failures} platform parity check(s) failed.`);
  process.exit(1);
}
console.info("\nAll platform parity checks passed.");
