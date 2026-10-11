import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import provenance from "../src/vendor/desert-ant/provenance.json";

// Refresh the exact upstream declarations without installing model WASM,
// native libraries, ML peers, or their dependency graph into every CI job.
const directory = await mkdtemp(join(tmpdir(), "kivo-sdk-types-"));
try {
  for (const [name, source] of Object.entries(provenance)) {
    const response = await fetch(source.url);
    if (!response.ok) throw new Error(`SDK download failed: ${response.status}`);
    const bytes = await response.arrayBuffer();
    if (
      `sha512-${new Bun.CryptoHasher("sha512").update(bytes).digest("base64")}` !== source.integrity
    )
      throw new Error(`Integrity mismatch for ${name}`);
    const archive = join(directory, `${name}.tgz`);
    await Bun.write(archive, bytes);
    const result = Bun.spawnSync([
      "tar",
      "-xzf",
      archive,
      "-C",
      directory,
      "package/index.d.ts",
      "package/LICENSE.md",
    ]);
    if (result.exitCode !== 0) throw new Error(result.stderr.toString());
    const declaration = (await readFile(join(directory, "package/index.d.ts"), "utf8")).replace(
      '"@desert-ant-labs/core"',
      '"./core"',
    );
    const license = await readFile(join(directory, "package/LICENSE.md"), "utf8");
    if (process.argv.includes("--check")) {
      if (
        declaration !== (await Bun.file(`src/vendor/desert-ant/${name}.d.ts`).text()) ||
        license !== (await Bun.file(`src/vendor/desert-ant/${name}.LICENSE.md`).text())
      )
        throw new Error(`Vendored ${name} types/notice drifted.`);
    } else {
      await Bun.write(`src/vendor/desert-ant/${name}.d.ts`, declaration);
      await Bun.write(`src/vendor/desert-ant/${name}.LICENSE.md`, license);
    }
  }
} finally {
  await rm(directory, { recursive: true, force: true });
}
