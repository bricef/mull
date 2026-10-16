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
cargo install --path .
```

## Use

```sh
mull trip.md            # open a map, or start one that's saved there
mull --print trip.md    # draw the map as text and exit
mull --keys             # list every key binding
```

Start with `Tab` to add a child and `Enter` to add a sibling; both drop you
straight into typing. `Enter` keeps the text, `Esc` throws it away. `s`
saves, `q` quits, `?` shows every key.

## The file format

mull writes the root as a heading and everything else as a nested list:

```markdown
# Trip to Lisbon

- Getting there
  - Flights
  - Train from Madrid
- Where to stay
```

That's the layout [Markmap](https://markmap.js.org) draws as a mind map too.

It reads more than it writes, so existing notes open as maps:

- Headings nest by level, and lists and paragraphs nest under the heading
  above them.
- Indentation nests list items, and plain indented lines (h-m-m's
  tab-indented files, say) nest the same way.
- List markers (`-`, `*`, `+`, `1.`) are dropped from the topic text. Other
  inline Markdown (`**bold**`, `[links](…)`) is kept as typed.
- A file with several top-level topics gets a root named after the file.

Every non-blank line becomes one topic, so opening and saving never loses
text. Saving does rewrite the file in the layout above, and mull says so
when you open a file that will change shape. Saves are atomic, go through
symlinks, and keep the file's permissions.

Folds are view state and aren't saved.

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

## Developing

```sh
just check    # rustfmt, clippy, and the tests
```

`docs/design.md` explains the model, the layout, and the open questions.
The key tables above are checked against `mull --keys` by the tests, so
when you change a binding in `src/keymap.rs`, paste the new output here.
