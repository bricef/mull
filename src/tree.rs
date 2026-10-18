//! The mind map: a tree of nodes, each a kind of Markdown, addressed by
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
    /// A section heading, with the level it was read or made with.
    Heading { level: u8 },
    /// A list item: a topic whose text is the item's first line.
    Item(ItemStyle),
    /// Content kept exactly as written: a leaf that mull moves but doesn't
    /// edit.
    Block(BlockKind),
}

impl Default for Kind {
    fn default() -> Self {
        Kind::Item(ItemStyle::default())
    }
}

/// How a list item is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStyle {
    pub marker: Marker,
    /// The task box: `Some(false)` for `[ ]`, `Some(true)` for `[x]`.
    pub checked: Option<bool>,
    /// Whether its list has blank lines between items.
    pub loose: bool,
}

impl Default for ItemStyle {
    fn default() -> Self {
        ItemStyle {
            marker: Marker::Bullet('-'),
            checked: None,
            loose: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// `-`, `*` or `+`.
    Bullet(char),
    /// `1.` or `1)`. Lists are renumbered from their first item's number.
    Ordered { number: u32, delimiter: char },
}

impl Marker {
    /// Whether an item with this marker continues a list of `other`s.
    pub fn same_list(self, other: Marker) -> bool {
        match (self, other) {
            (Marker::Bullet(a), Marker::Bullet(b)) => a == b,
            (Marker::Ordered { delimiter: a, .. }, Marker::Ordered { delimiter: b, .. }) => a == b,
            _ => false,
        }
    }
}

/// What a block of content is. It decides only how the block is shown and
/// where it may go; its source is kept verbatim whatever the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Code,
    Table,
    Quote,
    Html,
    Maths,
    FrontMatter,
    Definition,
    Footnote,
    Rule,
    /// Anything else, such as a heading inside a list item.
    Other,
}

/// One node and everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub kind: Kind,
    /// For topics, one line of raw inline Markdown with no surrounding
    /// whitespace (see [`normalize_text`]). For blocks, the exact Markdown
    /// source, any number of lines. For the document, the file's name.
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
    SectionInList,
    ContentAfterSection,
    TooDeep,
    FrontMatterNotFirst,
    NestedDocument,
    ParagraphUnderEmptyItem,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Problem::BlockWithChildren => "Content blocks can't hold topics",
            Problem::SectionInList => "A section can't go inside a list item",
            Problem::ContentAfterSection => "Content has to come before a section's sub-sections",
            Problem::TooDeep => "Sections only nest six deep",
            Problem::FrontMatterNotFirst => "Front matter has to stay at the top of the file",
            Problem::NestedDocument => "Only the root can be the whole document",
            Problem::ParagraphUnderEmptyItem => {
                "An item with no text can't start with a paragraph: it would become the text"
            }
        })
    }
}

/// Reduce arbitrary input to text a topic can hold: a single trimmed line,
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

impl Node {
    /// A plain list item.
    pub fn new(text: &str) -> Self {
        Node::item(ItemStyle::default(), text)
    }

    /// A plain list item holding `children`.
    pub fn with_children(text: &str, children: Vec<Node>) -> Self {
        Node::new(text).with(children)
    }

    pub fn item(style: ItemStyle, text: &str) -> Self {
        Node {
            kind: Kind::Item(style),
            text: normalize_text(text),
            ..Node::default()
        }
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

    /// What the map shows for this node: a topic's text (after its task box,
    /// if it has one), a paragraph joined onto one line, or another block's
    /// first line.
    pub fn label(&self) -> String {
        match self.kind {
            Kind::Block(BlockKind::Paragraph) => join_lines(&self.text),
            Kind::Block(_) => self.text.lines().next().unwrap_or("").trim().to_string(),
            Kind::Item(ItemStyle {
                checked: Some(done),
                ..
            }) => {
                format!("[{}] {}", if done { "x" } else { " " }, self.text)
            }
            _ => self.text.clone(),
        }
    }

    /// How many lines a block has beyond the one its label shows.
    pub fn hidden_lines(&self) -> usize {
        match self.kind {
            Kind::Block(BlockKind::Paragraph)
            | Kind::Document
            | Kind::Heading { .. }
            | Kind::Item(_) => 0,
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

    /// Whether the node at `path` exists and is on screen: no ancestor of it
    /// is folded.
    pub fn is_visible(&self, path: &[usize]) -> bool {
        self.get(path).is_some()
            && (0..path.len()).all(|depth| self.get(&path[..depth]).is_some_and(|n| !n.collapsed))
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
            _ => 1,
        };
        self.check_children(level, true)
    }

    fn check_children(&self, level: u8, is_root: bool) -> Result<(), Problem> {
        let section = is_root || matches!(self.kind, Kind::Heading { .. } | Kind::Document);
        let mut ceiling = 6;
        let mut seen_heading = false;
        for (i, child) in self.children.iter().enumerate() {
            match child.kind {
                Kind::Document => return Err(Problem::NestedDocument),
                Kind::Block(_) if !child.children.is_empty() => {
                    return Err(Problem::BlockWithChildren);
                }
                Kind::Block(BlockKind::FrontMatter)
                    if !(is_root && self.kind == Kind::Document && i == 0) =>
                {
                    return Err(Problem::FrontMatterNotFirst);
                }
                Kind::Heading { level: stored } => {
                    if !section {
                        return Err(Problem::SectionInList);
                    }
                    let written = written_level(level, stored, ceiling);
                    if written > 6 {
                        return Err(Problem::TooDeep);
                    }
                    ceiling = written;
                    seen_heading = true;
                    child.check_children(written, false)?;
                }
                Kind::Item(_) | Kind::Block(_) => {
                    if seen_heading {
                        return Err(Problem::ContentAfterSection);
                    }
                    let starts_with_paragraph = child
                        .children
                        .first()
                        .is_some_and(|c| c.kind == Kind::Block(BlockKind::Paragraph));
                    if matches!(child.kind, Kind::Item(_))
                        && child.text.is_empty()
                        && starts_with_paragraph
                    {
                        return Err(Problem::ParagraphUnderEmptyItem);
                    }
                    child.check_children(level, false)?;
                }
            }
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

    /// A section with content, then two sub-sections.
    fn sections() -> Node {
        Node::heading(1, "Root").with(vec![
            Node::new("item"),
            Node::block(BlockKind::Paragraph, "Some prose."),
            Node::heading(2, "A").with(vec![Node::new("a1")]),
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
    fn top_level_topics_cannot_outdent() {
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
    fn new_content_in_a_section_goes_before_its_sub_sections() {
        let mut tree = sections();
        assert_eq!(tree.append(&[], Node::new("new")), Some(vec![2]));
        assert_eq!(texts(&tree), ["item", "Some prose.", "new", "A", "B"]);
        assert_eq!(tree.append(&[], Node::heading(2, "C")), Some(vec![5]));
        assert!(tree.check().is_ok());
    }

    #[test]
    fn outdenting_out_of_a_sub_section_lands_before_the_sub_sections() {
        let mut tree = sections();
        assert_eq!(tree.outdent(&[2, 0]), Some(vec![2]));
        assert_eq!(texts(&tree), ["item", "Some prose.", "a1", "A", "B"]);
        assert!(tree.check().is_ok());
    }

    #[test]
    fn indenting_a_section_makes_it_the_last_sub_section_above() {
        let mut tree = Node::heading(1, "Root").with(vec![
            Node::heading(2, "A").with(vec![Node::new("a1"), Node::heading(3, "A.1")]),
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

        let section_in_list =
            Node::heading(1, "R").with(vec![Node::new("item").with(vec![Node::heading(2, "H")])]);
        assert_eq!(section_in_list.check(), Err(Problem::SectionInList));

        let mut content_after = sections();
        content_after.children.swap(1, 2);
        assert_eq!(content_after.check(), Err(Problem::ContentAfterSection));

        let mut deep = Node::heading(1, "1");
        for level in (2..=7).rev() {
            deep = Node::heading(level, &level.to_string()).with(vec![deep]);
        }
        let deep = Node::heading(1, "root").with(vec![deep]);
        assert_eq!(deep.check(), Err(Problem::TooDeep));

        let late_front_matter = Node::document("d").with(vec![
            Node::new("x"),
            Node::block(BlockKind::FrontMatter, "---\na: b\n---"),
        ]);
        assert_eq!(late_front_matter.check(), Err(Problem::FrontMatterNotFirst));

        let nested = Node::heading(1, "R").with(vec![Node::document("d")]);
        assert_eq!(nested.check(), Err(Problem::NestedDocument));

        let empty_item = Node::heading(1, "R").with(vec![
            Node::new("").with(vec![Node::block(BlockKind::Paragraph, "text")]),
        ]);
        assert_eq!(empty_item.check(), Err(Problem::ParagraphUnderEmptyItem));

        assert!(sections().check().is_ok());
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
    fn tasks_show_their_box() {
        let todo = Node::item(
            ItemStyle {
                checked: Some(false),
                ..ItemStyle::default()
            },
            "milk",
        );
        let done = Node::item(
            ItemStyle {
                checked: Some(true),
                ..ItemStyle::default()
            },
            "eggs",
        );
        assert_eq!(
            (todo.label(), done.label()),
            ("[ ] milk".to_string(), "[x] eggs".to_string())
        );
    }

    #[test]
    fn blocks_show_a_line_and_count_the_rest() {
        let code = Node::block(BlockKind::Code, "```rust\nfn main() {}\n```");
        assert_eq!(
            (code.label(), code.hidden_lines()),
            ("```rust".to_string(), 2)
        );
        let prose = Node::block(BlockKind::Paragraph, "One line\nand another.");
        assert_eq!(
            (prose.label(), prose.hidden_lines()),
            ("One line and another.".to_string(), 0)
        );
    }
}
