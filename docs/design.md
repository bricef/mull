# mull design notes

This is the first draft's reasoning, written down so it can be argued with.
The open questions at the end are the decisions worth making before the
format and the keys settle.

## The model

A map is a tree of **nodes**, and every node has a **kind** that says what
it is in Markdown terms. The kind decides what the node may hold and how it
is written back.

| Kind | What it is | Text | Children |
|---|---|---|---|
| Document | the whole file, when it isn't one heading holding everything | the file's name, never written | anything |
| Heading | a section | one line of inline Markdown | content, then sub-sections |
| Item | a list item | its first line | items and blocks |
| Block | content: a paragraph, code, a table, a quote, HTML, maths, front matter, a link or footnote definition, a rule | the exact Markdown source, any number of lines | none |

Headings and items are **topics**: you add, type and rename them in mull.
Blocks are **content**: mull shows them as leaves and lets you move, cut,
copy and paste them, but never edits them; that's what a Markdown editor is
for. A topic's text is raw inline Markdown on one line (`tree::normalize_text`),
so `**bold**` and `[a link](…)` survive untouched; a block's text is copied
in and out byte for byte.

Items remember their marker (`-`, `*`, `+`, or `1.`/`1)` with a number),
their task box (`[ ]`, `[x]` or none) and whether their list is loose
(blank lines between items). Headings remember their level.

There is exactly one root. Positions are **paths** of child indices from the
root (`[]` is the root, `[1, 0]` the first child of the second child). The
tree is an owned `Node { kind, text, children, collapsed }`, which keeps
equality, cloning and undo trivial. Undo is whole-tree snapshots: a mind map
is small, and snapshots can't drift out of step with the operations the way
inverse operations can.

`collapsed` is view state. It lives in the node so it follows the topic
through moves, but it isn't saved, and folding isn't an undoable change.

### Trees that Markdown can say

Not every tree can be written as Markdown and read back as the same tree,
so mull keeps to the ones that can (`tree::Node::check`):

1. Blocks have no children.
2. In a heading or the document, content (items and blocks) comes before
   sub-sections: anything written after a heading belongs to it.
3. Items hold items and blocks, never headings.
4. Headings nest at most six deep.
5. Front matter can only be the document's first child.
6. Only the root can be a document.

Adding and pasting place a node where the rules allow: a new item in a
section goes after the section's content, before its sub-sections.
Outdenting an item out of a sub-section does the same one level up. A
change that would still break a rule (indenting a heading into a list item,
swapping content below a sub-section, giving a block children) is refused
with a message saying why, and the tree is left as it was.

## The file

mull reads Markdown with [markdown-rs](https://github.com/wooorm/markdown-rs)
(CommonMark, GitHub's extensions, front matter and maths), then turns its
syntax tree into a map (`markdown::parse`):

- Headings nest by level: content after a heading belongs to it until a
  heading of the same or a higher level. (In the syntax tree headings are
  flat; this sectioning pass is what makes them a hierarchy.)
- Lists become items, nested as the lists are. An item's first paragraph is
  its text, lines joined with spaces; the rest of the item becomes its
  children.
- Everything else becomes a block, sliced verbatim from the file, with the
  indentation of the list item around it removed.
- If the file is one heading with everything inside it, that heading is the
  root; otherwise the root is the document, named after the file.

Saving walks the tree and writes each node as its kind (`markdown::serialize`):
headings as `#` headings, items with their own markers, numbers and task
boxes, blocks exactly as read, indented to sit inside their list item. A
section's heading level is its stored level, raised if the section has been
moved deeper and lowered if needed so that it still closes the sections
before it; a file that was never restructured keeps its levels.

Text a user types into a topic is made safe to write (`markdown::safe_text`):
if it would read back as something else (`# x` as an item becomes a
heading inside it), its first character is escaped (`\# x`). Typing `[ ] `
or `[x] ` at the start of an item makes it a task.

Saving keeps everything that changes how the file renders: the same HTML
comes out before and after (a test checks this on the kitchen-sink fixture).
What it doesn't keep is layout that renders the same:

- blank lines are normalised to one between blocks;
- underlined (setext) headings become `#` headings, closing `#`s go, and
  skipped heading levels close up when a section moves;
- indented code blocks become fenced ones;
- ordered lists are renumbered from their first number;
- an item's first paragraph is joined onto one line, so a hard line break
  inside it is lost;
- a list whose items have content blocks is written loose.

Because the parser follows CommonMark, plain indented lines are a paragraph,
not an outline: h-m-m's tab-indented files no longer open as maps.

### Working around markdown-rs

markdown-rs 1.0.0 (the latest release) has bugs that mull works around, each
covered by a test:

- Its front matter support panics on files that start with `---`, a blank
  line and a list or quote. mull finds front matter itself
  (`markdown::split_front_matter`) and leaves the construct off.
- It can still panic on unusual mixes of setext underlines and rules. mull
  catches the panic and keeps the file as one block, which saving writes
  back unchanged, and says so on open.
- It reads `# #x` as a heading saying "x"; the CommonMark answer is "#x".
  mull takes heading text from the heading's own line.
- It reads a rule straight after a setext heading, and the next line, as
  one paragraph. mull splits a paragraph that starts with a rule.
- Some constructs read differently depending on their neighbours (a quote
  straight after a paragraph line takes the next line lazily). So
  `markdown::parse` reads the file, writes it, and reads that again: the map
  on screen is the file as saving will leave it, and any file settles after
  one save.
- A sub-list ending in an empty item and a blank line reads back loose.
  That only changes blank lines, and is left alone.

These are worth reporting upstream.

## The layout

Root at the left, children stacked to the right, each parent centred
vertically on its children, joined with box-drawing characters
(`layout::layout`). Each child column starts just right of its own parent
(compact), rather than in a column shared by the whole depth (aligned).

Each subtree occupies a block of rows of its own, which is why the layout
can't overlap; the property tests check that, and that the map is exactly
as tall as it has visible leaves.

Long topics are cut to `--max-width` columns with `…`; the topic being typed
is always shown in full. A block shows its first line (a paragraph shows all
of it, joined), with `[N lines]` after it when there's more, in a quieter
style than topics.

## Keys

`keymap::BINDINGS` is the only place a key is bound. Dispatch, the hint bar,
the `?` screen, `--keys` and the README all derive from it, and tests check
there are no clashes and that the README matches. The hint bar is a
separate, curated list (`HINTS`) of which actions deserve the bottom of the
screen; it names actions, and their keys come from the table.

Movement is visual: `j`/`k` go to the next topic *at the same depth* on
screen, crossing between parents; `l` goes to the child nearest the
current row.

## Colour

`src/theme.rs` is the only place a colour is named. The UI asks for a style
by role (`theme.branch(i).line`, `theme.mode(context).key`,
`theme.tone(tone)`), so changing the look, or adding a theme, touches one
file. The palette is Catppuccin Mocha.

Colour follows structure: the layout tags each connector with the
first-level branch it belongs to, and that branch's accent colours its
lines, its first-level topic and its selection highlight. The root's own
junction is neutral, a shared trunk. Topics below the first level stay in
plain text so the accents don't shout.

Every role sets both foreground and background, so nothing depends on the
terminal's own colours, and a test holds each role to WCAG contrast: 4.5:1
for text, 3:1 for lines. Depth comes from the environment: `COLORTERM`
means 24-bit, otherwise the nearest xterm-256 colour, and `NO_COLOR` (or
`TERM=dumb`) gets a theme made of bold, underline and reverse video alone.

## Testing

- Example tests for each module, with hand-written expected output.
- Property tests (`tests/properties.rs`): Markdown round-trips and settles
  into one form, every line becomes a topic, layouts never overlap, moves
  carry whole subtrees, and random key sessions keep the selection valid
  and undo back to the start exactly.
- File tests for atomic saves, symlinks and permissions; CLI tests for
  `--print` and the README.

## Open questions

1. **Turning topics into sections.** Enter on a section adds a section, but
   nothing yet makes the first sub-section of a heading, or turns a list
   item into a heading and back. A key for each would do it.
2. **Persist folds?** h-m-m doesn't. Options are a marker in the text, an
   HTML comment, or a sidecar file. Each costs some of the plain-Markdown
   promise.
3. **Empty topics.** Enter on a new, empty topic keeps it, as `-` or `#`.
   Dropping it instead (as Esc does) would be tidier, and would avoid a
   markdown-rs quirk: a sub-list ending in an empty item reads back loose.
4. **Classic radial layout.** The root in the middle, with children
   split left and right like a drawn mind map, instead of everything flowing
   right.
5. **System clipboard.** `y`/`d`/`p` use an internal register; h-m-m uses
   the system clipboard via xclip/wl-clipboard.
6. **Search** (`/`, `n`, `N`), **export** (HTML via Markmap, PNG), and
   **mouse** support are the obvious next features.
7. **More themes.** A light palette (Catppuccin Latte) for light terminals,
   chosen with a `--theme` flag or a config file.
