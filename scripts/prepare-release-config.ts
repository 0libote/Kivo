// Tauri's updater pubkey is the base64 encoding of a minisign public key
// file: outer base64 decodes to UTF-8 text ("untrusted comment: ...\n<inner
// base64>\n"), and the last non-empty line inner-decodes to "Ed" + 8-byte key
// number + 32-byte Ed25519 key (42 bytes). A bare single-layer 42-byte key is
// also accepted for forwards compatibility, but `tauri signer generate`
// emits the file form.
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;

function decodeBase64Strict(value: string): Buffer | null {
  if (!BASE64_RE.test(value) || value.length % 4 !== 0) return null;
  try {
    const bytes = Buffer.from(value, "base64");
    // Buffer is lenient: reject non-canonical encodings.
    if (bytes.toString("base64") !== value) return null;
    return bytes;
  } catch {
    return null;
  }
}

export function assertMinisignPublicKey(publicKey: string): void {
  const invalid = "TAURI_UPDATER_PUBKEY must be a base64-encoded minisign public key file.";
  const outer = decodeBase64Strict(publicKey);
  if (!outer) throw new Error(invalid);
  // Tauri file form: outer decodes to text, last line decodes to 42 bytes.
  try {
    const text = outer.toString("utf8");
    if (!text.includes("\0")) {
      const lines = text
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean);
      if (lines.length >= 1) {
        const inner = decodeBase64Strict(lines[lines.length - 1]);
        if (inner?.length === 42 && inner.subarray(0, 2).toString("binary") === "Ed") {
          return;
        }
      }
    }
  } catch {
    // Fall through to the raw-key check below.
  }
  // Legacy raw form: outer directly decodes to the 42-byte key.
  if (outer.length === 42 && outer.subarray(0, 2).toString("binary") === "Ed") {
    return;
  }
  throw new Error(
    `TAURI_UPDATER_PUBKEY must decode to a minisign public key (file text ~100+ bytes whose last line decodes to 42 bytes starting with "Ed", or a raw 42-byte key), got outer ${outer.length} bytes.`,
  );
}

const publicKey = process.env.TAURI_UPDATER_PUBKEY?.trim();

if (!publicKey) {
  throw new Error("TAURI_UPDATER_PUBKEY is required for release builds.");
}

assertMinisignPublicKey(publicKey);

const outputPath = new URL("../src-tauri/target/release-tauri.conf.json", import.meta.url);
const config = {
  bundle: { createUpdaterArtifacts: true },
  plugins: { updater: { pubkey: publicKey } },
};

await Bun.write(outputPath, `${JSON.stringify(config, null, 2)}\n`);
console.info("Prepared the generated Tauri release configuration.");
