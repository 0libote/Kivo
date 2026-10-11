export {};

const assets = [...new Bun.Glob("assets/*").scanSync("dist")];
if (assets.length === 0) throw new Error("Build the frontend before checking its output.");
const forbidden = /(?:@desert-ant|onnxruntime|litert|koffi|voz-model-ready|voz\.worker)/;
const sizes = await Promise.all(
  assets.map(async (asset) => {
    const file = Bun.file(`dist/${asset}`);
    if (asset.endsWith(".js")) {
      const source = await file.text();
      if (
        /MockBridge|kivo-dev-settings|Simulated test engine|Development surfaces/.test(source) ||
        asset.includes("GalleryWindow")
      )
        throw new Error(`Development harness leaked into ${asset}`);
      if (forbidden.test(source)) throw new Error(`Retired ML runtime leaked into ${asset}`);
      return { js: file.size, css: 0 };
    }
    return { js: 0, css: asset.endsWith(".css") ? file.size : 0 };
  }),
);
const jsBytes = sizes.reduce((total, size) => total + size.js, 0);
const cssBytes = sizes.reduce((total, size) => total + size.css, 0);
const summary = `Frontend assets: JS ${(jsBytes / 1024).toFixed(1)} KiB; CSS ${(cssBytes / 1024).toFixed(1)} KiB. No browser ML runtime or development harness.\n`;
console.info(summary);
if (process.env.GITHUB_STEP_SUMMARY) {
  const path = process.env.GITHUB_STEP_SUMMARY;
  await Bun.write(path, (await Bun.file(path).text()) + summary);
}
