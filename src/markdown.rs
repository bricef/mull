//! Maps on disk are Markdown. mull reads it with markdown-rs and writes it
//! back node by node, each as its kind (docs/design.md, "The file").

use markdown::{mdast, unist::Position};

use crate::tree::{
    BlockKind, ItemStyle, Kind, Marker, Node, join_lines, normalize_text, written_level,
};

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
/// heading holding everything.
///
/// The map is what the file becomes once saved: it's read, written, and read
/// again. markdown-rs reads some constructs differently depending on what's
/// next to them (a quote straight after a paragraph line, a rule straight
/// after a setext heading), and mull writes blank lines between blocks, so a
/// single read can describe a file that a save then changes. Reading the
/// written form settles it, and what's on screen is what saving keeps.
pub fn parse(input: &str, name: &str) -> Node {
    read(&serialize(&read(input, name)), name)
}

/// One reading of `input` into a map.
fn read(input: &str, name: &str) -> Node {
    let input = input.replace("\r\n", "\n");
    let (front_matter, source) = split_front_matter(&input);
    let mut document = Node::document(name);
    if let Some(front_matter) = front_matter {
        document
            .children
            .push(Node::block(BlockKind::FrontMatter, front_matter));
    }
    let Some(ast) = syntax_tree(source) else {
        // Kept whole, so saving writes it back exactly as it was.
        document
            .children
            .push(Node::block(BlockKind::Other, trim_blank_lines(source)));
        return document;
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
    if document.children.len() == 1 && document.children[0].is_heading() {
        document.children.swap_remove(0)
    } else {
        document
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

    /// Add what a piece of content becomes to `into`: a list's items, or a
    /// block.
    fn content(&self, node: &mdast::Node, into: &mut Vec<Node>) {
        match node {
            mdast::Node::List(list) => {
                let loose = list.spread
                    || list
                        .children
                        .iter()
                        .any(|i| matches!(i, mdast::Node::ListItem(item) if item.spread));
                let start = list.start.unwrap_or(1);
                let items = list.children.iter().filter_map(|child| match child {
                    mdast::Node::ListItem(item) => Some(item),
                    _ => None,
                });
                for (i, item) in items.enumerate() {
                    let number = start.saturating_add(i as u32);
                    into.push(self.item(list, item, number, loose));
                }
            }
            mdast::Node::Paragraph(paragraph) => {
                // markdown-rs 1.0.0 reads a rule straight after a setext
                // heading, and the line after it, as one paragraph. CommonMark
                // never starts a paragraph with a rule, so split it off and
                // read the rest again on its own.
                let column = paragraph.position.as_ref().map_or(1, |p| p.start.column);
                let source = dedent(self.slice(paragraph.position.as_ref()), column - 1);
                match source.split_once('\n') {
                    Some((first, rest)) if is_rule(first) => {
                        into.push(Node::block(BlockKind::Rule, first.trim()));
                        into.extend(read_content(rest));
                    }
                    _ => into.push(self.block(node)),
                }
            }
            other => into.push(self.block(other)),
        }
    }

    /// An item's first paragraph is its text; everything after it in the
    /// item becomes its children.
    fn item(&self, list: &mdast::List, item: &mdast::ListItem, number: u32, loose: bool) -> Node {
        let at = item.position.as_ref().map_or(0, |p| p.start.offset);
        let marker_source = self.source[at..].trim_start_matches([' ', '\t']);
        let marker = if list.ordered {
            let delimiter = marker_source
                .chars()
                .find(|c| !c.is_ascii_digit())
                .unwrap_or('.');
            Marker::Ordered { number, delimiter }
        } else {
            Marker::Bullet(marker_source.chars().next().unwrap_or('-'))
        };
        let style = ItemStyle {
            marker,
            checked: item.checked,
            loose,
        };
        let mut rest = item.children.iter().peekable();
        let text = match rest.peek() {
            Some(mdast::Node::Paragraph(paragraph)) => {
                rest.next();
                join_lines(self.slice(paragraph.position.as_ref()))
            }
            _ => String::new(),
        };
        let mut node = Node::item(style, &text);
        for child in rest {
            self.content(child, &mut node.children);
        }
        node
    }

    /// Anything else, kept as written, less the indentation of the list
    /// item it sits in.
    fn block(&self, node: &mdast::Node) -> Node {
        let kind = match node {
            mdast::Node::Paragraph(_) => BlockKind::Paragraph,
            mdast::Node::Code(_) => BlockKind::Code,
            mdast::Node::Table(_) => BlockKind::Table,
            mdast::Node::Blockquote(_) => BlockKind::Quote,
            mdast::Node::Html(_) => BlockKind::Html,
            mdast::Node::Math(_) => BlockKind::Maths,
            mdast::Node::Yaml(_) | mdast::Node::Toml(_) => BlockKind::FrontMatter,
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

/// Write the tree as Markdown, each node as its kind.
pub fn serialize(root: &Node) -> String {
    let mut writer = Writer::default();
    match root.kind {
        Kind::Document => {
            writer.section(&root.children, 0);
            // A file starting with a `---` or `+++` line reads as front
            // matter if one closes it later; a blank first line keeps a rule
            // or a paragraph from being taken for it.
            let first = writer.out.lines().next().map(str::trim_end);
            let front_matter_first = root
                .children
                .first()
                .is_some_and(|c| c.kind == Kind::Block(BlockKind::FrontMatter));
            if matches!(first, Some("---" | "+++")) && !front_matter_first {
                writer.out.insert(0, '\n');
            }
        }
        _ => {
            let level = match root.kind {
                Kind::Heading { level } => level.clamp(1, 6),
                _ => 1,
            };
            writer.heading(level, &root.text);
            writer.section(&root.children, level);
        }
    }
    writer.out
}

#[derive(Default)]
struct Writer {
    out: String,
}

impl Writer {
    /// Start the next piece: after a blank line unless `tight`.
    fn start(&mut self, tight: bool) {
        if !self.out.is_empty() && !tight && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn line(&mut self, indent: &str, text: &str) {
        if text.is_empty() {
            self.out.push_str(indent.trim_end());
        } else {
            self.out.push_str(indent);
            self.out.push_str(text);
        }
        self.out.push('\n');
    }

    fn heading(&mut self, level: u8, text: &str) {
        self.start(false);
        let hashes = "#".repeat(usize::from(level));
        if text.is_empty() {
            self.line("", &hashes);
        } else {
            self.line("", &format!("{hashes} {text}"));
        }
    }

    /// A heading's or the document's children: content, then sub-sections.
    fn section(&mut self, children: &[Node], level: u8) {
        let mut ceiling = 6;
        let mut i = 0;
        while i < children.len() {
            let child = &children[i];
            match child.kind {
                Kind::Heading { level: stored } => {
                    let written = written_level(level, stored, ceiling).min(6);
                    ceiling = written;
                    self.heading(written, &child.text);
                    self.section(&child.children, written);
                    i += 1;
                }
                Kind::Item(_) => i += self.list(&children[i..], "", false),
                Kind::Block(_) => {
                    self.block(child, "", false);
                    i += 1;
                }
                // Never in a checked tree; keep what it holds anyway.
                Kind::Document => {
                    self.section(&child.children, level);
                    i += 1;
                }
            }
        }
    }

    /// Write the items at the start of `nodes` that make one list, the first
    /// after a blank line unless `tight`. Returns how many it wrote.
    fn list(&mut self, nodes: &[Node], indent: &str, tight: bool) -> usize {
        let Some(Kind::Item(first)) = nodes.first().map(|n| &n.kind) else {
            return 1;
        };
        let run: Vec<&Node> = nodes
            .iter()
            .take_while(|n| matches!(n.kind, Kind::Item(s) if s.marker.same_list(first.marker)))
            .collect();
        let loose = run
            .iter()
            .any(|n| matches!(n.kind, Kind::Item(s) if s.loose) || needs_gap(n));
        let mut number = match first.marker {
            Marker::Ordered { number, .. } => number,
            Marker::Bullet(_) => 0,
        };
        for (i, item) in run.iter().enumerate() {
            self.start(if i == 0 { tight } else { !loose });
            self.item(item, indent, number, loose);
            number = number.saturating_add(1);
        }
        run.len()
    }

    fn item(&mut self, node: &Node, indent: &str, number: u32, loose: bool) {
        let Kind::Item(style) = node.kind else {
            return;
        };
        let marker = match style.marker {
            Marker::Bullet(bullet) => bullet.to_string(),
            Marker::Ordered { delimiter, .. } => format!("{number}{delimiter}"),
        };
        let mut first_line = marker.clone();
        if let Some(checked) = style.checked {
            first_line.push_str(if checked { " [x]" } else { " [ ]" });
        }
        if !node.text.is_empty() {
            first_line.push(' ');
            first_line.push_str(&node.text);
        }
        self.line(indent, &first_line);

        let inner = format!("{indent}{}", " ".repeat(marker.chars().count() + 1));
        // An item with nothing on its first line takes its first child on
        // the very next line: a blank line there would end the item.
        let bare = node.text.is_empty() && style.checked.is_none();
        let mut i = 0;
        while i < node.children.len() {
            let child = &node.children[i];
            let after_text = i == 0;
            match child.kind {
                Kind::Item(_) => {
                    let previous_is_block = i > 0 && node.children[i - 1].is_block();
                    let tight = if after_text {
                        bare || (!loose && can_start_after_text(child))
                    } else {
                        !previous_is_block && !loose && can_start_after_text(child)
                    };
                    i += self.list(&node.children[i..], &inner, tight);
                }
                // Never in a checked tree: written as an item, children and all.
                Kind::Heading { .. } | Kind::Document => {
                    let as_item = Node {
                        kind: Kind::Item(ItemStyle::default()),
                        ..child.clone()
                    };
                    self.list(std::slice::from_ref(&as_item), &inner, false);
                    i += 1;
                }
                Kind::Block(_) => {
                    self.block(child, &inner, after_text && bare);
                    i += 1;
                }
            }
        }
    }

    fn block(&mut self, node: &Node, indent: &str, tight: bool) {
        self.start(tight);
        for line in node.text.split('\n') {
            self.line(indent, line);
        }
    }
}

/// Whether an item needs blank lines among its children, which makes its
/// whole list loose: for content blocks, or for a sub-list that can't start
/// straight after a line of text.
pub fn needs_gap(item: &Node) -> bool {
    let bare = item.text.is_empty() && !matches!(item.kind, Kind::Item(s) if s.checked.is_some());
    item.children.iter().any(Node::is_block)
        || item.children.iter().enumerate().any(|(i, child)| {
            let starts_list = match (
                child.kind.clone(),
                i.checked_sub(1).map(|p| &item.children[p].kind),
            ) {
                (Kind::Item(_), None) => !bare,
                (Kind::Item(style), Some(Kind::Item(previous))) => {
                    !style.marker.same_list(previous.marker)
                }
                _ => false,
            };
            starts_list && !can_start_after_text(child)
        })
}

/// Whether a list starting with `first` can follow a line of text without a
/// blank line: CommonMark reads an empty first item, or a first number
/// other than 1, as more of that text.
fn can_start_after_text(first: &Node) -> bool {
    match first.kind {
        Kind::Item(style) => {
            let empty = first.text.is_empty() && style.checked.is_none();
            let numbered_off_one =
                matches!(style.marker, Marker::Ordered { number, .. } if number != 1);
            !empty && !numbered_off_one
        }
        _ => true,
    }
}

// Typing

/// Text typed into a topic of `kind`, made safe to write: if it would read
/// back as something else (`# x` in an item reads as a heading), its first
/// character is escaped, which renders the same.
pub fn safe_text(kind: &Kind, typed: &str) -> String {
    let mut text = normalize_text(typed);
    for _ in 0..4 {
        if reads_back(kind, &text) {
            break;
        }
        let escaped = escape_once(kind, &text);
        if escaped == text {
            break;
        }
        text = escaped;
    }
    text
}

/// Whether `text` as a topic of `kind` reads back as the same text.
pub fn reads_back(kind: &Kind, text: &str) -> bool {
    match kind {
        Kind::Heading { .. } => {
            let read = parse(&serialize(&Node::heading(1, text)), "probe");
            read.is_heading() && read.text == text && read.children.is_empty()
        }
        // The marker matters: `*` and `**` together make a rule, not an item.
        Kind::Item(style) => {
            let item = Node::item(
                ItemStyle {
                    loose: false,
                    ..*style
                },
                text,
            );
            let read = parse(
                &serialize(&Node::document("probe").with(vec![item.clone()])),
                "probe",
            );
            read.kind == Kind::Document && read.children == [item]
        }
        Kind::Document | Kind::Block(_) => true,
    }
}

fn escape_once(kind: &Kind, text: &str) -> String {
    // A heading ending in `#`s would lose them as a closing sequence.
    if matches!(kind, Kind::Heading { .. })
        && text.ends_with('#')
        && let Some(at) = text.rfind('#')
    {
        return format!("{}\\{}", &text[..at], &text[at..]);
    }
    match text.chars().next() {
        Some(c) if c.is_ascii_punctuation() && c != '\\' => format!("\\{text}"),
        Some(c) if c.is_ascii_digit() => {
            let digits = text.bytes().take_while(u8::is_ascii_digit).count();
            if text[digits..].starts_with(['.', ')']) {
                format!("{}\\{}", &text[..digits], &text[digits..])
            } else {
                text.to_string()
            }
        }
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
                Kind::Item(style) => match (style.marker, style.checked) {
                    (_, Some(true)) => "[x]".to_string(),
                    (_, Some(false)) => "[ ]".to_string(),
                    (Marker::Bullet(c), None) => c.to_string(),
                    (Marker::Ordered { number, delimiter }, None) => format!("{number}{delimiter}"),
                },
                Kind::Block(kind) => format!("{kind:?}"),
            };
            let text = if node.is_block() {
                node.label()
            } else {
                node.text.clone()
            };
            out.push_str(&format!("{}{kind} {text}\n", "  ".repeat(depth)));
            for child in &node.children {
                walk(child, depth + 1, out);
            }
        }
        let mut out = String::new();
        walk(node, 0, &mut out);
        out
    }

    #[test]
    fn headings_become_sections_and_lists_become_items() {
        let doc = "# Plan\n\nWhy we're doing this.\n\n- Goals\n  - fast\n- Risks\n\n## Later\n\n1. one\n2. two\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            concat!(
                "h1 Plan\n",
                "  Paragraph Why we're doing this.\n",
                "  - Goals\n",
                "    - fast\n",
                "  - Risks\n",
                "  h2 Later\n",
                "    1. one\n",
                "    2. two\n",
            )
        );
    }

    #[test]
    fn code_is_one_block_whatever_it_contains() {
        let doc = "# R\n\n```bash\n# a comment, not a heading\n- not an item\n```\n\n## After\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            "h1 R\n  Code ```bash\n  h2 After\n"
        );
        assert_eq!(
            parse(doc, "x").children[0].text,
            "```bash\n# a comment, not a heading\n- not an item\n```"
        );
    }

    #[test]
    fn several_top_level_things_make_a_document() {
        let doc = "---\ntitle: x\n---\n\n# A\n\n# B\n";
        assert_eq!(
            outline(&parse(doc, "notes")),
            "doc notes\n  FrontMatter ---\n  h1 A\n  h1 B\n"
        );
    }

    #[test]
    fn front_matter_is_found_without_tripping_the_parser() {
        // markdown-rs 1.0.0 panics on this with its own front matter support.
        let read = parse("---\n\n> a\n", "x");
        assert_eq!(outline(&read), "doc x\n  Rule ---\n  Quote > a\n");
        let read = parse("+++\ntitle = \"x\"\n+++\n# T\n", "x");
        assert_eq!(outline(&read), "doc x\n  FrontMatter +++\n  h1 T\n");
        assert_eq!(
            split_front_matter("---\nnot closed\n"),
            (None, "---\nnot closed\n")
        );
    }

    #[test]
    fn heading_text_is_what_follows_the_hashes() {
        let text = |doc: &str| parse(doc, "x").text;
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
            outline(&parse(doc, "x")),
            "h2 Title\n  Rule ---\n  Paragraph Text.\n"
        );
        assert_eq!(serialize(&parse(doc, "x")), "## Title\n\n---\n\nText.\n");
    }

    #[test]
    fn a_file_markdown_rs_cannot_read_is_kept_whole() {
        // markdown-rs 1.0.0 panics on this.
        let doc = "a\n---\n---\nb\n---\n";
        assert!(!readable(doc));
        let read = parse(doc, "x");
        assert_eq!(outline(&read), "doc x\n  Other a\n");
        assert_eq!(serialize(&read), doc);
    }

    #[test]
    fn an_empty_file_is_an_empty_document() {
        assert_eq!(parse("", "ideas"), Node::document("ideas"));
        assert_eq!(parse("\n  \n", "ideas"), Node::document("ideas"));
        assert_eq!(serialize(&Node::document("ideas")), "");
    }

    #[test]
    fn a_leading_rule_is_not_mistaken_for_front_matter() {
        let doc = "\n---\n\n```\n---\n```\n";
        let saved = serialize(&parse(doc, "x"));
        assert_eq!(saved, "\n---\n\n```\n---\n```\n");
        assert_eq!(serialize(&parse(&saved, "x")), saved);
    }

    #[test]
    fn items_keep_markers_numbers_and_tasks() {
        let doc = "* star\n\n3) three\n4) four\n\n- [ ] todo\n- [x] done\n";
        let read = parse(doc, "x");
        assert_eq!(
            outline(&read),
            "doc x\n  * star\n  3) three\n  4) four\n  [ ] todo\n  [x] done\n"
        );
        assert_eq!(serialize(&read), doc);
    }

    #[test]
    fn indented_markers_are_read_past_the_indentation() {
        let read = parse(" 0) zero\n\n  + plus\n", "x");
        assert_eq!(outline(&read), "doc x\n  0) zero\n  + plus\n");
    }

    #[test]
    fn blocks_in_items_lose_the_item_indentation_and_get_it_back() {
        let doc = "- item\n\n  ```js\n  code\n    more\n  ```\n\n  para\n  two\n\n- next\n";
        let read = parse(doc, "x");
        assert_eq!(
            read.children[0].children[0].text,
            "```js\ncode\n  more\n```"
        );
        assert_eq!(read.children[0].children[1].text, "para\ntwo");
        assert_eq!(serialize(&read), doc);
    }

    #[test]
    fn setext_headings_closing_hashes_and_indented_code_are_tidied() {
        let doc = "Title\n=====\n\n## Part ##\n\n    indented code\n";
        assert_eq!(
            serialize(&parse(doc, "x")),
            "# Title\n\n## Part\n\n```\nindented code\n```\n"
        );
    }

    #[test]
    fn writes_sections_by_kind_with_content_first() {
        let tree = Node::heading(1, "Plan").with(vec![
            Node::new("Goals").with(vec![Node::new("fast")]),
            Node::block(BlockKind::Paragraph, "Some prose\nover two lines."),
            Node::heading(2, "Later").with(vec![Node::new("then")]),
        ]);
        assert_eq!(
            serialize(&tree),
            "# Plan\n\n- Goals\n  - fast\n\nSome prose\nover two lines.\n\n## Later\n\n- then\n"
        );
        assert_eq!(parse(&serialize(&tree), "x"), tree);
    }

    #[test]
    fn moved_sections_take_the_level_their_place_needs() {
        // B was a level-2 section, moved under A; it's written at level 3.
        let tree = Node::heading(1, "R").with(vec![
            Node::heading(2, "A").with(vec![Node::heading(2, "B")]),
        ]);
        assert_eq!(serialize(&tree), "# R\n\n## A\n\n### B\n");
    }

    #[test]
    fn empty_topics_are_bare_markers() {
        let tree = Node::heading(1, "").with(vec![Node::new("").with(vec![Node::new("x")])]);
        assert_eq!(serialize(&tree), "#\n\n-\n  - x\n");
        assert_eq!(parse(&serialize(&tree), "x"), tree);
    }

    #[test]
    fn typed_text_that_would_change_meaning_is_escaped() {
        let item = Kind::default();
        assert_eq!(safe_text(&item, "plain **text**"), "plain **text**");
        assert_eq!(safe_text(&item, "# not a heading"), "\\# not a heading");
        assert_eq!(safe_text(&item, "- not a sub-list"), "\\- not a sub-list");
        assert_eq!(safe_text(&item, "1. not a list"), "1\\. not a list");
        assert_eq!(safe_text(&item, "> not a quote"), "\\> not a quote");
        let star = Kind::Item(ItemStyle {
            marker: Marker::Bullet('*'),
            ..ItemStyle::default()
        });
        assert_eq!(safe_text(&star, "**"), "\\**", "`* **` would be a rule");
        let heading = Kind::Heading { level: 2 };
        assert_eq!(safe_text(&heading, "C #"), "C \\#");
        assert_eq!(safe_text(&heading, "# fine"), "# fine");
        assert_eq!(safe_text(&heading, "C# and F#"), "C# and F#");
    }
}
