# mull

A keyboard-driven outline editor for Markdown, in the terminal, drawn as a
mind map. A file's headings are the map: you add, rename, reorder and
re-nest sections, and everything else in them (paragraphs, lists, code,
tables) travels along. Files stay plain Markdown, so they diff cleanly,
live happily in git, and render in anything that shows Markdown.

```
                ┌─ Getting there ─┬─ Flights
                │                 └─ Train from Madrid
Trip to Lisbon ─┼─ Where to stay ─┬─ Alfama
                │                 └─ Baixa
                │                ┌─ Tram 28
                └─ Things to do ─┼─ Pastéis de Belém
                                 └─ Sintra day trip
```

It's in the spirit of [h-m-m](https://github.com/nadrad/h-m-m), written in
Rust with [ratatui](https://ratatui.rs). The keys you can use right now are
always listed along the bottom of the screen.

## Install

```sh
cargo install mullmd        # from crates.io
cargo install --path .      # from a checkout
```

Either way the command is `mull`. The crate is called `mullmd` because
another `mull` (a plain-text wiki) already has that name on crates.io.

Each [GitHub release](https://github.com/bricef/mull/releases) also has
prebuilt binaries for Linux, macOS and Windows.

## Use

```sh
mull trip.md            # open a map, or start one that's saved there
mull --print trip.md    # draw the map as text and exit
mull --keys             # list every key binding
```

Start with `Tab` to add a sub-section and `Enter` to add a section after
the one you're on; both drop you straight into typing the heading. `Enter`
keeps the text, `Esc` throws it away. `b` hides the content blocks to show
only the outline of sections. `s` saves, `q` quits, `?` shows every key.

## Colour

mull is drawn in [Catppuccin Mocha](https://catppuccin.com/palette). Each
first-level section and everything under it gets its own accent, like the
branches of a hand-drawn mind map. The status line names the mode (MAP, EDIT,
HELP) in that mode's colour, and messages are coloured by what they mean:
green for saved, yellow for a key that did nothing, red for a failure.

It uses 24-bit colour when the terminal sets `COLORTERM=truecolor`, the
nearest of the standard 256 colours otherwise, and only bold and reverse video
when [`NO_COLOR`](https://no-color.org) is set.

## The file format

A new map is a heading, and the sections you add under it are headings too:

```markdown
# Trip to Lisbon

## Getting there

### Flights

### Train from Madrid

## Where to stay
```

Any Markdown file opens as a map. mull reads it with a real CommonMark
parser ([markdown-rs](https://github.com/wooorm/markdown-rs), with GitHub's
extensions and maths):

- **Headings are sections**, the nodes of the map: everything after a
  heading sits under it, up to the next heading of the same or a higher
  level. Sections nest up to six deep.
- **Everything else is a block**, a leaf of the map: each paragraph, list,
  code block, table, quote, HTML block, maths block, rule, and link or
  footnote definition, shown in a quieter style by its first line. A whole
  list is one block.
- **The root** is the file's one top-level heading, if it has exactly one
  with nothing above it. Otherwise it's the document, named after the file.
- **Front matter** is the file's metadata: kept, but not shown and not part
  of the map.

You type into headings. Blocks can be moved, cut, copied and pasted, but
mull doesn't edit them: that's what a Markdown editor is for. (Editing a
list's items in mull is planned: [#1](https://github.com/bricef/mull/issues/1).)

Saving writes the front matter back first, then each section as a heading
at the level its place in the tree gives it, and each block exactly as it
was. A file renders the same before and after mull saves it. What can change
is layout that doesn't affect rendering: blank lines, underlined headings
(written with `#`), closing `#`s and indented code (written fenced). mull
says so when you open a file it would reformat.

Two lists side by side stay two lists: since blank lines alone would merge
lists that share a marker, mull puts an empty `<!-- -->` comment between
them, which renders as nothing.

Some trees can't be written as Markdown and read back the same, such as a
block after a sub-section. mull refuses those moves and says why. Saves are atomic, go through
symlinks, and keep the file's permissions. Folds and hidden blocks are view
state and aren't saved.

## Keys

**Move**

| Keys | Action |
|---|---|
| `h` `←` | Go to the parent |
| `l` `→` | Go to a child, unfolding |
| `j` `↓` | Next topic at this level |
| `k` `↑` | Previous topic at this level |
| `g` `Home` | Go to the root |

**Sections**

| Keys | Action |
|---|---|
| `Tab` | Add a sub-section |
| `Enter` | Add a section after |
| `e` `i` | Edit the heading |
| `c` | Replace the heading |
| `d` `Del` | Cut, with everything under it |
| `y` | Copy, with everything under it |
| `p` | Paste as the last child |

**Structure**

| Keys | Action |
|---|---|
| `Space` | Fold or unfold |
| `b` | Show or hide content blocks |
| `J` | Swap with the sibling below |
| `K` | Swap with the sibling above |
| `H` | Move out, after the parent |
| `L` | Move in, under the sibling above |

**File**

| Keys | Action |
|---|---|
| `u` | Undo |
| `^r` `U` | Redo |
| `s` `^s` | Save |
| `q` `^c` | Quit (twice if unsaved) |
| `?` | Show all keys |

**While typing**

| Keys | Action |
|---|---|
| `Enter` | Keep the text; empty new ones go |
| `Esc` `^c` | Discard; drops a new topic |
| `←` `^b` | Cursor left |
| `→` `^f` | Cursor right |
| `Home` `^a` | Cursor to the start |
| `End` `^e` | Cursor to the end |
| `Bksp` | Delete before the cursor |
| `Del` | Delete under the cursor |
| `^w` | Delete the word before |
| `^u` | Delete back to the start |

**On this screen**

| Keys | Action |
|---|---|
| `j` `↓` | Scroll down |
| `k` `↑` | Scroll up |
| `Esc` `q` `?` | Close (any other key does too) |

## Licence

mull is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE)
or the [MIT licence](LICENSE-MIT), at your option. Unless you say otherwise,
any contribution you submit for inclusion in mull is dual-licensed the same
way, without any additional terms or conditions.

## Developing

```sh
just toolchain   # install the pinned Rust (rust-toolchain.toml)
just ci          # rustfmt, clippy and the tests: exactly what CI runs
just --list      # everything else
```

Releases are two commands, `just mint patch` and `just push-release`;
`docs/releasing.md` covers what happens next and the one-time setup.

`docs/design.md` explains the model, the layout, and the open questions.
The key tables above are checked against `mull --keys` by the tests, so
when you change a binding in `src/keymap.rs`, paste the new output here.
