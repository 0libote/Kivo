export {};

const assets = [...new Bun.Glob("assets/*").scanSync("dist")];
const forbidden = /(?:desert-ant|onnxruntime|litert|koffi|voz-model-ready)/;
let jsBytes = 0;
let cssBytes = 0;
for (const asset of assets) {
  const file = Bun.file(`dist/${asset}`);
  if (asset.endsWith(".js")) {
    jsBytes += file.size;
    if (
      /MockBridge|kivo-dev-settings|Simulated test engine|Development surfaces/.test(
        await file.text(),
      ) ||
      asset.includes("GalleryWindow")
    )
      throw new Error(`Development harness leaked into ${asset}`);
    // Runtime URLs are expected only in the Windows worker.
    if (!asset.includes("voz.worker") && forbidden.test(await file.text()))
      throw new Error(`ML runtime leaked into ${asset}`);
  }
  if (asset.endsWith(".css")) cssBytes += file.size;
}
if (assets.length === 0) throw new Error("Build the frontend before checking its output.");
const summary = `Frontend assets: JS ${(jsBytes / 1024).toFixed(1)} KiB; CSS ${(cssBytes / 1024).toFixed(1)} KiB. No bundled ML runtime.\n`;
console.info(summary);
if (process.env.GITHUB_STEP_SUMMARY) {
  const path = process.env.GITHUB_STEP_SUMMARY;
  await Bun.write(path, (await Bun.file(path).text()) + summary);
}
