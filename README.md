# mull

A keyboard-driven mind mapper for the terminal. Maps are plain Markdown
files, so they diff cleanly, live happily in git, and render in anything
that shows Markdown.

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

Start with `Tab` to add a child and `Enter` to add a sibling; both drop you
straight into typing. `Enter` keeps the text, `Esc` throws it away. `s`
saves, `q` quits, `?` shows every key.

## Colour

mull is drawn in [Catppuccin Mocha](https://catppuccin.com/palette). Each
first-level topic and everything under it gets its own accent, like the
branches of a hand-drawn mind map. The status line names the mode (MAP, EDIT,
HELP) in that mode's colour, and messages are coloured by what they mean:
green for saved, yellow for a key that did nothing, red for a failure.

It uses 24-bit colour when the terminal sets `COLORTERM=truecolor`, the
nearest of the standard 256 colours otherwise, and only bold and reverse video
when [`NO_COLOR`](https://no-color.org) is set.

## The file format

A new map is a heading with a list under it:

```markdown
# Trip to Lisbon

- Getting there
  - Flights
  - Train from Madrid
- Where to stay
```

That's the layout [Markmap](https://markmap.js.org) draws as a mind map too.

Any Markdown file opens as a map. mull reads it with a real CommonMark
parser ([markdown-rs](https://github.com/wooorm/markdown-rs), with GitHub's
extensions, front matter and maths):

- **Headings are sections:** everything after a heading sits under it, up
  to the next heading of the same or a higher level.
- **List items are topics,** nested as the lists are.
- **Everything else is content:** paragraphs, code, tables, quotes, HTML,
  maths, front matter, and link and footnote definitions each become one
  leaf on the map, shown in a quieter style with its first line.

You type into headings and list items. Content blocks can be moved, cut,
copied and pasted, but mull doesn't edit them: that's what a Markdown editor
is for.

Saving writes each node back as what it is: headings as headings, list items
with their own markers, numbers and task boxes, and content blocks exactly
as they were. A file renders the same before and after mull saves it. What
can change is layout that doesn't affect rendering: blank lines, underlined
headings (written with `#`), indented code (written fenced), and list
numbering. mull says so when you open a file it would reformat.

Some trees can't be written as Markdown and read back the same, such as
content after a sub-section, or a section inside a list item. mull refuses
those moves and says why. Saves are atomic, go through symlinks, and keep
the file's permissions. Folds are view state and aren't saved.

## Keys

**Move**

| Keys | Action |
|---|---|
| `h` `←` | Go to the parent |
| `l` `→` | Go to a child, unfolding |
| `j` `↓` | Next topic at this level |
| `k` `↑` | Previous topic at this level |
| `g` `Home` | Go to the root |

**Topics**

| Keys | Action |
|---|---|
| `Tab` | Add a child |
| `Enter` | Add a sibling below |
| `e` `i` | Edit the text |
| `c` | Replace the text |
| `d` `Del` | Cut, with everything under it |
| `y` | Copy, with everything under it |
| `p` | Paste as the last child |

**Structure**

| Keys | Action |
|---|---|
| `Space` | Fold or unfold |
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
| `Enter` | Keep the text |
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
