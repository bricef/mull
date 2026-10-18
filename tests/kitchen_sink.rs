//! The kitchen-sink fixture holds every common Markdown construct. Opening
//! and saving it must keep what it renders as, and the constructs that the
//! old line-by-line reader got wrong must come through whole.

use std::fs;

use mull::{
    markdown,
    tree::{BlockKind, Kind, Node},
};

fn fixture() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/kitchen-sink.md"
    );
    fs::read_to_string(path).expect("the fixture is readable")
}

/// What a document renders as, with whitespace evened out: joining an
/// item's lines or changing blank lines moves whitespace, not meaning.
fn rendered(source: &str) -> String {
    let options = ::markdown::Options {
        parse: markdown::parse_options(),
        compile: ::markdown::CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..::markdown::CompileOptions::gfm()
        },
    };
    let html = ::markdown::to_html_with_options(source, &options).expect("renders");
    html.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

fn all_nodes(node: &Node) -> Vec<&Node> {
    let mut out = vec![node];
    for child in &node.children {
        out.extend(all_nodes(child));
    }
    out
}

fn section<'a>(tree: &'a Node, title: &str) -> &'a Node {
    all_nodes(tree)
        .into_iter()
        .find(|n| n.is_heading() && n.text == title)
        .unwrap_or_else(|| panic!("no section {title:?}"))
}

#[test]
fn saving_keeps_what_the_file_renders_as() {
    let original = fixture();
    let saved = markdown::serialize(&markdown::parse(&original, "kitchen-sink"));
    let html = rendered(&original);
    for construct in [
        "<table>",
        "<pre><code class=\"language-rust\">",
        "<blockquote>",
        "data-footnotes",
    ] {
        assert!(html.contains(construct), "the oracle renders {construct}");
    }
    assert_eq!(rendered(&saved), html, "saved as:\n{saved}");
}

#[test]
fn saving_settles_after_one_save() {
    let once = markdown::serialize(&markdown::parse(&fixture(), "kitchen-sink"));
    let twice = markdown::serialize(&markdown::parse(&once, "kitchen-sink"));
    assert_eq!(once, twice);
}

#[test]
fn the_map_follows_the_rules() {
    let map = markdown::parse(&fixture(), "kitchen-sink");
    assert_eq!(map.root.check(), Ok(()));
}

#[test]
fn code_comments_stay_inside_their_code() {
    let tree = markdown::parse(&fixture(), "kitchen-sink").root;
    let headings: Vec<&str> = all_nodes(&tree)
        .into_iter()
        .filter(|n| n.is_heading())
        .map(|n| n.text.as_str())
        .collect();
    assert!(!headings.contains(&"A shell comment that looks like a heading"));
    assert!(!headings.contains(&"Tilde fences"));
    let code = section(&tree, "Code");
    assert!(
        code.children
            .iter()
            .all(|c| c.kind == Kind::Block(BlockKind::Code))
    );
    assert_eq!(
        code.children.len(),
        6,
        "rust, bash, python, markdown, indented, mermaid"
    );
}

#[test]
fn front_matter_is_metadata_and_four_top_level_headings_make_a_document() {
    let map = markdown::parse(&fixture(), "kitchen-sink");
    assert_eq!(
        map.front_matter.as_deref(),
        Some("---\ntitle: Kitchen sink\ntags: [markdown, test]\n---")
    );
    assert_eq!(map.root.kind, Kind::Document);
    assert!(
        map.root.children.iter().all(Node::is_heading),
        "nothing above the first heading"
    );
}

#[test]
fn blocks_come_through_whole() {
    let tree = markdown::parse(&fixture(), "kitchen-sink").root;
    let tables = section(&tree, "Tables");
    assert_eq!(tables.children.len(), 1);
    assert_eq!(tables.children[0].kind, Kind::Block(BlockKind::Table));
    assert_eq!(tables.children[0].text.lines().count(), 5);

    let kinds: Vec<&Kind> = all_nodes(&tree).into_iter().map(|n| &n.kind).collect();
    for kind in [
        BlockKind::List,
        BlockKind::Definition,
        BlockKind::Footnote,
        BlockKind::Quote,
        BlockKind::Html,
        BlockKind::Maths,
        BlockKind::Rule,
    ] {
        assert!(kinds.contains(&&Kind::Block(kind)), "no {kind:?} block");
    }
}

#[test]
fn lists_are_blocks_kept_exactly_as_written() {
    let tree = markdown::parse(&fixture(), "kitchen-sink").root;
    let lists = section(&tree, "Lists");
    assert!(
        lists
            .children
            .iter()
            .all(|c| c.kind == Kind::Block(BlockKind::List))
    );
    assert_eq!(lists.children.len(), 8);
    let saved = markdown::serialize(&markdown::parse(&fixture(), "kitchen-sink"));
    for exact in [
        "\n1. First\n2. Second\n   1. Nested ordered\n   2. Again\n10. Ten\n",
        "\n3) Parenthesis style, starting at three\n4) Four\n",
        "\n- [ ] A task to do\n- [x] A task done\n  - [ ] A nested task\n",
        "\n- Lazy continuation:\nthis line isn't indented but still belongs to the item.\n",
    ] {
        assert!(saved.contains(exact), "{exact:?} not kept");
    }
}
