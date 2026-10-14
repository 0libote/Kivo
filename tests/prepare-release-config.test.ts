import { describe, expect, it } from "bun:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("../scripts/prepare-release-config.ts", import.meta.url));
// The script resolves its output relative to its own location, i.e. the
// ignored build artifact normal releases overwrite via bun run prepare:release.
const output = fileURLToPath(
  new URL("../src-tauri/target/release-tauri.conf.json", import.meta.url),
);
// "Ed" algorithm identifier + 8-byte key number + 32-byte Ed25519 key.
const rawValid = Buffer.concat([Buffer.from("Ed"), Buffer.alloc(40, 7)]).toString("base64");
// The form `tauri signer generate` actually emits: outer base64 of the
// minisign .pub file text, whose last line inner-decodes to the 42-byte key.
const valid = Buffer.from(
  `untrusted comment: minisign public key: ABCDEF12\n${rawValid}\n`,
  "utf8",
).toString("base64");

function prepare(publicKey?: string) {
  const env = { ...process.env };
  if (publicKey === undefined) delete env.TAURI_UPDATER_PUBKEY;
  else env.TAURI_UPDATER_PUBKEY = publicKey;
  return Bun.spawnSync([process.execPath, script], { env });
}

const priorExists = await Bun.file(output).exists();
const priorSize = priorExists ? Bun.file(output).size : 0;

describe("release updater public key", () => {
  it("requires the secret", () => {
    const result = prepare(undefined);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr.toString()).toContain("TAURI_UPDATER_PUBKEY is required");
  });

  it("rejects non-minisign values", async () => {
    const truncatedFileKey = Buffer.from(
      `untrusted comment: minisign public key: ABCDEF12\n${rawValid.slice(0, 40)}\n`,
      "utf8",
    ).toString("base64");
    const wrongInnerSize = Buffer.from(
      `untrusted comment: minisign public key: ABCDEF12\n${Buffer.alloc(64).toString("base64")}\n`,
      "utf8",
    ).toString("base64");
    const invalid = [
      "not-a-key",
      "untrusted comment: minisign public key: DEADBEEF\nRW fake",
      rawValid.slice(0, 40),
      Buffer.alloc(64).toString("base64"),
      truncatedFileKey,
      wrongInnerSize,
    ];
    for (const publicKey of invalid) {
      const result = prepare(publicKey);
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr.toString()).toMatch(/minisign|42 bytes/);
    }
    expect(await Bun.file(output).exists()).toBe(priorExists);
    if (priorExists) expect(Bun.file(output).size).toBe(priorSize);
  });

  it("writes a minisign public key into the generated release config", async () => {
    const previous = (await Bun.file(output).exists()) ? await Bun.file(output).text() : null;
    try {
      for (const publicKey of [valid, rawValid]) {
        const result = prepare(publicKey);
        expect(result.exitCode).toBe(0);
        const config = await Bun.file(output).json();
        expect(config.plugins.updater.pubkey).toBe(publicKey);
      }
    } finally {
      if (previous === null)
        await Bun.file(output)
          .unlink()
          .catch(() => {});
      else await Bun.write(output, previous);
    }
  });
});
