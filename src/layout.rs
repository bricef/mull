//! Where each topic goes on the canvas.
//!
//! The root sits at the left. Each node's children stack to its right, and
//! the node is centred on them vertically:
//!
//! ```text
//!       ┌─ A ─┬─ A1
//! Root ─┤     └─ A2
//!       ├─ B
//!       └─ C
//! ```
//!
//! Coordinates are in terminal cells, with the origin at the top left of
//! the whole map; the UI decides which part of it is on screen.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::tree::{Node, Path};

#[derive(Debug, Clone)]
pub struct Options {
    /// Widest a topic is shown before it's cut short with `…`.
    pub max_text_width: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_text_width: 40 }
    }
}

/// A topic's place on the canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub path: Path,
    pub x: usize,
    pub y: usize,
    /// The text as shown, possibly cut short. Never empty, so an empty topic
    /// still has a cell to highlight.
    pub label: String,
    /// A quiet note after the label: `[+N]` on a folded topic hiding N
    /// others, `[N lines]` on a block longer than its label.
    pub note: Option<String>,
}

impl Placed {
    pub fn label_width(&self) -> usize {
        self.label.width()
    }

    /// Columns taken by the label and fold marker together.
    pub fn width(&self) -> usize {
        self.label_width() + self.note.as_ref().map_or(0, |m| 1 + m.width())
    }
}

/// One box-drawing character of the lines joining parents to children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connector {
    pub x: usize,
    pub y: usize,
    pub ch: char,
    /// The first-level topic whose branch this is part of, by index; `None`
    /// for the root's own junction, which all branches share.
    pub branch: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// Every visible topic, in preorder.
    pub nodes: Vec<Placed>,
    pub connectors: Vec<Connector>,
    pub width: usize,
    pub height: usize,
}

/// Lay out every visible topic of `root`. `editing` replaces the shown text
/// of one topic, uncut, while it's being typed.
pub fn layout(root: &Node, options: &Options, editing: Option<(&[usize], &str)>) -> Layout {
    let mut placer = Placer {
        options,
        editing,
        out: Layout::default(),
    };
    let (_, height) = placer.place(root, &mut Vec::new(), 0, 0);
    let mut out = placer.out;
    out.height = height;
    out.width = out
        .nodes
        .iter()
        .map(|n| n.x + n.width())
        .chain(out.connectors.iter().map(|c| c.x + 1))
        .max()
        .unwrap_or(0);
    out
}

/// Columns between a parent's text and its children's: ` ─┬─ `.
const GAP: usize = 5;

struct Placer<'a> {
    options: &'a Options,
    editing: Option<(&'a [usize], &'a str)>,
    out: Layout,
}

impl Placer<'_> {
    /// Place `node` at column `x`, with its subtree starting at row `top`.
    /// Returns the node's row and the number of rows its subtree takes.
    fn place(&mut self, node: &Node, path: &mut Path, x: usize, top: usize) -> (usize, usize) {
        let label = match self.editing {
            Some((editing, text)) if editing == path.as_slice() => text.to_string(),
            _ => truncate(&node.label(), self.options.max_text_width),
        };
        let label = if label.is_empty() {
            " ".to_string()
        } else {
            label
        };
        let label_width = label.width();
        let folded = node.collapsed && !node.children.is_empty();
        let index = self.out.nodes.len();
        self.out.nodes.push(Placed {
            path: path.clone(),
            x,
            y: top,
            label,
            note: if folded {
                Some(format!("[+{}]", node.size() - 1))
            } else {
                let lines = node.hidden_lines();
                (lines > 0).then(|| format!("[{} lines]", lines + 1))
            },
        });
        if node.children.is_empty() || node.collapsed {
            return (top, 1);
        }

        let child_x = x + label_width + GAP;
        let mut rows = Vec::with_capacity(node.children.len());
        let mut next = top;
        for (i, child) in node.children.iter().enumerate() {
            path.push(i);
            let (row, height) = self.place(child, path, child_x, next);
            path.pop();
            rows.push(row);
            next += height;
        }

        let (first, last) = (rows[0], rows[rows.len() - 1]);
        let row = (first + last) / 2;
        self.out.nodes[index].y = row;
        let junction = x + label_width + 2;
        let branch = path.first().copied();
        self.out.connectors.push(Connector {
            x: junction - 1,
            y: row,
            ch: '─',
            branch,
        });
        let mut children = rows.iter().enumerate().peekable();
        for y in first..=last {
            let child = children.next_if(|&(_, &r)| r == y).map(|(i, _)| i);
            let ch = junction_char(y > first, y < last, y == row, child.is_some());
            self.out.connectors.push(Connector {
                x: junction,
                y,
                ch,
                branch,
            });
            if let Some(i) = child {
                // From the root, each stub starts the branch it leads to.
                self.out.connectors.push(Connector {
                    x: junction + 1,
                    y,
                    ch: '─',
                    branch: branch.or(Some(i)),
                });
            }
        }
        (row, next - top)
    }
}

/// The box-drawing character for a junction cell that connects up, down,
/// left (to the parent) and right (to a child) as asked. The top and bottom
/// cells of a junction always lead to a child.
fn junction_char(up: bool, down: bool, left: bool, right: bool) -> char {
    match (up, down, left, right) {
        (false, false, ..) => '─',
        (false, true, true, _) => '┬',
        (false, true, false, _) => '┌',
        (true, false, true, _) => '┴',
        (true, false, false, _) => '└',
        (true, true, true, true) => '┼',
        (true, true, true, false) => '┤',
        (true, true, false, true) => '├',
        (true, true, false, false) => '│',
    }
}

/// Cut `text` to at most `max` columns, ending in `…` if anything was cut.
fn truncate(text: &str, max: usize) -> String {
    let max = max.max(2);
    if text.width() <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut width = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if width + w + 1 > max {
            break;
        }
        width += w;
        out.push(c);
    }
    out.push('…');
    out
}

impl Layout {
    pub fn node(&self, path: &[usize]) -> Option<&Placed> {
        self.nodes.iter().find(|n| n.path == path)
    }

    /// The map as plain text, one line per row, trailing spaces trimmed.
    pub fn to_text(&self) -> String {
        // One string per cell; the second cell of a double-width character
        // is left empty.
        let mut grid = vec![vec![String::from(" "); self.width]; self.height];
        let mut put = |x: usize, y: usize, text: &str| {
            let mut x = x;
            for c in text.chars() {
                match c.width().unwrap_or(0) {
                    0 if x > 0 => grid[y][x - 1].push(c),
                    0 => {}
                    w => {
                        grid[y][x] = c.to_string();
                        for cell in &mut grid[y][x + 1..x + w] {
                            cell.clear();
                        }
                        x += w;
                    }
                }
            }
        };
        for c in &self.connectors {
            put(c.x, c.y, c.ch.encode_utf8(&mut [0; 4]));
        }
        for n in &self.nodes {
            put(n.x, n.y, &n.label);
            if let Some(marker) = &n.note {
                put(n.x + n.label_width() + 1, n.y, marker);
            }
        }
        grid.iter()
            .map(|row| row.concat().trim_end().to_string() + "\n")
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn sample() -> Node {
        Node::with_children(
            "Root",
            vec![
                Node::with_children("A", vec![Node::new("A1"), Node::new("A2")]),
                Node::new("B"),
                Node::new("C"),
            ],
        )
    }

    fn draw(tree: &Node) -> String {
        layout(tree, &Options::default(), None).to_text()
    }

    #[test]
    fn parents_are_centred_on_their_children() {
        assert_eq!(
            draw(&sample()),
            concat!(
                "      ┌─ A ─┬─ A1\n",
                "Root ─┤     └─ A2\n",
                "      ├─ B\n",
                "      └─ C\n",
            )
        );
    }

    #[test]
    fn folded_topics_count_what_they_hide() {
        let mut tree = sample();
        tree.children[0].collapsed = true;
        assert_eq!(
            draw(&tree),
            concat!("      ┌─ A [+2]\n", "Root ─┼─ B\n", "      └─ C\n",)
        );
    }

    #[test]
    fn an_only_child_sits_level_with_its_parent() {
        let tree = Node::with_children("Root", vec![Node::new("A")]);
        assert_eq!(draw(&tree), "Root ─── A\n");
    }

    #[test]
    fn two_children_hang_from_the_parent_row() {
        let tree = Node::with_children("Root", vec![Node::new("A"), Node::new("B")]);
        assert_eq!(draw(&tree), "Root ─┬─ A\n      └─ B\n");
    }

    #[test]
    fn long_topics_are_cut_short() {
        let options = Options { max_text_width: 5 };
        let map = layout(&Node::new("abcdefgh"), &options, None);
        assert_eq!(map.nodes[0].label, "abcd…");
    }

    #[test]
    fn the_topic_being_typed_is_shown_in_full() {
        let options = Options { max_text_width: 5 };
        let tree = Node::with_children("Root", vec![Node::new("short")]);
        let map = layout(&tree, &options, Some((&[0], "much longer text")));
        assert_eq!(map.to_text(), "Root ─── much longer text\n");
    }

    #[test]
    fn double_width_text_takes_two_columns() {
        let tree = Node::with_children("漢字", vec![Node::new("a")]);
        let map = layout(&tree, &Options::default(), None);
        assert_eq!(map.node(&[0]).map(|n| n.x), Some(9));
        assert_eq!(map.to_text(), "漢字 ─── a\n");
    }

    #[test]
    fn connectors_know_their_branch() {
        let map = layout(&sample(), &Options::default(), None);
        let at = |x: usize, y: usize| {
            map.connectors
                .iter()
                .find(|c| (c.x, c.y) == (x, y))
                .map(|c| (c.ch, c.branch))
        };
        assert_eq!(at(6, 1), Some(('┤', None)), "the root's junction is shared");
        assert_eq!(
            at(7, 2),
            Some(('─', Some(1))),
            "the stub to B starts B's branch"
        );
        assert_eq!(
            at(12, 1),
            Some(('└', Some(0))),
            "A's junction is in A's branch"
        );
    }

    #[test]
    fn empty_topics_still_take_a_cell() {
        let map = layout(&Node::new(""), &Options::default(), None);
        assert_eq!(map.nodes[0].label, " ");
        assert_eq!(map.width, 1);
    }
}
