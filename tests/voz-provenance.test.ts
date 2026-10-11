import { expect, test } from "bun:test";
import provenance from "../src/vendor/desert-ant/provenance.json";

test("vendored speech declarations match their pinned provenance", async () => {
  for (const [name, artifact] of Object.entries(provenance)) {
    const bytes = await Bun.file(`src/vendor/desert-ant/${name}.d.ts`).arrayBuffer();
    expect(new Bun.CryptoHasher("sha256").update(bytes).digest("hex")).toBe(
      artifact.declarationSha256,
    );
    expect(artifact.url).toEndWith(`-${artifact.version}.tgz`);
  }
});
