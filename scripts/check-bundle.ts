export {};

// The production harness builds a separate directory (KIVO_DIST=dist-browser);
// check whichever frontend output the caller built, defaulting to dist.
const outDir = process.env.KIVO_DIST ?? "dist";
const assets = [...new Bun.Glob("assets/*").scanSync(outDir)];
if (assets.length === 0) throw new Error("Build the frontend before checking its output.");
const forbidden = /(?:OrtSession|InferenceSession|WebAssembly\.instantiate|koffi)/;
const sizes = await Promise.all(
  assets.map(async (asset) => {
    const file = Bun.file(`${outDir}/${asset}`);
    if (asset.endsWith(".js")) {
      const source = await file.text();
      if (
        /MockBridge|kivo-dev-settings|Simulated test engine|Development surfaces/.test(source) ||
        asset.includes("GalleryWindow")
      )
        throw new Error(`Development harness leaked into ${asset}`);
      if (forbidden.test(source)) throw new Error(`Bundled ML implementation leaked into ${asset}`);
      return { js: file.size, css: 0 };
    }
    return { js: 0, css: asset.endsWith(".css") ? file.size : 0 };
  }),
);
const jsBytes = sizes.reduce((total, size) => total + size.js, 0);
const cssBytes = sizes.reduce((total, size) => total + size.css, 0);
const summary = `Frontend assets: JS ${(jsBytes / 1024).toFixed(1)} KiB; CSS ${(cssBytes / 1024).toFixed(1)} KiB. No bundled ML implementation or development harness; optional Voz loader downloads its runtime on demand.\n`;
console.info(summary);
if (process.env.GITHUB_STEP_SUMMARY) {
  const path = process.env.GITHUB_STEP_SUMMARY;
  await Bun.write(path, (await Bun.file(path).text()) + summary);
}
