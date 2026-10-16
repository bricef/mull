# mull's commands. GitHub Actions runs these same recipes, so what passes
# here passes there: CI is `just ci`, a release is `just release-check`,
# `just dist`, `just publish` and `just github-release`.

set shell := ["bash", "-euo", "pipefail", "-c"]
set windows-shell := ["bash", "-euo", "pipefail", "-c"]
set script-interpreter := ["bash", "-euo", "pipefail"]

# List the recipes
default:
    @just --list

# Install the Rust toolchain pinned in rust-toolchain.toml
[group('develop')]
toolchain:
    rustup toolchain install

# Everything CI checks on main and on pull requests
[group('develop')]
ci: fmt-check lint test

# Same as ci
[group('develop')]
check: ci

[group('develop')]
fmt-check:
    cargo fmt --check

[group('develop')]
lint:
    cargo clippy --all-targets --locked -- -D warnings

[group('develop')]
test:
    cargo test --locked

# An optimised build for this machine, in target/release/mull
[group('develop')]
build:
    cargo build --release --locked

# Open a map in the terminal UI
[group('develop')]
run file:
    cargo run -- "{{ file }}"

# Print the key tables the README must contain, after changing src/keymap.rs
[group('develop')]
keys:
    cargo run -q -- --keys

# The version in Cargo.toml
[group('release')]
version:
    @id="$(cargo pkgid)"; echo "${id##*[#@]}"

# Bump the version (major, minor, patch or X.Y.Z), then commit and tag it on main
[group('release')]
[script]
mint level:
    die() { echo "mint: $*" >&2; exit 1; }
    level="{{ level }}"
    [[ "$(git branch --show-current)" == main ]] || die "releases come from main; switch to it first"
    [[ -z "$(git status --porcelain --untracked-files=no)" ]] || die "commit or stash your changes first"
    git fetch --quiet origin main
    [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] \
        || die "main and origin/main differ; pull or push first"

    current="$(just version)"
    IFS=. read -r major minor patch <<< "$current"
    case "$level" in
        major) next="$((major + 1)).0.0" ;;
        minor) next="$major.$((minor + 1)).0" ;;
        patch) next="$major.$minor.$((patch + 1))" ;;
        *) next="$level" ;;
    esac
    [[ "$next" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
        || die "'$level' isn't major, minor, patch or a version like 1.2.3"
    [[ "$next" != "$current" && "$(printf '%s\n' "$current" "$next" | sort -V | tail -n 1)" == "$next" ]] \
        || die "$next doesn't come after $current"
    ! git rev-parse --quiet --verify "refs/tags/v$next" > /dev/null || die "v$next is already tagged"

    # Gate the commit being released before touching anything.
    just ci
    # The first `version =` line is the package's own.
    current="$current" next="$next" perl -0pi -e 's/^version = "\Q$ENV{current}\E"/version = "$ENV{next}"/m' Cargo.toml
    cargo update --workspace --quiet
    [[ "$(just version)" == "$next" ]] || die "couldn't set the version in Cargo.toml"
    git commit --quiet -m "Release v$next" Cargo.toml Cargo.lock
    git tag -a "v$next" -m "mull v$next"
    echo "Tagged v$next. Start the release with: just push-release"

# Push main and the current version's tag; the tag starts the release workflow
[group('release')]
[script]
push-release:
    tag="v$(just version)"
    git rev-parse --quiet --verify "refs/tags/$tag" > /dev/null \
        || { echo "push-release: there's no $tag tag; run just mint first" >&2; exit 1; }
    git push --atomic origin main "refs/tags/$tag"

# Check that a tag may be released: vX.Y.Z, checked out, on main, matching Cargo.toml, publishable
[group('release')]
[script]
release-check tag:
    die() { echo "release-check: $*" >&2; exit 1; }
    tag="{{ tag }}"
    [[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || die "$tag isn't a vX.Y.Z tag"
    commit="$(git rev-parse --quiet --verify "refs/tags/$tag^{commit}")" || die "there's no $tag tag"
    [[ "$(git rev-parse HEAD)" == "$commit" ]] || die "check out $tag first"
    git fetch --quiet origin main
    git merge-base --is-ancestor "$commit" origin/main || die "$tag isn't on main"
    version="$(just version)"
    [[ "$tag" == "v$version" ]] || die "$tag doesn't match the version in Cargo.toml, $version"
    for field in description license repository; do
        grep -Eq "^$field(-file)? = " Cargo.toml || die "crates.io needs a $field in Cargo.toml"
    done
    cargo publish --dry-run --locked

# Build a release archive and its checksum for a target triple, into dist/
[group('release')]
[script]
dist target:
    target="{{ target }}"
    rustup target add "$target"
    cargo build --release --locked --target "$target"
    name="mull-v$(just version)-$target"
    exe=mull
    if [[ "$target" == *windows* ]]; then exe=mull.exe; fi
    rm -rf "dist/$name"
    mkdir -p "dist/$name"
    shopt -s nullglob
    cp "target/$target/release/$exe" README.md LICENSE* "dist/$name/"
    cd dist
    if [[ "$target" == *windows* ]]; then
        archive="$name.zip"
        pwsh -NoProfile -Command "Compress-Archive -Force -Path '$name' -DestinationPath '$archive'"
    else
        archive="$name.tar.gz"
        tar -czf "$archive" "$name"
    fi
    rm -rf "$name"
    if command -v sha256sum > /dev/null; then sha256sum "$archive"; else shasum -a 256 "$archive"; fi > "$archive.sha256"
    echo "dist/$archive"

# Publish the crate; needs CARGO_REGISTRY_TOKEN or a prior cargo login
[group('release')]
publish:
    cargo publish --locked

# Create the GitHub release for a tag from the archives in dist/; needs gh auth or GH_TOKEN
[group('release')]
github-release tag:
    gh release create "{{ tag }}" dist/* --verify-tag --title "mull {{ tag }}" --generate-notes
