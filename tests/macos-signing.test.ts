import { afterAll, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";

const directory = mkdtempSync(join(tmpdir(), "kivo-signing-"));
const keys = [
  "APPLE_CERTIFICATE",
  "APPLE_CERTIFICATE_PASSWORD",
  "APPLE_ID",
  "APPLE_PASSWORD",
  "APPLE_TEAM_ID",
  "APPLE_SIGNING_IDENTITY",
] as const;
const mock = join(directory, "bun");
// The production wrapper runs on macOS; Linux can verify its shell behavior too.
const shellTest = test.skipIf(process.platform === "win32");
await Bun.write(
  mock,
  `#!/bin/bash
for key in ${keys.join(" ")}; do
  printf '%s\\n' "\${!key-unset}"
done
printf '%s\\n' "$@"
exit "\${KIVO_TEST_STATUS:-0}"
`,
);
chmodSync(mock, 0o755);
afterAll(() => rmSync(directory, { recursive: true, force: true }));

async function build(credentials: Record<string, string>, status = "0") {
  const env = { ...process.env };
  for (const key of keys) delete env[key];
  const child = Bun.spawn(
    ["bash", "scripts/build-macos.sh", "build", "--config", "path with spaces.json"],
    {
      env: {
        ...env,
        ...credentials,
        PATH: `${directory}${delimiter}${process.env.PATH}`,
        KIVO_TEST_STATUS: status,
      },
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const output = (await new Response(child.stdout).text()).trimEnd().split("\n");
  return { output, status: await child.exited };
}

shellTest(
  "missing and empty Apple secrets use ad-hoc signing without certificate import or notarization",
  async () => {
    for (const credentials of [{}, Object.fromEntries(keys.map((key) => [key, ""]))]) {
      const result = await build(credentials);
      expect(result.status).toBe(0);
      expect(result.output).toEqual([
        "unset",
        "unset",
        "unset",
        "unset",
        "unset",
        "-",
        "tauri",
        "build",
        "--config",
        "path with spaces.json",
      ]);
    }
  },
);

shellTest("configured Apple credentials pass through unchanged", async () => {
  const values = [
    "certificate",
    "password with spaces",
    "apple-id",
    "apple-password",
    "team",
    "Developer ID Application: Kivo",
  ];
  const result = await build(Object.fromEntries(keys.map((key, index) => [key, values[index]])));
  expect(result.status).toBe(0);
  expect(result.output.slice(0, keys.length)).toEqual(values);
});

shellTest(
  "empty notarization credentials stay unset with a configured signing certificate",
  async () => {
    const result = await build({
      APPLE_CERTIFICATE: "certificate",
      APPLE_CERTIFICATE_PASSWORD: "password",
      APPLE_SIGNING_IDENTITY: "Developer ID Application: Kivo",
      APPLE_ID: "",
      APPLE_PASSWORD: "",
      APPLE_TEAM_ID: "",
    });
    expect(result.output.slice(0, keys.length)).toEqual([
      "certificate",
      "password",
      "unset",
      "unset",
      "unset",
      "Developer ID Application: Kivo",
    ]);
  },
);

shellTest("macOS build failures propagate to the workflow", async () => {
  expect((await build({}, "42")).status).toBe(42);
});
