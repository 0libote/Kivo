/** Validate Windows packaging, updater signatures, and shared build metadata. */
import { existsSync, readFileSync } from "node:fs";
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

const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
  version: string;
  scripts: Record<string, string>;
};
const version: string = packageJson.version;
check(
  "package.json version is numeric X.Y.Z",
  /^\d+\.\d+\.\d+$/.test(version),
  `got "${version}"; the NSIS artifact names embed this string`,
);

const windowsConf = JSON.parse(
  readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8"),
) as {
  bundle: { targets: string[]; windows: { nsis: { installMode: string; installerHooks: string } } };
};
check(
  "Windows ships a per-user .exe",
  windowsConf.bundle.targets.length === 1 &&
    windowsConf.bundle.targets[0] === "nsis" &&
    windowsConf.bundle.windows.nsis.installMode === "currentUser",
  "Windows must keep the NSIS .exe installer",
);
check(
  "Windows installer enforces 24H2",
  readFileSync(join(root, "packaging/windows/hooks.nsh"), "utf8").includes("${AtLeastBuild} 26100"),
  "installer OS floor is missing",
);

// --- Tauri config -----------------------------------------------------------
const tauriConf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")) as {
  productName: string;
  version: string;
  identifier: string;
  build: { beforeBuildCommand: string; frontendDist: string };
  bundle: {
    targets: unknown;
    icon?: string[];
    windows?: { digestAlgorithm?: string };
  };
  plugins: { updater: { endpoints: string[]; pubkey: string } };
};
check(
  "tauri.conf version matches package.json",
  tauriConf.version === version,
  `tauri.conf has "${tauriConf.version}"`,
);
check(
  "tauri.conf identifier is set",
  typeof tauriConf.identifier === "string" && tauriConf.identifier.length > 0,
  "identifier missing",
);
check(
  "Tauri packaging uses the frontend-only build",
  tauriConf.build.beforeBuildCommand === "bun run build:frontend" &&
    packageJson.scripts["build:frontend"] === "bun --bun vite build",
  "desktop packaging must not repeat the TypeScript gate that CI and bun run build already run",
);
check(
  "base bundle targets allow the Linux development bench",
  tauriConf.bundle.targets === "all",
  `targets is ${JSON.stringify(tauriConf.bundle.targets)}; Windows releases narrow targets to nsis (narrowed by tauri.windows.conf.json)`,
);
check(
  "updater endpoints include the rolling beta manifest",
  tauriConf.plugins.updater.endpoints.some((endpoint) =>
    endpoint.includes("continuous/continuous.json"),
  ),
  "continuous.json endpoint missing or misnamed; beta updates break",
);

// --- Windows floor: single NSIS installer --------------------------------------
// NSIS is the only Windows route (the legacy MSIX manifest/script were
// removed): one floor constant, enforced by the installer hooks.
const WINDOWS_FLOOR_BUILD = "26100"; // Windows 11 24H2: supported floor.

// --- Icons (both bundlers fail late when these are missing) ------------------
check(
  "bundle icons are configured for Windows",
  ["icons/icon.png", "icons/icon.ico"].every((icon) => tauriConf.bundle.icon?.includes(icon)),
  "existing icon files are not bundled unless bundle.icon lists them",
);
for (const icon of ["src-tauri/icons/icon.png", "src-tauri/icons/icon.ico"]) {
  check(`icon exists: ${icon}`, existsSync(join(root, icon)), "missing bundle icon");
}

// --- Rust package version -----------------------------------------------------
const cargoToml = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
check(
  "Cargo.toml version matches package.json",
  cargoVersion === version,
  `Cargo.toml has "${cargoVersion}"; the app version is stamped from package.json/tauri.conf`,
);

// --- Voz runtime packaging ----------------------------------------------------
const viteSource = readFileSync(join(root, "vite.config.ts"), "utf8");
check(
  "Windows-only Voz worker stays out of Linux bundles",
  viteSource.includes('process.env.TAURI_ENV_PLATFORM === "windows"') &&
    viteSource.includes("voz.worker.stub.ts"),
  "the Windows worker should not enter Linux frontend output",
);
const vozWorkerSource = readFileSync(join(root, "src/features/dictation/voz.worker.ts"), "utf8");
check(
  "Windows Voz ML runtime is downloaded instead of bundled",
  vozWorkerSource.includes("/* @vite-ignore */") &&
    vozWorkerSource.includes("@desert-ant-labs/voz@3.5.0/+esm") &&
    vozWorkerSource.includes("@desert-ant-labs/ear@3.5.0/+esm") &&
    vozWorkerSource.includes("onnxruntime-web@1.30.0/dist/ort.webgpu.bundle.min.mjs") &&
    vozWorkerSource.includes("@litertjs/core@2.5.3/+esm") &&
    !vozWorkerSource.includes('await import("@desert-ant-labs/voz")') &&
    !vozWorkerSource.includes('await import("@desert-ant-labs/ear")'),
  "Voz/Ear/ONNX/LiteRT must remain version-pinned remote imports so they do not inflate the NSIS installer",
);
const csp = (
  JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")) as {
    app: { security: { csp: string } };
  }
).app.security.csp;
const cspDirectives = new Map(
  csp
    .split(";")
    .map((directive) => directive.trim().split(/\s+/))
    .filter(([name]) => name)
    .map(([name, ...sources]) => [name, new Set(sources)] as const),
);
check(
  "Voz runtime CDN is allowed by the desktop CSP",
  cspDirectives.get("script-src")?.has("https://cdn.jsdelivr.net") === true &&
    cspDirectives.get("connect-src")?.has("https://cdn.jsdelivr.net") === true,
  "the on-demand Windows runtime cannot load unless jsDelivr is allowed for scripts and fetches",
);
// --- Capability windows match the windows the shell creates -------------------
// The bundler allows any label, so a renamed window label merges fine and
// then fails at runtime when show_surface cannot find the window.
const capabilities = JSON.parse(
  readFileSync(join(root, "src-tauri/capabilities/default.json"), "utf8"),
) as { windows: string[] };
const shellSource = readFileSync(join(root, "src-tauri/src/shell.rs"), "utf8");
const createdWindows = [...shellSource.matchAll(/build_window\(\s*app,\s*"([^"]+)"/g)].map(
  (match) => match[1],
);
check(
  "capabilities/default.json parses a window list",
  Array.isArray(capabilities.windows) && capabilities.windows.length > 0,
  "windows list missing",
);
// Set-diff instead of sorting: bare sort() compares UTF-16 code units, so
// ordering (and therefore the comparison) is locale-dependent.
const missingWindows = createdWindows.filter((window) => !capabilities.windows.includes(window));
const extraWindows = capabilities.windows.filter((window) => !createdWindows.includes(window));
check(
  "capability windows match the windows the shell creates",
  missingWindows.length === 0 && extraWindows.length === 0,
  `missing from capabilities: [${missingWindows}]; not created by shell.rs: [${extraWindows}]`,
);

const nsis = windowsConf.bundle.windows.nsis as typeof windowsConf.bundle.windows.nsis & {
  installerIcon?: string;
  headerImage?: string;
  sidebarImage?: string;
};
check(
  "Windows installer uses the Kivo icon",
  nsis.installerIcon === "icons/icon.ico",
  "installer must use the app icon",
);
for (const [key, width, height] of [
  ["headerImage", 150, 57],
  ["sidebarImage", 164, 314],
] as const) {
  const path = nsis[key];
  const file = path && join(root, "src-tauri", path);
  const bmp = file && existsSync(file) ? readFileSync(file) : undefined;
  check(
    `Windows ${key} has the NSIS bitmap dimensions`,
    Boolean(
      bmp?.subarray(0, 2).toString() === "BM" &&
        bmp.readInt32LE(18) === width &&
        bmp.readInt32LE(22) === height &&
        bmp.readUInt16LE(28) === 24,
    ),
    `expected a ${width} × ${height} 24-bit BMP`,
  );
}
// --- Windows floor consistency --------------------------------------------------
const hooksSource = readFileSync(join(root, "packaging/windows/hooks.nsh"), "utf8");
const hookBuild = /\$\{AtLeastBuild\}\s*(\d+)/.exec(hooksSource)?.[1];
check(
  "NSIS installer enforces the supported floor",
  hookBuild === WINDOWS_FLOOR_BUILD,
  `hooks.nsh enforces build ${hookBuild}, expected ${WINDOWS_FLOOR_BUILD} (Windows 11 24H2); raising it drops installed users, lowering it claims untested support`,
);

// --- Windows overlay config stays an overlay ------------------------------------
const windowsConfRaw = readFileSync(join(root, "src-tauri/tauri.windows.conf.json"), "utf8");
const windowsConfTop = JSON.parse(windowsConfRaw) as Record<string, unknown>;
check(
  "tauri.windows.conf.json does not fork identity",
  !("identifier" in windowsConfTop) &&
    !("productName" in windowsConfTop) &&
    !("version" in windowsConfTop),
  "the Windows overlay must only narrow bundle targets; identity/version stay in tauri.conf.json or releases fork",
);

// --- Updater key is injected at release time, never committed --------------------
check(
  "committed updater pubkey stays empty",
  tauriConf.plugins.updater.pubkey === "",
  "a pubkey in the repo would silently ship beta builds with the wrong update trust; prepare-release-config.ts injects it from secrets",
);

// --- Beta updater signatures must bind the manifest version ----------------------
// `tauri build` signs for the plain package version, but continuous.json
// announces `version+sha`. The updater plugin rejects that mismatch, so the
// beta jobs must re-sign with `--app-version version+sha` before publishing
// (build-continuous-manifest.ts verifies this at publish time).
const ciSource = readFileSync(join(root, ".github/workflows/ci.yml"), "utf8");
check(
  "beta updater archives are re-signed for version+sha",
  ciSource.includes("signer sign --app-version") &&
    ciSource.includes("Re-sign Windows updater archive for the beta version"),
  "ci.yml beta jobs must re-sign with --app-version version+sha or beta installs fail with SignedVersionMismatch",
);

// --- Artifact name parity (what CI uploads vs. what releases expect) ---------
console.info(
  `info - expected NSIS artifact: src-tauri/target/release/bundle/nsis/Kivo_${version}_x64-setup.exe`,
);

if (failures > 0) {
  console.error(`\n${failures} packaging check(s) failed.`);
  process.exit(1);
}
console.info("\nAll Windows packaging checks passed.");
