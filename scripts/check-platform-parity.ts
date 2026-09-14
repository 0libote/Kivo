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
  const start = source.indexOf(`fn ${name}`);
  if (start === -1) return "";
  const end = source.indexOf("\n}\n", start);
  return end === -1 ? "" : source.slice(start, end);
}

const configRs = read("src-tauri/src/config/mod.rs");
const typesTs = read("src/types.ts");
const shellRs = read("src-tauri/src/shell.rs");
const commandsRs = read("src-tauri/src/commands/mod.rs");
const nativeTs = read("src/platform/native.ts");

/** `HostPlatform::Macos => ShortcutBinding::new("...")` inside `fnName`. */
function rustDefault(fnName: string, host: "Macos" | "Windows"): string | null {
  const match = new RegExp(
    `${fnName}[\\s\\S]*?HostPlatform::${host} => ShortcutBinding::new\\("([^"]+)"\\)`,
  ).exec(configRs);
  return match?.[1] ?? null;
}

/** `key: platform === "macos" ? "a" : "b"` inside defaultSettings. */
function tsDefault(key: "dictationShortcut" | "writingShortcut"): [string, string] | null {
  const match = new RegExp(
    `${key}: platform === "macos" \\? "([^"]+)" : "([^"]+)"`,
  ).exec(typesTs);
  return match ? [match[1], match[2]] : null;
}

// --- 1. Shortcut defaults agree on both sides --------------------------------
const rustDictationMacos = rustDefault("dictation_default_for", "Macos");
const rustDictationWindows = rustDefault("dictation_default_for", "Windows");
const rustWritingMacos = rustDefault("writing_tools_default_for", "Macos");
const rustWritingWindows = rustDefault("writing_tools_default_for", "Windows");
const tsDictation = tsDefault("dictationShortcut");
const tsWriting = tsDefault("writingShortcut");

check("Rust dictation defaults parse", rustDictationMacos !== null && rustDictationWindows !== null, "dictation_default_for arms not found in config/mod.rs");
check("Rust writing defaults parse", rustWritingMacos !== null && rustWritingWindows !== null, "writing_tools_default_for arms not found in config/mod.rs");
check("TS dictation default parses", tsDictation !== null, "defaultSettings dictationShortcut not found in src/types.ts");
check("TS writing default parses", tsWriting !== null, "defaultSettings writingShortcut not found in src/types.ts");

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
}

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
  "harness marks input-monitoring unavailable off macOS",
  nativeTs.includes('this.platform === "macos" ? "not-determined" : "unavailable"'),
  "MockBridge input-monitoring availability drifted",
);
check(
  "harness requires speech recognition only on macOS",
  nativeTs.includes('required: this.platform === "macos"'),
  "MockBridge speech-recognition requirement drifted",
);

if (failures > 0) {
  console.error(`\n${failures} platform parity check(s) failed.`);
  process.exit(1);
}
console.info("\nAll platform parity checks passed.");
