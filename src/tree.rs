//! The map: a tree of sections holding blocks of content, addressed by
//! paths. See docs/design.md, "The model".

use std::fmt;

/// Where a node sits: the child index to take at each level, starting from
/// the root. The empty path is the root itself.
pub type Path = Vec<usize>;

/// What a node is in the Markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The whole file, when it isn't one heading holding everything. Its text
    /// is the file's name and is never written.
    Document,
    /// A section: a heading and everything under it. The level is the one it
    /// was read or made with; where it sits in the tree decides the level it
    /// is written at.
    Heading { level: u8 },
    /// Content kept exactly as written, lists included: a leaf that mull
    /// moves but doesn't edit.
    Block(BlockKind),
}

impl Default for Kind {
    fn default() -> Self {
        Kind::Heading { level: 1 }
    }
}

/// What a block of content is. It decides only how the block is shown and
/// where it may go; its source is kept verbatim whatever the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    List,
    Code,
    Table,
    Quote,
    Html,
    Maths,
    Definition,
    Footnote,
    Rule,
    /// Anything else markdown-rs reads, or a file it can't read at all.
    Other,
}

/// One node and everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub kind: Kind,
    /// For a section, its heading: one line of raw inline Markdown with no
    /// surrounding whitespace (see [`normalize_text`]). For a block, its
    /// exact Markdown source, any number of lines. For the document, the
    /// file's name.
    pub text: String,
    pub children: Vec<Node>,
    /// Folded nodes hide their children on screen. This is view state: it
    /// isn't written to the file.
    pub collapsed: bool,
}

/// Why a change was refused: the tree it would make can't be written as
/// Markdown and read back the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    BlockWithChildren,
    ContentAfterSection,
    TooDeep,
    NestedDocument,
    ListsWouldMerge,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Problem::BlockWithChildren => "Content blocks can't hold sections",
            Problem::ContentAfterSection => "Content has to come before a section's sub-sections",
            Problem::TooDeep => "Sections only nest six deep",
            Problem::NestedDocument => "Only the root can be the whole document",
            Problem::ListsWouldMerge => "Two lists side by side would read back as one list",
        })
    }
}

/// Reduce arbitrary input to text a heading can hold: a single trimmed line,
/// with control characters (newlines and tabs included) turned into spaces.
pub fn normalize_text(text: &str) -> String {
    let line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    line.trim().to_string()
}

/// Lines joined into one with single spaces, the way a soft line break
/// reads.
pub fn join_lines(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    normalize_text(&lines.join(" "))
}

/// The level a heading is written at, given its parent's level (0 for the
/// document) and the lowest level written so far among its sibling headings.
/// Its own level is kept unless it has to go deeper than its parent, or no
/// deeper than the siblings before it so that it still closes them.
pub fn written_level(parent: u8, stored: u8, ceiling: u8) -> u8 {
    (parent + 1).max(stored.min(ceiling))
}

/// The path of child `index` under `parent`.
pub fn child_path(parent: &[usize], index: usize) -> Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

/// How a list's first marker continues another list: lists with the same
/// bullet, or numbered with the same delimiter, are one list in Markdown
/// unless something else stands between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListMarker {
    Bullet(char),
    Numbered(char),
}

fn list_marker(source: &str) -> Option<ListMarker> {
    let source = source.trim_start_matches([' ', '\t']);
    let first = source.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        return Some(ListMarker::Bullet(first));
    }
    let digits = source.bytes().take_while(u8::is_ascii_digit).count();
    let delimiter = source[digits..].chars().next()?;
    (digits > 0 && matches!(delimiter, '.' | ')')).then_some(ListMarker::Numbered(delimiter))
}

impl Node {
    /// A section titled `text`. Its level is decided by where it sits when
    /// written.
    pub fn new(text: &str) -> Self {
        Node::heading(1, text)
    }

    /// A section titled `text` holding `children`.
    pub fn with_children(text: &str, children: Vec<Node>) -> Self {
        Node::new(text).with(children)
    }

    pub fn heading(level: u8, text: &str) -> Self {
        Node {
            kind: Kind::Heading { level },
            text: normalize_text(text),
            ..Node::default()
        }
    }

    pub fn document(name: &str) -> Self {
        Node {
            kind: Kind::Document,
            text: normalize_text(name),
            ..Node::default()
        }
    }

    /// A block holding `source` verbatim.
    pub fn block(kind: BlockKind, source: &str) -> Self {
        Node {
            kind: Kind::Block(kind),
            text: source.to_string(),
            ..Node::default()
        }
    }

    /// This node with `children`.
    pub fn with(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }

    pub fn is_block(&self) -> bool {
        matches!(self.kind, Kind::Block(_))
    }

    pub fn is_heading(&self) -> bool {
        matches!(self.kind, Kind::Heading { .. })
    }

    /// What the map shows for this node: a section's heading, a paragraph
    /// joined onto one line, or another block's first line.
    pub fn label(&self) -> String {
        match self.kind {
            Kind::Block(BlockKind::Paragraph) => join_lines(&self.text),
            Kind::Block(_) => self.text.lines().next().unwrap_or("").trim().to_string(),
            Kind::Document | Kind::Heading { .. } => self.text.clone(),
        }
    }

    /// How many lines a block has beyond the one its label shows.
    pub fn hidden_lines(&self) -> usize {
        match self.kind {
            Kind::Block(BlockKind::Paragraph) | Kind::Document | Kind::Heading { .. } => 0,
            Kind::Block(_) => self.text.lines().count().saturating_sub(1),
        }
    }

    pub fn get(&self, path: &[usize]) -> Option<&Node> {
        path.iter().try_fold(self, |node, &i| node.children.get(i))
    }

    pub fn get_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        path.iter()
            .try_fold(self, |node, &i| node.children.get_mut(i))
    }

    /// Number of nodes in this subtree, this one included.
    pub fn size(&self) -> usize {
        1 + self.children.iter().map(Node::size).sum::<usize>()
    }

    /// Whether the node at `path` exists and no ancestor of it is folded.
    pub fn is_visible(&self, path: &[usize]) -> bool {
        self.get(path).is_some()
            && (0..path.len()).all(|depth| self.get(&path[..depth]).is_some_and(|n| !n.collapsed))
    }

    /// Whether the node at `path` is on screen: visible, and not a block
    /// while blocks are hidden.
    pub fn is_shown(&self, path: &[usize], show_blocks: bool) -> bool {
        self.is_visible(path) && (show_blocks || self.get(path).is_some_and(|n| !n.is_block()))
    }

    /// Where a section's content ends and its sub-sections begin.
    fn content_end(&self) -> usize {
        self.children
            .iter()
            .position(Node::is_heading)
            .unwrap_or(self.children.len())
    }

    /// The nearest place to `index` among `children` where `node` keeps
    /// content ahead of sub-sections.
    fn place(&self, index: usize, node: &Node) -> usize {
        let index = index.min(self.children.len());
        if node.is_heading() {
            index.max(self.content_end())
        } else {
            index.min(self.content_end())
        }
    }

    /// Insert `node` as child `index` of `parent`, moved if need be to where
    /// content stays ahead of sub-sections. Returns the new node's path.
    pub fn insert(&mut self, parent: &[usize], index: usize, node: Node) -> Option<Path> {
        let parent_node = self.get_mut(parent)?;
        let index = parent_node.place(index, &node);
        parent_node.children.insert(index, node);
        Some(child_path(parent, index))
    }

    /// Insert `node` as the last child of `parent` that the rules allow.
    pub fn append(&mut self, parent: &[usize], node: Node) -> Option<Path> {
        self.insert(parent, usize::MAX, node)
    }

    /// Detach the node at `path`. The root can't be removed.
    pub fn remove(&mut self, path: &[usize]) -> Option<Node> {
        let (&index, parent) = path.split_last()?;
        let siblings = &mut self.get_mut(parent)?.children;
        (index < siblings.len()).then(|| siblings.remove(index))
    }

    /// Swap the node with the sibling above it. Returns its new path.
    pub fn move_up(&mut self, path: &[usize]) -> Option<Path> {
        let (&index, parent) = path.split_last()?;
        let siblings = &mut self.get_mut(parent)?.children;
        if index == 0 || index >= siblings.len() {
            return None;
        }
        siblings.swap(index - 1, index);
        Some(child_path(parent, index - 1))
    }

    /// Swap the node with the sibling below it. Returns its new path.
    pub fn move_down(&mut self, path: &[usize]) -> Option<Path> {
        let (&index, parent) = path.split_last()?;
        let siblings = &mut self.get_mut(parent)?.children;
        if index + 1 >= siblings.len() {
            return None;
        }
        siblings.swap(index, index + 1);
        Some(child_path(parent, index + 1))
    }

    /// Make the node the last child the sibling above it can take, unfolding
    /// that sibling. Returns the node's new path.
    pub fn indent(&mut self, path: &[usize]) -> Option<Path> {
        let (&index, parent) = path.split_last()?;
        if index == 0 || self.get(path).is_none() {
            return None;
        }
        let node = self.remove(path)?;
        let new_parent = child_path(parent, index - 1);
        self.get_mut(&new_parent)?.collapsed = false;
        self.append(&new_parent, node)
    }

    /// Make the node a sibling of its parent, just after it where the rules
    /// allow. Children of the root stay put: a map has a single root.
    /// Returns the node's new path.
    pub fn outdent(&mut self, path: &[usize]) -> Option<Path> {
        if path.len() < 2 || self.get(path).is_none() {
            return None;
        }
        let (&parent_index, grandparent) = path[..path.len() - 1].split_last()?;
        let node = self.remove(path)?;
        self.insert(grandparent, parent_index + 1, node)
    }

    /// Check this tree, taken as a root, against the rules that let it be
    /// written as Markdown and read back the same (docs/design.md, "Trees
    /// that Markdown can say").
    pub fn check(&self) -> Result<(), Problem> {
        let level = match self.kind {
            Kind::Document => 0,
            Kind::Heading { level } => level.clamp(1, 6),
            Kind::Block(_) => 1,
        };
        self.check_children(level)
    }

    fn check_children(&self, level: u8) -> Result<(), Problem> {
        let mut ceiling = 6;
        let mut seen_heading = false;
        let mut previous_list: Option<ListMarker> = None;
        for child in &self.children {
            let list = match child.kind {
                Kind::Block(BlockKind::List) => list_marker(&child.text),
                _ => None,
            };
            match child.kind {
                Kind::Document => return Err(Problem::NestedDocument),
                Kind::Heading { level: stored } => {
                    let written = written_level(level, stored, ceiling);
                    if written > 6 {
                        return Err(Problem::TooDeep);
                    }
                    ceiling = written;
                    seen_heading = true;
                    child.check_children(written)?;
                }
                Kind::Block(_) => {
                    if !child.children.is_empty() {
                        return Err(Problem::BlockWithChildren);
                    }
                    if seen_heading {
                        return Err(Problem::ContentAfterSection);
                    }
                    if list.is_some() && list == previous_list {
                        return Err(Problem::ListsWouldMerge);
                    }
                }
            }
            previous_list = list;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Node {
        Node::with_children(
            "root",
            vec![
                Node::with_children("a", vec![Node::new("a1"), Node::new("a2")]),
                Node::new("b"),
            ],
        )
    }

    fn paragraph(text: &str) -> Node {
        Node::block(BlockKind::Paragraph, text)
    }

    /// A section with content, then two sub-sections.
    fn sections() -> Node {
        Node::heading(1, "Root").with(vec![
            paragraph("Some prose."),
            Node::block(BlockKind::List, "- one\n- two"),
            Node::heading(2, "A").with(vec![paragraph("a1")]),
            Node::heading(2, "B"),
        ])
    }

    fn texts(node: &Node) -> Vec<&str> {
        node.children.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn normalizing_makes_one_trimmed_line() {
        assert_eq!(normalize_text("  a\tb\nc \r"), "a b c");
        assert_eq!(join_lines("a\n   b\n\nc"), "a b c");
    }

    #[test]
    fn get_follows_child_indices() {
        let tree = sample();
        assert_eq!(tree.get(&[0, 1]).map(|n| n.text.as_str()), Some("a2"));
        assert!(tree.get(&[2]).is_none());
    }

    #[test]
    fn indent_moves_under_the_sibling_above() {
        let mut tree = sample();
        assert_eq!(tree.indent(&[1]), Some(vec![0, 2]));
        assert_eq!(texts(&tree.children[0]), ["a1", "a2", "b"]);
    }

    #[test]
    fn indent_needs_a_sibling_above() {
        let mut tree = sample();
        assert_eq!(tree.indent(&[0]), None);
        assert_eq!(tree, sample());
    }

    #[test]
    fn outdent_moves_after_the_parent() {
        let mut tree = sample();
        assert_eq!(tree.outdent(&[0, 0]), Some(vec![1]));
        assert_eq!(texts(&tree), ["a", "a1", "b"]);
    }

    #[test]
    fn top_level_nodes_cannot_outdent() {
        let mut tree = sample();
        assert_eq!(tree.outdent(&[0]), None);
        assert_eq!(tree, sample());
    }

    #[test]
    fn moving_swaps_siblings_within_bounds() {
        let mut tree = sample();
        assert_eq!(tree.move_down(&[0]), Some(vec![1]));
        assert_eq!(texts(&tree), ["b", "a"]);
        assert_eq!(tree.move_down(&[1]), None);
        assert_eq!(tree.move_up(&[1]), Some(vec![0]));
        assert_eq!(tree.move_up(&[0]), None);
        assert_eq!(tree, sample());
    }

    #[test]
    fn the_root_cannot_be_removed() {
        let mut tree = sample();
        assert!(tree.remove(&[]).is_none());
    }

    #[test]
    fn folded_ancestors_hide_a_node() {
        let mut tree = sample();
        tree.children[0].collapsed = true;
        assert!(tree.is_visible(&[0]));
        assert!(!tree.is_visible(&[0, 1]));
        assert!(!tree.is_visible(&[5]));
    }

    #[test]
    fn hiding_blocks_hides_only_blocks() {
        let tree = sections();
        assert!(tree.is_shown(&[0], true));
        assert!(!tree.is_shown(&[0], false));
        assert!(tree.is_shown(&[2], false));
    }

    #[test]
    fn new_content_goes_before_sub_sections_and_new_sections_after() {
        let mut tree = sections();
        assert_eq!(tree.append(&[], paragraph("new")), Some(vec![2]));
        assert_eq!(
            texts(&tree),
            ["Some prose.", "- one\n- two", "new", "A", "B"]
        );
        assert_eq!(tree.append(&[], Node::heading(2, "C")), Some(vec![5]));
        assert!(tree.check().is_ok());
    }

    #[test]
    fn outdenting_out_of_a_sub_section_lands_before_the_sub_sections() {
        let mut tree = sections();
        assert_eq!(tree.outdent(&[2, 0]), Some(vec![2]));
        assert_eq!(
            texts(&tree),
            ["Some prose.", "- one\n- two", "a1", "A", "B"]
        );
        assert!(tree.check().is_ok());
    }

    #[test]
    fn indenting_a_section_makes_it_the_last_sub_section_above() {
        let mut tree = Node::heading(1, "Root").with(vec![
            Node::heading(2, "A").with(vec![paragraph("a1"), Node::heading(3, "A.1")]),
            Node::heading(2, "B"),
        ]);
        assert_eq!(tree.indent(&[1]), Some(vec![0, 2]));
        assert_eq!(texts(&tree.children[0]), ["a1", "A.1", "B"]);
        assert!(tree.check().is_ok());
    }

    #[test]
    fn the_rules_name_what_markdown_cannot_say() {
        let block_with_child = Node::heading(1, "R").with(vec![
            Node::block(BlockKind::Code, "```\nx\n```").with(vec![Node::new("child")]),
        ]);
        assert_eq!(block_with_child.check(), Err(Problem::BlockWithChildren));

        let mut content_after = sections();
        content_after.children.swap(1, 2);
        assert_eq!(content_after.check(), Err(Problem::ContentAfterSection));

        let mut deep = Node::heading(1, "1");
        for level in (2..=7).rev() {
            deep = Node::heading(level, &level.to_string()).with(vec![deep]);
        }
        let deep = Node::heading(1, "root").with(vec![deep]);
        assert_eq!(deep.check(), Err(Problem::TooDeep));

        let nested = Node::heading(1, "R").with(vec![Node::document("d")]);
        assert_eq!(nested.check(), Err(Problem::NestedDocument));

        let merging = Node::heading(1, "R").with(vec![
            Node::block(BlockKind::List, "- a"),
            Node::block(BlockKind::List, "- b"),
        ]);
        assert_eq!(merging.check(), Err(Problem::ListsWouldMerge));

        assert!(sections().check().is_ok());
    }

    #[test]
    fn only_lists_that_continue_each_other_would_merge() {
        let pair = |a: &str, b: &str| {
            Node::heading(1, "R")
                .with(vec![
                    Node::block(BlockKind::List, a),
                    Node::block(BlockKind::List, b),
                ])
                .check()
        };
        assert_eq!(pair("1. a", "3. b"), Err(Problem::ListsWouldMerge));
        assert!(pair("- a", "* b").is_ok());
        assert!(pair("1. a", "1) b").is_ok());
        assert!(pair("- a", "1. b").is_ok());
        let apart = Node::heading(1, "R").with(vec![
            Node::block(BlockKind::List, "- a"),
            paragraph("between"),
            Node::block(BlockKind::List, "- b"),
        ]);
        assert!(apart.check().is_ok());
    }

    #[test]
    fn heading_levels_follow_the_tree() {
        // Kept when it fits; raised below a deeper parent; lowered to close
        // an earlier, shallower sibling.
        assert_eq!(written_level(1, 3, 6), 3);
        assert_eq!(written_level(2, 1, 6), 3);
        assert_eq!(written_level(1, 4, 2), 2);
    }

    #[test]
    fn blocks_show_a_line_and_count_the_rest() {
        let code = Node::block(BlockKind::Code, "```rust\nfn main() {}\n```");
        assert_eq!(
            (code.label(), code.hidden_lines()),
            ("```rust".to_string(), 2)
        );
        let list = Node::block(BlockKind::List, "- one\n- two");
        assert_eq!(
            (list.label(), list.hidden_lines()),
            ("- one".to_string(), 1)
        );
        let prose = Node::block(BlockKind::Paragraph, "One line\nand another.");
        assert_eq!(
            (prose.label(), prose.hidden_lines()),
            ("One line and another.".to_string(), 0)
        );
    }
}
