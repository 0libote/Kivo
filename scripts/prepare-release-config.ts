const publicKey = process.env.TAURI_UPDATER_PUBKEY?.trim();

if (!publicKey) {
  throw new Error("TAURI_UPDATER_PUBKEY is required for release builds.");
}

const outputPath = new URL("../src-tauri/target/release-tauri.conf.json", import.meta.url);
const config = {
  bundle: { createUpdaterArtifacts: true },
  plugins: { updater: { pubkey: publicKey } },
};

await Bun.write(outputPath, `${JSON.stringify(config, null, 2)}\n`);
console.info("Prepared the generated Tauri release configuration.");
