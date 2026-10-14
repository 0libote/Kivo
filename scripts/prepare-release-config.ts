// The Tauri updater verifies releases with minisign, so the configured public
// key must be a minisign public key: base64-encoded "Ed" algorithm identifier,
// 8-byte key number, and 32-byte Ed25519 key (42 bytes, 56 base64 characters).
// Anything else (a private key, the commented .pub file, a truncated secret)
// would only fail later at signing or update-verification time.
export function assertMinisignPublicKey(publicKey: string): void {
  let bytes: Buffer;
  try {
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(publicKey)) throw new Error("not base64");
    bytes = Buffer.from(publicKey, "base64");
  } catch {
    throw new Error("TAURI_UPDATER_PUBKEY must be a base64-encoded minisign public key.");
  }
  if (bytes.length !== 42) {
    throw new Error(
      `TAURI_UPDATER_PUBKEY must decode to 42 bytes (a minisign public key), got ${bytes.length}.`,
    );
  }
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
