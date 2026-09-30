This is the crates.io `glib` 0.18.5 source with the upstream fix from
gtk-rs/gtk-rs-core PR #1343 applied to `src/variant_iter.rs`: pass `&mut p`
to `g_variant_get_child` because it writes through that variadic out-parameter.

The Tauri GTK dependency stack currently requires glib 0.18, while the
published upstream fix is only available in glib 0.20. Remove this local
patch once that stack can use a fixed upstream release.

Upstream change: https://github.com/gtk-rs/gtk-rs-core/pull/1343
Advisory: https://rustsec.org/advisories/RUSTSEC-2024-0429.html
