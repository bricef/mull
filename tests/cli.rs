//! The command line, and the docs that describe it.

use std::{fs, process::Command};

fn mull() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mull"))
}

#[test]
fn print_draws_the_map_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("plan.md");
    fs::write(&file, "# Plan\n\n- Goals\n  - fast\n- Risks\n").unwrap();
    let out = mull().arg("--print").arg(&file).output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "Plan ─┬─ Goals ─── fast\n      └─ Risks\n"
    );
}

#[test]
fn a_file_is_required() {
    assert!(!mull().output().unwrap().status.success());
}

#[test]
fn the_readme_lists_the_keys_mull_actually_uses() {
    let out = mull().arg("--keys").output().unwrap();
    let keys = String::from_utf8(out.stdout).unwrap();
    assert!(keys.contains("| `h` `←` | Go to the parent |"), "{keys}");
    let readme = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    assert!(
        readme.contains(&keys),
        "README.md's key tables are stale: replace them with the output of `mull --keys`"
    );
}
