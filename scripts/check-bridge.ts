/**
 * Bridge contract gate: fails fast (seconds, any OS, no compile) when the
 * frontend Tauri bridge and the Rust commands drift apart.
 *
 * The most common AI breakage is a rename on one side only: an
 * `invoke("...")` in `src/platform/native.ts` with no matching
 * `#[tauri::command]` in Rust (runtime "command not found"), a command
 * defined but never registered in `lib.rs generate_handler!` (same symptom),
 * an event the UI listens for that Rust never emits (dead UI), or an
 * `AppSettings` field with no `FrontendSettings` counterpart (silently
 * dropped preference). Unit tests on either side cannot see these; only a
 * cross-side check can.
 *
 * Run: `bun run check:bridge`
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
let failures = 0;

function pass(name: string) {
  console.info(`ok - ${name}`);
}

function fail(name: string, detail: string) {
  failures += 1;
  console.error(`FAIL - ${name}: ${detail}`);
}

function read(relative: string): string {
  return readFileSync(join(root, relative), "utf8");
}

/** Deterministic display order for the per-item checks below. */
function byName(a: string, b: string): number {
  return a.localeCompare(b);
}

const nativeTs = read("src/platform/native.ts");
const typesTs = read("src/types.ts");
const commandsRs = read("src-tauri/src/commands/mod.rs");
const shellRs = read("src-tauri/src/shell.rs");
const libRs = read("src-tauri/src/lib.rs");

// --- 1. Every invoked command exists in Rust --------------------------------
// `call<T>("name")`, including generics with `>` inside like
// `call<Omit<AppContext, "surface">>("get_app_context")`. Type arguments never
// contain parens, so `[^()]*` reaches the opening `(` without backtracking.
const invoked = new Set(
  [...nativeTs.matchAll(/call<[^()]*>\(\s*"([a-z_]+)"/g)].map((m) => m[1]),
);
if (invoked.size > 0) {
  pass("frontend invokes at least one command");
} else {
  fail("frontend invokes at least one command", "invoke() call sites not found in native.ts");
}

const rustSources = `${commandsRs}\n${shellRs}`;
const defined = new Set(
  [...rustSources.matchAll(/#\[tauri::command\]\s*\n\s*pub (?:async )?fn ([a-z_]+)/g)].map(
    (m) => m[1],
  ),
);
if (defined.size > 0) {
  pass("Rust defines at least one command");
} else {
  fail("Rust defines at least one command", "#[tauri::command] fns not found");
}

for (const command of [...invoked].sort(byName)) {
  if (defined.has(command)) {
    pass(`command exists: ${command}`);
  } else {
    fail(
      `command exists: ${command}`,
      `native.ts invokes "${command}" but no #[tauri::command] fn ${command} exists; add it or fix the invoke name`,
    );
  }
}

// --- 2. Every invoked command is registered in generate_handler! -------------
const handlerBlock = libRs.slice(
  libRs.indexOf("generate_handler!"),
  libRs.indexOf("])", libRs.indexOf("generate_handler!")) + 2,
);
const registered = new Set(
  [...handlerBlock.matchAll(/(?:commands|crate::shell)::([a-z_]+)/g)].map((m) => m[1]),
);
for (const command of [...invoked].sort(byName)) {
  if (registered.has(command)) {
    pass(`command registered: ${command}`);
  } else {
    fail(
      `command registered: ${command}`,
      `"${command}" is invoked but missing from lib.rs generate_handler!; the app fails at runtime with "command not found"`,
    );
  }
}

// --- 3. No dead registrations -------------------------------------------------
for (const command of [...registered].sort(byName)) {
  if (defined.has(command)) {
    pass(`registration live: ${command}`);
  } else {
    fail(
      `registration live: ${command}`,
      `lib.rs registers "${command}" but no #[tauri::command] fn exists; remove it or restore the fn`,
    );
  }
}

// --- 4. Every listened event is emitted by Rust --------------------------------
const eventBlockMatch = /type NativeEventMap = \{([\s\S]*?)\n\};/.exec(nativeTs);
const events = new Set(
  eventBlockMatch
    ? [...eventBlockMatch[1].matchAll(/"([a-z-]+)":/g)].map((m) => m[1])
    : [],
);
if (events.size > 0) {
  pass("event map parses");
} else {
  fail("event map parses", "NativeEventMap keys not found in native.ts");
}
// Rust emits via `emit("e", ..)` or window-targeted `emit_to("win", "e", ..)`.
// An event counts as emitted when its quoted name appears in the same
// statement as an emit call (plain substring search, no regex backtracking).
const emitStatements: string[] = [];
for (const m of rustSources.matchAll(/\bemit\w*\(/g)) {
  const start = m.index ?? 0;
  emitStatements.push(rustSources.slice(start, rustSources.indexOf(";", start)));
}
for (const event of [...events].sort(byName)) {
  if (emitStatements.some((stmt) => stmt.includes(`"${event}"`))) {
    pass(`event emitted: ${event}`);
  } else {
    fail(
      `event emitted: ${event}`,
      `UI listens for "${event}" but Rust never emit()s it; the listener is dead code or the emit was renamed`,
    );
  }
}

// --- 5. Settings fields survive the Rust boundary ------------------------------
// TS `AppSettings` is flat camelCase; Rust `FrontendSettings` is flat
// snake_case. A field renamed on one side only is silently dropped by serde.
// rustfmt always formats fields as `pub name: Type`, so a plain substring
// search is exact enough.
const appSettingsBlock = typesTs.slice(
  typesTs.indexOf("interface AppSettings"),
  typesTs.indexOf("}", typesTs.indexOf("interface AppSettings")) + 1,
);
const tsFields = [...appSettingsBlock.matchAll(/^\s*([a-zA-Z]+)[?]?:/gm)].map((m) => m[1]);
const frontendBlock = commandsRs.slice(
  commandsRs.indexOf("pub struct FrontendSettings"),
  commandsRs.indexOf("}", commandsRs.indexOf("pub struct FrontendSettings")) + 1,
);
const camelToSnake = (key: string) =>
  key.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
if (tsFields.length > 0) {
  pass("TS AppSettings parses");
} else {
  fail("TS AppSettings parses", "interface AppSettings fields not found");
}
for (const field of tsFields) {
  const rustField = camelToSnake(field);
  if (frontendBlock.includes(`pub ${rustField}:`)) {
    pass(`setting plumbed: ${field}`);
  } else {
    fail(
      `setting plumbed: ${field}`,
      `AppSettings.${field} has no FrontendSettings.${rustField}; the preference is dropped at the bridge`,
    );
  }
}

if (failures > 0) {
  console.error(`\n${failures} bridge contract check(s) failed.`);
  process.exit(1);
}
console.info("\nAll bridge contract checks passed.");
