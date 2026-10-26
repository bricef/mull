# mull: objectives

A keyboard-driven terminal outline editor for Markdown, drawn as a mind map. Headings are the map, and everything else travels with its section. It is published for anyone as the crate `mullmd`.

## Status

paused: v0.2.0 shipped 2026-10-18 and includes all core functions. The repo plans no next release.

## Done when

The file format and keys have settled: the open questions in `docs/design.md` are decided, which it calls "the decisions worth making before the format and the keys settle". After that, a map saved by one version opens the same in every later version.

## Milestones

- [x] A map is plain Markdown that saving leaves rendering the same: the kitchen-sink fixture renders identical HTML before and after a save (1a01e70, 2026-10-18)
- [x] Anyone can install it: crates.io `mullmd` and prebuilt binaries for 5 targets, v0.1.1 released 2026-10-18
- [x] Sections are the structure and everything else is a move-only block: v0.2.0 released 2026-10-18 (9cdf2fc)
- [ ] List items can be edited as topics that look distinct from sections: issue #1 (open), planned after the sections-only structure, which shipped in v0.2.0  ← next
- [ ] The format and keys have settled: `docs/design.md` open questions 2–6 decided (persisting folds, radial layout, system clipboard, search/export/mouse, more themes)
- [ ] A release stores no long-lived crates.io token: trusted publishing (`docs/releasing.md`, one-time setup step 5). As of 2026-10-26, `CARGO_REGISTRY_TOKEN` is still set in the `release` environment and `CRATES_IO_TRUSTED_PUBLISHING` is not set

## Not goals

- Editing content blocks: "that's what a Markdown editor is for" (`docs/design.md`, "The model")
- Turning list items into sections; they stay inside their section (issue #1)
- Keeping layout that renders the same (blank lines, setext headings, indented code); only the rendering is kept (`docs/design.md`, "The file")
