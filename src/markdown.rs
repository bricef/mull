//! Maps on disk are Markdown. mull reads it with markdown-rs, makes its
//! headings into sections and everything else into blocks, and writes it
//! back node by node (docs/design.md, "The file").

use markdown::{mdast, unist::Position};

use crate::tree::{
    BlockKind, Kind, Node, join_lines, lists_continue, normalize_text, written_level,
};

/// A Markdown file as mull holds it: its front matter, kept aside as the
/// file's metadata (never shown, never part of the structure), and the tree
/// of sections and blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub front_matter: Option<String>,
    pub root: Node,
}

impl From<Node> for Map {
    fn from(root: Node) -> Self {
        Map {
            front_matter: None,
            root,
        }
    }
}

/// Written between two lists that would otherwise read back as one, as
/// CommonMark recommends: an empty HTML comment, which renders as nothing.
/// Read back between two such lists, it's the boundary, not a block.
pub const LIST_SEPARATOR: &str = "<!-- -->";

/// CommonMark with GitHub's extensions and maths. Front matter is found by
/// [`split_front_matter`] instead: markdown-rs 1.0.0 panics on files that
/// start with `---`, a blank line and a list or quote when it looks for
/// front matter itself.
pub fn parse_options() -> markdown::ParseOptions {
    markdown::ParseOptions {
        constructs: markdown::Constructs {
            frontmatter: false,
            math_flow: true,
            math_text: true,
            ..markdown::Constructs::gfm()
        },
        ..markdown::ParseOptions::gfm()
    }
}

// Reading

/// Read a map from Markdown. `name` names the root when the file isn't one
/// top-level heading holding everything.
///
/// The map is what the file becomes once saved: it's read, written, and read
/// again. markdown-rs reads some constructs differently depending on what's
/// next to them (a rule straight after a setext heading, say), and mull
/// writes blank lines between blocks, so a single read can describe a file
/// that a save then changes. Reading the written form settles it, and
/// what's on screen is what saving keeps.
pub fn parse(input: &str, name: &str) -> Map {
    read(&serialize(&read(input, name)), name)
}

/// One reading of `input` into a map.
fn read(input: &str, name: &str) -> Map {
    let input = input.replace("\r\n", "\n");
    let (front_matter, source) = split_front_matter(&input);
    let front_matter = front_matter.map(str::to_string);
    let mut document = Node::document(name);
    let Some(ast) = syntax_tree(source) else {
        // Kept whole, so saving writes it back exactly as it was.
        let source = trim_blank_lines(source);
        if !source.is_empty() {
            document
                .children
                .push(Node::block(BlockKind::Other, source));
        }
        return Map {
            front_matter,
            root: document,
        };
    };
    let reader = Reader { source };
    // Headings still taking content, outermost first. In the syntax tree
    // headings are flat; this is what makes them sections.
    let mut open: Vec<Node> = Vec::new();
    for child in ast.children().into_iter().flatten() {
        if let mdast::Node::Heading(heading) = child {
            while open.last().is_some_and(
                |o| matches!(o.kind, Kind::Heading { level } if level >= heading.depth),
            ) {
                close(&mut open, &mut document);
            }
            open.push(reader.heading(heading));
        } else {
            let container = open.last_mut().unwrap_or(&mut document);
            reader.content(child, &mut container.children);
        }
    }
    while !open.is_empty() {
        close(&mut open, &mut document);
    }
    drop_list_separators(&mut document);
    // One top-level heading holding everything is the root. Anything above
    // it, or a second one, makes the document the root.
    let root = if document.children.len() == 1 && document.children[0].is_heading() {
        document.children.swap_remove(0)
    } else {
        document
    };
    Map { front_matter, root }
}

/// Take out the separators between lists that would otherwise be one list:
/// the map holds the two lists side by side, and writing puts the
/// separator back.
fn drop_list_separators(node: &mut Node) {
    let mut i = 1;
    while i + 1 < node.children.len() {
        let separator = node.children[i].kind == Kind::Block(BlockKind::Html)
            && node.children[i].text.trim() == LIST_SEPARATOR;
        if separator && lists_continue(&node.children[i - 1], &node.children[i + 1]) {
            node.children.remove(i);
        } else {
            i += 1;
        }
    }
    for child in &mut node.children {
        drop_list_separators(child);
    }
}

/// Front matter at the very start of a file, `---` to `---` (YAML) or `+++`
/// to `+++` (TOML), and the rest of the file after it.
pub fn split_front_matter(source: &str) -> (Option<&str>, &str) {
    for fence in ["---", "+++"] {
        let Some(rest) = source
            .strip_prefix(fence)
            .and_then(|r| r.strip_prefix('\n'))
        else {
            continue;
        };
        let mut offset = fence.len() + 1;
        for line in rest.split_inclusive('\n') {
            if line.trim_end() == fence {
                let block = &source[..offset + line.trim_end().len()];
                return (Some(block), &source[offset + line.len()..]);
            }
            offset += line.len();
        }
    }
    (None, source)
}

/// markdown-rs's syntax tree for `source`, or `None` if it can't read it.
/// markdown-rs 1.0.0 panics on some unusual combinations of setext
/// underlines and rules, so a panic is caught and treated as unreadable,
/// with the panic hook silenced meanwhile: nothing is printed, and the UI's
/// hook doesn't tear down the screen.
fn syntax_tree(source: &str) -> Option<mdast::Node> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| markdown::to_mdast(source, &parse_options()));
    std::panic::set_hook(hook);
    // Without MDX, which is off, reading can't fail any other way.
    result.ok()?.ok()
}

/// Text without its leading blank lines or trailing whitespace: the blank
/// lines around a block are the writer's to choose. Leading spaces on the
/// first line that has text are kept, since they can mean code.
fn trim_blank_lines(text: &str) -> &str {
    let start = text
        .split_inclusive('\n')
        .take_while(|line| line.trim().is_empty())
        .map(str::len)
        .sum::<usize>();
    text[start..].trim_end()
}

/// Whether markdown-rs can read `source` at all; when it can't, mull keeps
/// the file as one block.
pub fn readable(input: &str) -> bool {
    let input = input.replace("\r\n", "\n");
    syntax_tree(split_front_matter(&input).1).is_some()
}

/// Finish the innermost open section by adding it to the one around it.
fn close(open: &mut Vec<Node>, document: &mut Node) {
    if let Some(section) = open.pop() {
        open.last_mut().unwrap_or(document).children.push(section);
    }
}

struct Reader<'a> {
    source: &'a str,
}

impl Reader<'_> {
    fn slice(&self, position: Option<&Position>) -> &str {
        position.map_or("", |p| &self.source[p.start.offset..p.end.offset])
    }

    /// A heading's text is the source of its content, taken from the
    /// heading's own lines as CommonMark defines it rather than from its
    /// children (whose positions skip escapes, and which markdown-rs gets
    /// wrong for content starting with `#`).
    fn heading(&self, heading: &mdast::Heading) -> Node {
        let source = self.slice(heading.position.as_ref());
        let text = match source.rsplit_once('\n') {
            // Setext: the lines above the underline. Written as an ATX
            // heading, a trailing ` #` would read as a closing sequence.
            Some((content, _underline)) => escape_closing_hashes(&join_lines(content)),
            None => atx_content(source).to_string(),
        };
        Node::heading(heading.depth, &text)
    }

    /// Add the block, or blocks, that a piece of content becomes to `into`.
    fn content(&self, node: &mdast::Node, into: &mut Vec<Node>) {
        if let mdast::Node::Paragraph(paragraph) = node {
            // markdown-rs 1.0.0 reads a rule straight after a setext heading,
            // and the line after it, as one paragraph. CommonMark never
            // starts a paragraph with a rule, so split it off and read the
            // rest again on its own.
            let column = paragraph.position.as_ref().map_or(1, |p| p.start.column);
            let source = dedent(self.slice(paragraph.position.as_ref()), column - 1);
            if let Some((first, rest)) = source.split_once('\n')
                && is_rule(first)
            {
                into.push(Node::block(BlockKind::Rule, first.trim()));
                into.extend(read_content(rest));
                return;
            }
        }
        into.push(self.block(node));
    }

    /// A block, kept as written, its later lines less the indentation of
    /// its first.
    fn block(&self, node: &mdast::Node) -> Node {
        let kind = match node {
            mdast::Node::Paragraph(_) => BlockKind::Paragraph,
            mdast::Node::List(_) => BlockKind::List,
            mdast::Node::Code(_) => BlockKind::Code,
            mdast::Node::Table(_) => BlockKind::Table,
            mdast::Node::Blockquote(_) => BlockKind::Quote,
            mdast::Node::Html(_) => BlockKind::Html,
            mdast::Node::Math(_) => BlockKind::Maths,
            mdast::Node::Definition(_) => BlockKind::Definition,
            mdast::Node::FootnoteDefinition(_) => BlockKind::Footnote,
            mdast::Node::ThematicBreak(_) => BlockKind::Rule,
            _ => BlockKind::Other,
        };
        let Some(position) = node.position() else {
            return Node::block(kind, "");
        };
        let source = dedent(
            self.slice(Some(position)),
            position.start.column.saturating_sub(1),
        );
        let source = match node {
            // An indented code block can't follow a list without joining it,
            // so it's written fenced: it renders the same.
            mdast::Node::Code(code) if !source.starts_with("```") && !source.starts_with("~~~") => {
                fence(code)
            }
            _ => source,
        };
        Node::block(kind, source.trim_end_matches('\n'))
    }
}

/// What `source` holds when read on its own as content, with no sections:
/// any heading in it is kept as a block.
fn read_content(source: &str) -> Vec<Node> {
    let mut out = Vec::new();
    let Some(ast) = syntax_tree(source) else {
        out.push(Node::block(BlockKind::Other, trim_blank_lines(source)));
        return out;
    };
    let reader = Reader { source };
    for child in ast.children().into_iter().flatten() {
        match child {
            mdast::Node::Heading(_) => out.push(reader.block(child)),
            _ => reader.content(child, &mut out),
        }
    }
    out
}

/// Whether a line is a thematic break: three or more of one of `-`, `*` or
/// `_`, with nothing but spaces between.
fn is_rule(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !matches!(c, ' ' | '\t')).collect();
    marks.len() >= 3
        && ["-", "*", "_"]
            .iter()
            .any(|mark| marks.chars().all(|c| c.to_string() == *mark))
        && line.len() - line.trim_start_matches(' ').len() < 4
}

/// The content of an ATX heading line: after the opening `#`s and before
/// any closing sequence (`#`s after a space, or nothing but `#`s).
fn atx_content(line: &str) -> &str {
    let content = line
        .trim_start_matches(' ')
        .trim_start_matches('#')
        .trim_matches([' ', '\t']);
    let before_closing = content.trim_end_matches('#');
    if before_closing.is_empty() {
        ""
    } else if before_closing.ends_with([' ', '\t']) {
        before_closing.trim_end_matches([' ', '\t'])
    } else {
        content
    }
}

/// Text whose trailing `#`s would read as an ATX closing sequence, with the
/// last one escaped.
fn escape_closing_hashes(text: &str) -> String {
    let before = text.trim_end_matches('#');
    if text.ends_with('#') && (before.is_empty() || before.ends_with([' ', '\t'])) {
        format!("{}\\#", &text[..text.len() - 1])
    } else {
        text.to_string()
    }
}

/// Remove up to `columns` columns of indentation from every line but the
/// first (which starts where the block does).
fn dedent(text: &str, columns: usize) -> String {
    let mut lines = text.split('\n');
    let mut out = String::from(lines.next().unwrap_or(""));
    for line in lines {
        out.push('\n');
        let mut column = 0;
        let mut cut = 0;
        for (i, c) in line.char_indices() {
            let next = match c {
                ' ' => column + 1,
                '\t' => column + 4 - column % 4,
                _ => break,
            };
            if next > columns {
                break;
            }
            column = next;
            cut = i + c.len_utf8();
        }
        out.push_str(&line[cut..]);
    }
    out
}

/// A code block's content inside a backtick fence longer than any run of
/// backticks in it.
fn fence(code: &mdast::Code) -> String {
    let longest = code
        .value
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}\n{}\n{fence}", code.value)
}

// Writing

/// Write the map as Markdown: its front matter as it was, then each
/// section as a heading at the level its place in the tree gives it, and
/// each block exactly as read.
pub fn serialize(map: &Map) -> String {
    let mut writer = Writer::default();
    match map.root.kind {
        Kind::Document => writer.section(&map.root.children, 0),
        _ => {
            let level = match map.root.kind {
                Kind::Heading { level } => level.clamp(1, 6),
                _ => 1,
            };
            writer.heading(level, &map.root.text);
            writer.section(&map.root.children, level);
        }
    }
    let body = writer.out;
    match &map.front_matter {
        Some(front_matter) if body.is_empty() => format!("{front_matter}\n"),
        Some(front_matter) => format!("{front_matter}\n\n{body}"),
        // A file starting with a `---` or `+++` line reads as front matter
        // if one closes it later; a blank first line keeps a rule or a
        // paragraph from being taken for it.
        None if matches!(body.lines().next().map(str::trim_end), Some("---" | "+++")) => {
            format!("\n{body}")
        }
        None => body,
    }
}

#[derive(Default)]
struct Writer {
    out: String,
}

impl Writer {
    /// Start the next piece, after a blank line.
    fn start(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn heading(&mut self, level: u8, text: &str) {
        self.start();
        self.out.push_str(&"#".repeat(usize::from(level)));
        if !text.is_empty() {
            self.out.push(' ');
            self.out.push_str(text);
        }
        self.out.push('\n');
    }

    /// A section's children: its blocks, then its sub-sections. Two lists
    /// side by side get a separator, or they'd read back as one.
    fn section(&mut self, children: &[Node], level: u8) {
        let mut ceiling = 6;
        for (i, child) in children.iter().enumerate() {
            if i > 0 && lists_continue(&children[i - 1], child) {
                self.start();
                self.out.push_str(LIST_SEPARATOR);
                self.out.push('\n');
            }
            match child.kind {
                Kind::Heading { level: stored } => {
                    let written = written_level(level, stored, ceiling).min(6);
                    ceiling = written;
                    self.heading(written, &child.text);
                    self.section(&child.children, written);
                }
                Kind::Block(_) => {
                    self.start();
                    for line in child.text.split('\n') {
                        self.out.push_str(line);
                        self.out.push('\n');
                    }
                }
                // Never in a checked tree; keep what it holds anyway.
                Kind::Document => self.section(&child.children, level),
            }
        }
    }
}

// Typing

/// Text typed into a heading, made safe to write: if it would read back as
/// something else (`C #` would lose its `#` as a closing sequence), the
/// character at fault is escaped, which renders the same.
pub fn safe_text(kind: &Kind, typed: &str) -> String {
    let mut text = normalize_text(typed);
    for _ in 0..4 {
        if reads_back(kind, &text) {
            break;
        }
        let escaped = escape_once(&text);
        if escaped == text {
            break;
        }
        text = escaped;
    }
    text
}

/// Whether `text`, as the heading of a section, reads back as the same text.
/// Only headings are typed into, so any other kind always does.
pub fn reads_back(kind: &Kind, text: &str) -> bool {
    match kind {
        Kind::Heading { .. } => {
            let read = parse(&serialize(&Map::from(Node::heading(1, text))), "probe").root;
            read.is_heading() && read.text == text && read.children.is_empty()
        }
        Kind::Document | Kind::Block(_) => true,
    }
}

fn escape_once(text: &str) -> String {
    // Trailing `#`s would be taken for a closing sequence.
    if text.ends_with('#')
        && let Some(at) = text.rfind('#')
    {
        return format!("{}\\{}", &text[..at], &text[at..]);
    }
    match text.chars().next() {
        Some(c) if c.is_ascii_punctuation() && c != '\\' => format!("\\{text}"),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The tree as an indented outline of kinds and labels.
    fn outline(node: &Node) -> String {
        fn walk(node: &Node, depth: usize, out: &mut String) {
            let kind = match &node.kind {
                Kind::Document => "doc".to_string(),
                Kind::Heading { level } => format!("h{level}"),
                Kind::Block(kind) => format!("{kind:?}"),
            };
            out.push_str(&format!("{}{kind} {}\n", "  ".repeat(depth), node.label()));
            for child in &node.children {
                walk(child, depth + 1, out);
            }
        }
        let mut out = String::new();
        walk(node, 0, &mut out);
        out
    }

    fn read_outline(doc: &str) -> String {
        outline(&parse(doc, "notes").root)
    }

    fn round_trip(doc: &str) -> String {
        serialize(&parse(doc, "notes"))
    }

    #[test]
    fn headings_are_sections_and_the_rest_are_blocks() {
        let doc = concat!(
            "# Trip to Lisbon\n\nNotes for the October trip.\n\n",
            "## Getting there\n\n- Flights\n- Train from Madrid\n\n",
            "## Where to stay\n\nAlfama or Baixa?\n\n### Alfama\n\n",
            "| Hotel | Price |\n|-------|-------|\n| Lapa  | €€    |\n",
        );
        assert_eq!(
            read_outline(doc),
            concat!(
                "h1 Trip to Lisbon\n",
                "  Paragraph Notes for the October trip.\n",
                "  h2 Getting there\n",
                "    List - Flights\n",
                "  h2 Where to stay\n",
                "    Paragraph Alfama or Baixa?\n",
                "    h3 Alfama\n",
                "      Table | Hotel | Price |\n",
            )
        );
        assert_eq!(round_trip(doc), doc);
    }

    #[test]
    fn several_top_level_headings_make_a_document() {
        let doc = "# Ideas\n\nSome half-formed thoughts.\n\n# Done\n\n- shipped v0.1\n";
        assert_eq!(
            read_outline(doc),
            "doc notes\n  h1 Ideas\n    Paragraph Some half-formed thoughts.\n  h1 Done\n    List - shipped v0.1\n"
        );
        assert_eq!(round_trip(doc), doc);
    }

    #[test]
    fn one_top_level_heading_of_any_level_is_the_root() {
        let doc = "## Notes\n\nAll of it.\n\n### Part\n";
        assert_eq!(
            read_outline(doc),
            "h2 Notes\n  Paragraph All of it.\n  h3 Part\n"
        );
        assert_eq!(round_trip(doc), doc);
    }

    #[test]
    fn content_above_the_heading_makes_a_document() {
        let doc = "<p align=\"center\"><img src=\"logo.png\"></p>\n\n# mull\n\nA mind mapper.\n";
        assert_eq!(
            read_outline(doc),
            "doc notes\n  Html <p align=\"center\"><img src=\"logo.png\"></p>\n  h1 mull\n    Paragraph A mind mapper.\n"
        );
    }

    #[test]
    fn front_matter_is_metadata_kept_aside() {
        let doc = "---\ntitle: Lisbon\ntags: [travel]\n---\n# Trip to Lisbon\n\n## Getting there\n";
        let map = parse(doc, "notes");
        assert_eq!(
            map.front_matter.as_deref(),
            Some("---\ntitle: Lisbon\ntags: [travel]\n---")
        );
        assert_eq!(
            outline(&map.root),
            "h1 Trip to Lisbon\n  h2 Getting there\n"
        );
        assert_eq!(
            serialize(&map),
            "---\ntitle: Lisbon\ntags: [travel]\n---\n\n# Trip to Lisbon\n\n## Getting there\n"
        );
    }

    #[test]
    fn a_file_without_headings_is_a_document_of_blocks() {
        let doc = "Just a paragraph.\n\n- a\n- b\n";
        assert_eq!(
            read_outline(doc),
            "doc notes\n  Paragraph Just a paragraph.\n  List - a\n"
        );
        assert_eq!(round_trip(doc), doc);
    }

    #[test]
    fn a_whole_list_is_one_block_whatever_it_holds() {
        let doc = "# Plan\n\n- Goals\n  - fast\n\n  ```rust\n  # not a heading\n  ```\n- [ ] task\n  3. nested numbers\n";
        let root = parse(doc, "x").root;
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, Kind::Block(BlockKind::List));
        assert_eq!(serialize(&Map::from(root)), doc);
    }

    #[test]
    fn lists_side_by_side_stay_two_lists() {
        let tree = Node::heading(1, "R").with(vec![
            Node::block(BlockKind::List, "- a\n- b"),
            Node::block(BlockKind::List, "- c"),
            Node::block(BlockKind::List, "- d"),
            Node::block(BlockKind::List, "1. one"),
        ]);
        let written = serialize(&Map::from(tree.clone()));
        assert_eq!(
            written,
            "# R\n\n- a\n- b\n\n<!-- -->\n\n- c\n\n<!-- -->\n\n- d\n\n1. one\n"
        );
        assert_eq!(parse(&written, "x").root, tree);
    }

    #[test]
    fn a_comment_elsewhere_is_a_block() {
        let doc = "# R\n\n<!-- -->\n\n- a\n\n<!-- -->\n\n* b\n";
        assert_eq!(
            read_outline(doc),
            "h1 R\n  Html <!-- -->\n  List - a\n  Html <!-- -->\n  List * b\n"
        );
    }

    #[test]
    fn code_is_one_block_whatever_it_contains() {
        let doc = "# R\n\n```bash\n# a comment, not a heading\n- not an item\n```\n\n## After\n";
        assert_eq!(read_outline(doc), "h1 R\n  Code ```bash\n  h2 After\n");
    }

    #[test]
    fn front_matter_is_found_without_tripping_the_parser() {
        // markdown-rs 1.0.0 panics on this with its own front matter support.
        let map = parse("---\n\n> a\n", "x");
        assert_eq!(map.front_matter, None);
        assert_eq!(outline(&map.root), "doc x\n  Rule ---\n  Quote > a\n");
        let map = parse("+++\ntitle = \"x\"\n+++\n# T\n", "x");
        assert_eq!(map.front_matter.as_deref(), Some("+++\ntitle = \"x\"\n+++"));
        assert_eq!(outline(&map.root), "h1 T\n");
        assert_eq!(
            split_front_matter("---\nnot closed\n"),
            (None, "---\nnot closed\n")
        );
    }

    #[test]
    fn heading_text_is_what_follows_the_hashes() {
        let text = |doc: &str| parse(doc, "x").root.text;
        assert_eq!(text("# \\# escaped\n"), "\\# escaped");
        assert_eq!(text("# #x\n"), "#x");
        assert_eq!(text("## Closing ##\n"), "Closing");
        assert_eq!(text("# C#\n"), "C#");
        assert_eq!(text("# #\n"), "");
        assert_eq!(text("#é\n---\n"), "#é");
        assert_eq!(text("a #\n===\n"), "a \\#");
    }

    #[test]
    fn a_rule_after_a_setext_heading_stays_a_rule() {
        let doc = "Title\n---\n---\nText.\n";
        assert_eq!(
            read_outline(doc),
            "h2 Title\n  Rule ---\n  Paragraph Text.\n"
        );
        assert_eq!(round_trip(doc), "## Title\n\n---\n\nText.\n");
    }

    #[test]
    fn a_file_markdown_rs_cannot_read_is_kept_whole() {
        // markdown-rs 1.0.0 panics on this.
        let doc = "a\n---\n---\nb\n---\n";
        assert!(!readable(doc));
        assert_eq!(read_outline(doc), "doc notes\n  Other a\n");
        assert_eq!(round_trip(doc), doc);
    }

    #[test]
    fn an_empty_file_is_an_empty_document() {
        assert_eq!(parse("", "ideas").root, Node::document("ideas"));
        assert_eq!(parse("\n  \n", "ideas").root, Node::document("ideas"));
        assert_eq!(serialize(&Map::from(Node::document("ideas"))), "");
    }

    #[test]
    fn a_leading_rule_is_not_mistaken_for_front_matter() {
        let doc = "\n---\n\n```\n---\n```\n";
        let saved = round_trip(doc);
        assert_eq!(saved, "\n---\n\n```\n---\n```\n");
        assert_eq!(round_trip(&saved), saved);
    }

    #[test]
    fn setext_headings_closing_hashes_and_indented_code_are_tidied() {
        let doc = "Title\n=====\n\n## Part ##\n\n    indented code\n";
        assert_eq!(
            round_trip(doc),
            "# Title\n\n## Part\n\n```\nindented code\n```\n"
        );
    }

    #[test]
    fn writes_blocks_then_sub_sections() {
        let tree = Node::heading(1, "Plan").with(vec![
            Node::block(BlockKind::List, "- Goals\n  - fast"),
            Node::block(BlockKind::Paragraph, "Some prose\nover two lines."),
            Node::heading(2, "Later").with(vec![Node::block(BlockKind::Paragraph, "then")]),
        ]);
        let written = serialize(&Map::from(tree.clone()));
        assert_eq!(
            written,
            "# Plan\n\n- Goals\n  - fast\n\nSome prose\nover two lines.\n\n## Later\n\nthen\n"
        );
        assert_eq!(parse(&written, "x").root, tree);
    }

    #[test]
    fn moved_sections_take_the_level_their_place_needs() {
        // B was a level-2 section, moved under A; it's written at level 3.
        let tree = Node::heading(1, "R").with(vec![
            Node::heading(2, "A").with(vec![Node::heading(2, "B")]),
        ]);
        assert_eq!(serialize(&Map::from(tree)), "# R\n\n## A\n\n### B\n");
    }

    #[test]
    fn typed_headings_that_would_change_meaning_are_escaped() {
        let heading = Kind::Heading { level: 2 };
        assert_eq!(safe_text(&heading, "plain **text**"), "plain **text**");
        assert_eq!(safe_text(&heading, "C #"), "C \\#");
        assert_eq!(safe_text(&heading, "# fine"), "# fine");
        assert_eq!(safe_text(&heading, "C# and F#"), "C# and F#");
    }
}
