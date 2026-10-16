//! The mind map: a tree of one-line topics, addressed by paths.

/// Where a node sits: the child index to take at each level, starting from
/// the root. The empty path is the root itself.
pub type Path = Vec<usize>;

/// One topic and everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    /// One line of raw inline Markdown with no surrounding whitespace. See
    /// [`normalize_text`].
    pub text: String,
    pub children: Vec<Node>,
    /// Folded nodes hide their children on screen. This is view state: it
    /// isn't written to the file.
    pub collapsed: bool,
}

/// Reduce arbitrary input to text a node can hold: a single trimmed line,
/// with control characters (newlines and tabs included) turned into spaces.
pub fn normalize_text(text: &str) -> String {
    let line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    line.trim().to_string()
}

/// The path of child `index` under `parent`.
pub fn child_path(parent: &[usize], index: usize) -> Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

impl Node {
    pub fn new(text: &str) -> Self {
        Node {
            text: normalize_text(text),
            children: Vec::new(),
            collapsed: false,
        }
    }

    pub fn with_children(text: &str, children: Vec<Node>) -> Self {
        Node {
            children,
            ..Node::new(text)
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

    /// Insert `node` as child `index` of `parent`, or as its last child if
    /// `index` is past the end. Returns the new node's path.
    pub fn insert(&mut self, parent: &[usize], index: usize, node: Node) -> Option<Path> {
        let siblings = &mut self.get_mut(parent)?.children;
        let index = index.min(siblings.len());
        siblings.insert(index, node);
        Some(child_path(parent, index))
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

    /// Make the node the last child of the sibling above it, unfolding that
    /// sibling. Returns the node's new path.
    pub fn indent(&mut self, path: &[usize]) -> Option<Path> {
        let (&index, parent) = path.split_last()?;
        if index == 0 || self.get(path).is_none() {
            return None;
        }
        let node = self.remove(path)?;
        let new_parent = child_path(parent, index - 1);
        let target = self.get_mut(&new_parent)?;
        target.collapsed = false;
        target.children.push(node);
        let at = target.children.len() - 1;
        Some(child_path(&new_parent, at))
    }

    /// Make the node the sibling just after its parent. Children of the root
    /// stay put: a map has a single root. Returns the node's new path.
    pub fn outdent(&mut self, path: &[usize]) -> Option<Path> {
        if path.len() < 2 || self.get(path).is_none() {
            return None;
        }
        let (&parent_index, grandparent) = path[..path.len() - 1].split_last()?;
        let node = self.remove(path)?;
        self.insert(grandparent, parent_index + 1, node)
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

    fn texts(node: &Node) -> Vec<&str> {
        node.children.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn normalizing_makes_one_trimmed_line() {
        assert_eq!(normalize_text("  a\tb\nc \r"), "a b c");
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
}
