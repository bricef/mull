# mull

Terminal mind mapper (Rust, ratatui 0.30). Maps are Markdown files.

- Gate: `just ci` (fmt, clippy `-D warnings`, tests), the same recipe CI
  runs, on the toolchain pinned in `rust-toolchain.toml`. Run it before
  committing. Workflows only install just and call recipes; put new CI
  logic in the justfile, not in workflow YAML.
- Releases: `just mint <level>` then `just push-release` (see
  `docs/releasing.md`). Both are outward-facing: only when Brice asks.
- Design, format rules and open questions: `docs/design.md`. Read it before
  changing the file format, the layout or the keys.
- Keys are bound only in `src/keymap.rs` (`BINDINGS`); the hint bar
  (`HINTS`), help screen, `--keys` and README all derive from it. After
  changing a binding, replace the README key tables with `just keys`
  output (a test enforces this).
- Colours are named only in `src/theme.rs`; the UI asks for styles by
  role. Add any new role to `Theme::roles()` so the contrast test covers it.
- The Markdown round-trip, layout and session invariants are property
  tests in `tests/properties.rs`; keep them passing rather than narrowing
  the generators.
- Solo project: commit straight to `main`.
