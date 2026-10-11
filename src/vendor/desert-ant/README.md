# Browser speech SDK declarations

These are the full upstream public `index.d.ts` declarations from the exact
3.5.0 Ear, Voz, and Core npm artifacts. The sole transformation is Ear's
Core type import becoming `./core`. Copyright and license notices are kept
beside each declaration. No SDK executable/model files are vendored.

`provenance.json` records the registry tarball integrity from the former Bun
lockfile and the checked-in declaration hashes. `bun run vendor:voz-types
--check` verifies against those exact artifacts without installing the SDKs
or their large optional runtime dependencies. Use that command when updating
the runtime, and update versions, integrity, declaration hashes, worker URLs,
and the versioned cache together. Ordinary checks verify local hashes offline.

The Windows worker fetches the runtime explicitly. These declarations do not
make remote JavaScript part of Bun's installation integrity chain. jsDelivr,
the SDK's WASM/model assets, WebView2, CSP, and offline behavior still need the
Windows acceptance tests in docs/releasing.md. License notices remain alongside the declarations; Desert Ant Labs credits
are linked from the model setup UI.
