# mull design notes

This is the first draft's reasoning, written down so it can be argued with.
The open questions at the end are the decisions worth making before the
format and the keys settle.

## The model

A map is a tree of **topics**. A topic is one line of raw inline Markdown,
trimmed: newlines, tabs and other control characters become spaces
(`tree::normalize_text`). mull never escapes or unescapes that text, so
`**bold**` or `[a link](…)` survive untouched.

There is exactly one root. Positions are **paths** of child indices from the
root (`[]` is the root, `[1, 0]` the first child of the second child). The
tree is an owned `Node { text, children, collapsed }`, which keeps equality,
cloning and undo trivial. Undo is whole-tree snapshots: a mind map is small,
and snapshots can't drift out of step with the operations the way inverse
operations can.

`collapsed` is view state. It lives in the node so it follows the topic
through moves, but it isn't saved, and folding isn't an undoable change.

## The file

Written form (`markdown::serialize`): the root as `# heading`, everything
else as a `-` list indented two spaces per level, with a blank line between
the two. It's the plainest Markdown that still reads as an outline, and
Markmap draws it as a mind map.

Read form (`markdown::parse`) is a single rule applied line by line:

- A heading is a child of the nearest open heading of a lower level.
- Any other non-blank line is a child of the nearest open line (since the
  last heading) that is indented less, or else of the current heading.
- List markers are stripped from the text; plain lines keep theirs.

So every non-blank line becomes exactly one topic. That's the guarantee
that makes it safe to open arbitrary notes: structure may change on save,
text never disappears. When the file on disk isn't already in the written
form, mull says so on open.

Known gaps, all things that become topics rather than being understood:
fenced code blocks, front matter, setext (`===`) headings, lazy paragraph
continuation lines, and closing `#` sequences on headings.

## The layout

Root at the left, children stacked to the right, each parent centred
vertically on its children, joined with box-drawing characters
(`layout::layout`). Each child column starts just right of its own parent
(compact), rather than in a column shared by the whole depth (aligned).

Each subtree occupies a block of rows of its own, which is why the layout
can't overlap; the property tests check that, and that the map is exactly
as tall as it has visible leaves.

Long topics are cut to `--max-width` columns with `…`; the topic being typed
is always shown in full.

## Keys

`keymap::BINDINGS` is the only place a key is bound. Dispatch, the hint bar,
the `?` screen, `--keys` and the README all derive from it, and tests check
there are no clashes and that the README matches. The hint bar is a
separate, curated list (`HINTS`) of which actions deserve the bottom of the
screen; it names actions, and their keys come from the table.

Movement is visual: `j`/`k` go to the next topic *at the same depth* on
screen, crossing between parents; `l` goes to the child nearest the
current row.

## Testing

- Example tests for each module, with hand-written expected output.
- Property tests (`tests/properties.rs`): Markdown round-trips and settles
  into one form, every line becomes a topic, layouts never overlap, moves
  carry whole subtrees, and random key sessions keep the selection valid
  and undo back to the start exactly.
- File tests for atomic saves, symlinks and permissions; CLI tests for
  `--print` and the README.

## Open questions

1. **First-level topics as `##` headings?** `## Topic` for depth one reads
   better as a document and makes sections foldable in editors. Bullets
   everywhere is simpler and diffs better.
2. **Persist folds?** h-m-m doesn't. Options are a marker in the text, an
   HTML comment, or a sidecar file. Each costs some of the plain-Markdown
   promise.
3. **Notes on topics.** Multi-line bodies under a topic (indented paragraphs
   in the file) would make the format richer but need a notes pane.
4. **Classic radial layout.** The root in the middle, with children
   split left and right like a drawn mind map, instead of everything flowing
   right.
5. **System clipboard.** `y`/`d`/`p` use an internal register; h-m-m uses
   the system clipboard via xclip/wl-clipboard.
6. **Search** (`/`, `n`, `N`), **export** (HTML via Markmap, PNG), and
   **mouse** support are the obvious next features.
7. **Licence.** None chosen yet.
