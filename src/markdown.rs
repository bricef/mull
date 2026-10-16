//! Maps on disk are Markdown.
//!
//! mull writes one layout, which also renders as a mind map in Markmap:
//!
//! ```markdown
//! # Root topic
//!
//! - Child
//!   - Grandchild
//! - Another child
//! ```
//!
//! It reads leniently, so ordinary notes open as maps too:
//!
//! * a heading is a child of the nearest heading above it with a lower level;
//! * any other non-blank line, list item or not, is a child of the nearest
//!   line above it in the same heading section that is indented less, or
//!   failing that of the section's heading;
//! * list markers (`-`, `*`, `+`, `1.`, `1)`) are dropped from topic text;
//! * a document with several top-level topics gets a root named after the
//!   file.
//!
//! Every non-blank line becomes exactly one topic, so opening and saving a
//! file never loses text, though it may reshape it into the layout above.
//! A tab indents to the next multiple of four columns, which makes h-m-m's
//! tab-indented files open as the outlines they are.

use crate::tree::Node;

/// Write `root` in mull's layout.
pub fn serialize(root: &Node) -> String {
    let mut out = String::new();
    push_line(&mut out, "#", &root.text);
    if !root.children.is_empty() {
        out.push('\n');
    }
    for child in &root.children {
        push_item(&mut out, child, 0);
    }
    out
}

fn push_item(out: &mut String, node: &Node, depth: usize) {
    out.push_str(&"  ".repeat(depth));
    push_line(out, "-", &node.text);
    for child in &node.children {
        push_item(out, child, depth + 1);
    }
}

fn push_line(out: &mut String, marker: &str, text: &str) {
    out.push_str(marker);
    if !text.is_empty() {
        out.push(' ');
        out.push_str(text);
    }
    out.push('\n');
}

/// What opened a node, which decides what closes it.
enum Opener {
    /// A heading of this level (1 to 6).
    Heading(usize),
    /// Any other line, indented this many columns.
    Line(usize),
}

impl Opener {
    /// Whether a `next` line ends this node, so that `next` can't be its child.
    fn closed_by(&self, next: &Opener) -> bool {
        match (self, next) {
            (Opener::Heading(open), Opener::Heading(level)) => open >= level,
            (Opener::Heading(_), Opener::Line(_)) => false,
            (Opener::Line(_), Opener::Heading(_)) => true,
            (Opener::Line(open), Opener::Line(indent)) => open >= indent,
        }
    }
}

/// Read a map from Markdown. `fallback_root` names the root when the
/// document doesn't have exactly one top-level topic.
pub fn parse(input: &str, fallback_root: &str) -> Node {
    let mut top_level = Vec::new();
    let mut open: Vec<(Opener, Node)> = Vec::new();
    for line in input.lines() {
        let Some((opener, text)) = classify(line) else {
            continue;
        };
        while open.last().is_some_and(|(o, _)| o.closed_by(&opener)) {
            close(&mut open, &mut top_level);
        }
        open.push((opener, Node::new(text)));
    }
    while !open.is_empty() {
        close(&mut open, &mut top_level);
    }
    if top_level.len() == 1 {
        top_level.swap_remove(0)
    } else {
        Node::with_children(fallback_root, top_level)
    }
}

/// Finish the innermost open node by attaching it to its parent.
fn close(open: &mut Vec<(Opener, Node)>, top_level: &mut Vec<Node>) {
    if let Some((_, node)) = open.pop() {
        match open.last_mut() {
            Some((_, parent)) => parent.children.push(node),
            None => top_level.push(node),
        }
    }
}

/// What a line opens and its topic text, or `None` for a blank line.
fn classify(line: &str) -> Option<(Opener, &str)> {
    let (indent, rest) = split_indent(line);
    if rest.trim().is_empty() {
        return None;
    }
    if indent < 4
        && let Some((level, text)) = heading(rest)
    {
        return Some((Opener::Heading(level), text));
    }
    Some((Opener::Line(indent), strip_marker(rest)))
}

/// The line's indentation in columns, and the text after it.
fn split_indent(line: &str) -> (usize, &str) {
    let mut column = 0;
    for (i, c) in line.char_indices() {
        match c {
            ' ' => column += 1,
            '\t' => column += 4 - column % 4,
            _ => return (column, &line[i..]),
        }
    }
    (column, "")
}

/// `#` to `######` followed by a space, a tab or nothing.
fn heading(rest: &str) -> Option<(usize, &str)> {
    let level = rest.bytes().take_while(|&b| b == b'#').count();
    let text = &rest[level..];
    ((1..=6).contains(&level) && gap_or_end(text)).then_some((level, text))
}

/// The text after a bullet or ordered-list marker, or the whole line if it
/// doesn't start with one.
fn strip_marker(rest: &str) -> &str {
    let ordered = || {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        (1..=9)
            .contains(&digits)
            .then(|| rest[digits..].strip_prefix(['.', ')']))
            .flatten()
    };
    match rest.strip_prefix(['-', '*', '+']).or_else(ordered) {
        Some(text) if gap_or_end(text) => text,
        _ => rest,
    }
}

fn gap_or_end(text: &str) -> bool {
    text.is_empty() || text.starts_with([' ', '\t'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The tree as an indented outline, for compact expectations.
    fn outline(node: &Node) -> String {
        fn walk(node: &Node, depth: usize, out: &mut String) {
            out.push_str(&"  ".repeat(depth));
            out.push_str(&node.text);
            out.push('\n');
            for child in &node.children {
                walk(child, depth + 1, out);
            }
        }
        let mut out = String::new();
        walk(node, 0, &mut out);
        out
    }

    #[test]
    fn writes_the_root_as_a_heading_and_the_rest_as_a_nested_list() {
        let tree = Node::with_children(
            "Plan",
            vec![
                Node::with_children("Goals", vec![Node::new("fast")]),
                Node::new("Risks"),
            ],
        );
        assert_eq!(serialize(&tree), "# Plan\n\n- Goals\n  - fast\n- Risks\n");
    }

    #[test]
    fn a_lone_root_is_one_line() {
        assert_eq!(serialize(&Node::new("Idea")), "# Idea\n");
    }

    #[test]
    fn empty_topics_are_bare_markers() {
        let tree = Node::with_children("", vec![Node::new("")]);
        assert_eq!(serialize(&tree), "#\n\n-\n");
        assert_eq!(parse("#\n\n-\n", "x"), tree);
    }

    #[test]
    fn reads_its_own_layout() {
        let doc = "# Plan\n\n- Goals\n  - fast\n- Risks\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            "Plan\n  Goals\n    fast\n  Risks\n"
        );
    }

    #[test]
    fn headings_nest_by_level() {
        let doc = "# Project\n## Goals\n- fast\n- small\n## Risks\n- scope\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            "Project\n  Goals\n    fast\n    small\n  Risks\n    scope\n"
        );
    }

    #[test]
    fn skipped_heading_levels_still_nest() {
        assert_eq!(outline(&parse("# A\n### B\n## C\n", "x")), "A\n  B\n  C\n");
    }

    #[test]
    fn a_heading_closes_the_list_above_it() {
        let doc = "# A\n- x\n  - y\n# B\n- z\n";
        assert_eq!(
            outline(&parse(doc, "notes")),
            "notes\n  A\n    x\n      y\n  B\n    z\n"
        );
    }

    #[test]
    fn tab_indented_plain_text_opens_as_an_outline() {
        let doc = "root\n\tchild a\n\t\tgrand\n\tchild b\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            "root\n  child a\n    grand\n  child b\n"
        );
    }

    #[test]
    fn list_markers_are_dropped() {
        let doc = "# R\n- dash\n* star\n+ plus\n1. one\n2) two\n-not a marker\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            "R\n  dash\n  star\n  plus\n  one\n  two\n  -not a marker\n"
        );
    }

    #[test]
    fn paragraphs_become_topics() {
        let doc = "# R\n\nSome prose.\n\n- item\n";
        assert_eq!(outline(&parse(doc, "x")), "R\n  Some prose.\n  item\n");
    }

    #[test]
    fn several_top_level_topics_share_a_root_named_after_the_file() {
        assert_eq!(outline(&parse("- a\n- b\n", "ideas")), "ideas\n  a\n  b\n");
    }

    #[test]
    fn an_empty_file_is_just_a_root() {
        assert_eq!(parse("", "ideas"), Node::new("ideas"));
        assert_eq!(parse("\n   \n\t\n", "ideas"), Node::new("ideas"));
    }

    #[test]
    fn hashes_are_text_unless_they_make_a_heading() {
        let doc = "# R\n    # indented too far\n- #tag\n";
        assert_eq!(
            outline(&parse(doc, "x")),
            "R\n  # indented too far\n  #tag\n"
        );
        assert_eq!(outline(&parse("#tag\n", "x")), "#tag\n");
    }
}
