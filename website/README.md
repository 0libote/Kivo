# Kivo website

Static landing page + docs. No build step, no dependencies, no framework — the
folder is deploy-ready as-is on Cloudflare Pages, GitHub Pages, Netlify, or
any other static host.

## Pages

- `index.html` — landing page (what Kivo is, features, privacy, platforms)
- `docs.html` — user guide (setup, dictation, Writing Tools, summaries, settings, troubleshooting)
- `dev.html` — developer guide (architecture, repo map, platform boundary, pipelines, conventions, releases)
- `styles.css`, `site.js` — shared theme, layout, and behavior

## Preview locally

```sh
# from the repo root
python3 -m http.server -d website 8000
# or
bunx serve website
```

Then open http://localhost:8000.

## Deploy to Cloudflare Pages

The site needs **no build command** — Cloudflare serves the folder directly.

**Option A — connected to Git (recommended):**

1. Go to the Cloudflare dashboard → **Workers & Pages** → **Create** → **Pages** → **Connect to Git**.
2. Select the `Kivo` repository.
3. Under **Build settings**, use:
   - Framework preset: `None`
   - Build command: *(leave empty)*
   - Build output directory: `website`
4. **Save and Deploy.** Every push to the connected branch redeploys automatically.

**Option B — direct upload with Wrangler:**

```sh
npx wrangler pages deploy website --project-name kivo
```

**Custom domain (either option):** Pages project → **Custom domains** → **Set up a custom domain**, then point your DNS at Cloudflare. The download CTAs link to the GitHub repo (`https://github.com/0libote/Kivo`), so no environment variables or redirects are needed.

## Design

Tokens mirror `src/styles/globals.css` (`--bg #f5f5f3`, `--accent #0869d7/#4c9cff`, radius 12/7, system font) with automatic light/dark via `prefers-color-scheme` plus a manual toggle persisted to `localStorage`. Layout takes cues from Whispr Flow: centered hero, quiet borders, generous whitespace, bento-ish feature grid.

## Keeping docs accurate

- Writing actions table ↔ `src/features/writing-tools/actions.ts`
- Model/endpoint/deadlines ↔ `src-tauri/src/ai/mod.rs` (`GEMINI_MODEL`, `store: false`, 200k-char summary cap)
- Surfaces/shortcuts/defaults ↔ `src/types.ts` (`defaultSettings`), `src-tauri/src/lib.rs`, `src-tauri/src/shell.rs`
- Permissions/platforms/packaging ↔ root `README.md`
