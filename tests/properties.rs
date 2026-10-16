//! Properties that must hold for any map, not just the unit tests' examples.

use std::{collections::HashSet, path::PathBuf};

use mull::{
    app::{App, Mode},
    layout::{self, Options},
    markdown,
    tree::{Node, normalize_text},
};
use proptest::{prelude::*, sample::Index};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,6}",
        // Characters that could be mistaken for Markdown structure.
        "[ #*+\\-.)0-9a-z\\[\\]`]{0,12}",
        "\\PC{0,8}",
    ]
    .prop_map(|s| normalize_text(&s))
}

fn tree() -> impl Strategy<Value = Node> {
    let leaf = text().prop_map(|t| Node::new(&t));
    leaf.prop_recursive(4, 48, 5, |inner| {
        (text(), prop::collection::vec(inner, 0..5))
            .prop_map(|(t, children)| Node::with_children(&t, children))
    })
}

/// Text shaped like Markdown: headings, bullets, numbers, indentation.
fn document() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        "[ \t]{0,6}([-*+]|#{1,7}|[0-9]{1,2}[.)])?[ \t]?[a-z#é ]{0,8}",
        any::<String>(),
    ];
    prop::collection::vec(line, 0..15).prop_map(|lines| lines.join("\n"))
}

fn preorder<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(node);
    for child in &node.children {
        preorder(child, out);
    }
}

fn paths(node: &Node, path: &mut Vec<usize>, visible_only: bool, out: &mut Vec<Vec<usize>>) {
    out.push(path.clone());
    if visible_only && node.collapsed {
        return;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        paths(child, path, visible_only, out);
        path.pop();
    }
}

fn all_paths(node: &Node, visible_only: bool) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    paths(node, &mut Vec::new(), visible_only, &mut out);
    out
}

fn sorted_texts(node: &Node) -> Vec<String> {
    let mut nodes = Vec::new();
    preorder(node, &mut nodes);
    let mut texts: Vec<String> = nodes.iter().map(|n| n.text.clone()).collect();
    texts.sort();
    texts
}

fn plain(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Every key except the ones that save or quit.
fn key() -> impl Strategy<Value = KeyEvent> {
    let mut keys: Vec<KeyEvent> = "hjklgecdyp JKHLuU?x漢".chars().map(plain).collect();
    keys.extend([
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        KeyEvent::from(KeyCode::Tab),
        KeyEvent::from(KeyCode::Enter),
        KeyEvent::from(KeyCode::Esc),
        KeyEvent::from(KeyCode::Backspace),
        KeyEvent::from(KeyCode::Delete),
        KeyEvent::from(KeyCode::Left),
        KeyEvent::from(KeyCode::Home),
    ]);
    prop::sample::select(keys)
}

proptest! {
    #[test]
    fn saving_then_reading_gives_back_the_same_map(tree in tree()) {
        let text = markdown::serialize(&tree);
        prop_assert_eq!(markdown::parse(&text, "fallback"), tree);
    }

    #[test]
    fn any_document_settles_into_one_layout(input in document()) {
        let once = markdown::serialize(&markdown::parse(&input, "root"));
        let twice = markdown::serialize(&markdown::parse(&once, "root"));
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn every_non_blank_line_becomes_one_topic(input in document()) {
        let tree = markdown::parse(&input, "root");
        let lines = input.lines().filter(|l| !l.trim().is_empty()).count();
        // Plus one when the lines needed a root to share.
        prop_assert!(tree.size() == lines || (tree.size() == lines + 1 && tree.text == "root"));
    }

    #[test]
    fn layout_shows_each_visible_topic_once_without_overlaps(
        mut tree in tree(),
        folds in prop::collection::vec(any::<bool>(), 0..64),
        max_width in 2usize..30,
    ) {
        let mut folds = folds.into_iter();
        for path in all_paths(&tree.clone(), false) {
            if let Some(node) = tree.get_mut(&path) {
                node.collapsed = folds.next().unwrap_or(false);
            }
        }
        let map = layout::layout(&tree, &Options { max_text_width: max_width }, None);

        let placed: Vec<Vec<usize>> = map.nodes.iter().map(|n| n.path.clone()).collect();
        prop_assert_eq!(placed, all_paths(&tree, true));

        let mut cells = HashSet::new();
        for node in &map.nodes {
            prop_assert!(node.label_width() <= max_width);
            for x in node.x..node.x + node.width() {
                prop_assert!(cells.insert((x, node.y)), "{:?} overlaps at {x},{}", node.path, node.y);
            }
        }
        for c in &map.connectors {
            prop_assert!(cells.insert((c.x, c.y)), "connector overlaps at {},{}", c.x, c.y);
        }
        prop_assert!(cells.iter().all(|&(x, y)| x < map.width && y < map.height));

        let mut leaves = 0;
        for node in &map.nodes {
            let children: Vec<_> = map
                .nodes
                .iter()
                .filter(|c| c.path.len() == node.path.len() + 1 && c.path.starts_with(&node.path))
                .collect();
            let (Some(top), Some(bottom)) = (children.iter().map(|c| c.y).min(), children.iter().map(|c| c.y).max()) else {
                leaves += 1;
                continue;
            };
            prop_assert!(top <= node.y && node.y <= bottom, "{:?} isn't beside its children", node.path);
            prop_assert!(children.iter().all(|c| c.x == node.x + node.label_width() + 5));
        }
        prop_assert_eq!(map.height, leaves, "one row per visible leaf");
    }

    #[test]
    fn restructuring_moves_whole_subtrees_and_loses_nothing(
        mut tree in tree(),
        moves in prop::collection::vec((0..4usize, any::<Index>()), 0..30),
    ) {
        let texts = sorted_texts(&tree);
        for (op, pick) in moves {
            let paths = all_paths(&tree, false);
            let path = pick.get(&paths).clone();
            let before = tree.clone();
            let subtree = tree.get(&path).cloned();
            let moved = match op {
                0 => tree.move_up(&path),
                1 => tree.move_down(&path),
                2 => tree.indent(&path),
                _ => tree.outdent(&path),
            };
            match moved {
                Some(new_path) => prop_assert_eq!(tree.get(&new_path).cloned(), subtree),
                None => prop_assert_eq!(&tree, &before),
            }
        }
        prop_assert_eq!(sorted_texts(&tree), texts);
    }

    #[test]
    fn random_sessions_stay_consistent_and_undo_completely(
        start in tree(),
        keys in prop::collection::vec(key(), 0..80),
    ) {
        let mut app = App::new(start.clone(), PathBuf::from("unused.md"));
        for key in keys {
            app.handle_key(key);
            prop_assert!(app.tree.is_visible(&app.selected), "selection {:?} isn't on screen", app.selected);
            let mut nodes = Vec::new();
            preorder(&app.tree, &mut nodes);
            prop_assert!(nodes.iter().all(|n| n.text == normalize_text(&n.text)));
            prop_assert!(!app.should_quit);
        }
        // Leave whatever mode the keys ended in.
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        prop_assert!(matches!(app.mode, Mode::Map));

        // The session may itself end on an undo, so go forward to the
        // newest state first; redo-everything must come back to exactly it.
        for _ in 0..200 {
            app.handle_key(plain('U'));
        }
        let newest = markdown::serialize(&app.tree);
        for _ in 0..200 {
            app.handle_key(plain('u'));
        }
        prop_assert_eq!(markdown::serialize(&app.tree), markdown::serialize(&start));
        prop_assert!(!app.is_dirty());
        for _ in 0..200 {
            app.handle_key(plain('U'));
        }
        prop_assert_eq!(markdown::serialize(&app.tree), newest);
    }
}
