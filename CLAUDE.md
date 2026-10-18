# mull

Terminal mind mapper (Rust, ratatui 0.30). Maps are Markdown files. The
crate is `mullmd` on crates.io; the command and the library are `mull`.

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
- The map's model is in `docs/design.md` ("The model", "The file"): nodes
  have kinds, and `tree::Node::check` holds the rules that keep a tree
  writable as Markdown. Every change in `app.rs` goes through `settle` or a
  check; new rules go in `check` with a test. `markdown.rs` works around
  markdown-rs 1.0.0 bugs (listed in the design doc); don't remove one
  without its test going green some other way.
- The kitchen-sink fixture (`tests/kitchen_sink.rs`) must render the same
  before and after saving.
- Colours are named only in `src/theme.rs`; the UI asks for styles by
  role. Add any new role to `Theme::roles()` so the contrast test covers it.
- The Markdown round-trip, layout and session invariants are property
  tests in `tests/properties.rs`; keep them passing rather than narrowing
  the generators.
- Solo project: commit straight to `main`.
