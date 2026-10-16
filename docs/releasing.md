# Releasing mull

## A release

```sh
just mint patch       # or minor, major, or an exact version like 1.2.3
just push-release
```

`just mint` works on an up-to-date `main` with no uncommitted changes. It
runs `just ci`, bumps the version in `Cargo.toml` and `Cargo.lock`, commits
"Release vX.Y.Z" and tags it `vX.Y.Z`. Nothing leaves your machine until
`just push-release` pushes `main` and the tag together.

The tag starts `.github/workflows/release.yml`, whose every step is a just
recipe you can run yourself:

1. `just release-check vX.Y.Z` and `just ci`. The check confirms the tag
   is `vX.Y.Z`, is on `main`, matches `Cargo.toml`, and that the crate
   packages and builds (`cargo publish --dry-run`).
2. `just dist <target>` on each platform: Linux x86_64 and arm64 (static,
   musl), macOS arm64 and x86_64, Windows x86_64. Each makes an archive
   with the binary and README, plus a `.sha256`, in `dist/`.
3. `just publish` uploads the crate to crates.io, but only once every
   binary has built, because a published version can't be taken back.
4. `just github-release vX.Y.Z` creates the GitHub release, with generated
   notes and the archives attached.

To rehearse without publishing, run the Release workflow by hand from the
Actions tab, on any branch: it runs `just ci` and builds every archive
(downloadable from the run), then stops.

CI (`.github/workflows/ci.yml`) runs `just ci` on every push to `main` and
every pull request. Both workflows install the same just version as dev
machines, and `rust-toolchain.toml` pins the compiler for everyone.

### When a release fails

- **Before step 3:** nothing is public yet. Fix it on `main`, then move the
  tag to the fix and push it again:
  `git tag -fa vX.Y.Z -m "mull vX.Y.Z" && git push -f origin refs/tags/vX.Y.Z`.
  The fix must keep the version in `Cargo.toml` unchanged.
- **At step 4:** the crate is out; re-run the failed job from the Actions
  page.
- **After step 3, if the release itself is broken:** crates.io versions are
  permanent. Yank it (`cargo yank --version X.Y.Z`) and mint the next
  patch.

## One-time setup

1. **The repository.** `gh repo create bricef/mull --public --source . --push`.
   The `repository` field in `Cargo.toml` assumes this name.
2. **A licence.** crates.io refuses crates without one: add `license` to
   `Cargo.toml` and the licence file(s) at the root (`LICENSE*` files are
   packaged and shipped in the archives). `just release-check` refuses to
   go on without it.
3. **A `release` environment** that only tags can deploy to, holding the
   crates.io token:

   ```sh
   gh api -X PUT repos/bricef/mull/environments/release \
     -F 'deployment_branch_policy[protected_branches]=false' \
     -F 'deployment_branch_policy[custom_branch_policies]=true'
   gh api -X POST repos/bricef/mull/environments/release/deployment-branch-policies \
     -f name='v*' -f type=tag
   ```

4. **A crates.io token for the first publish.** crates.io won't create a
   new crate through trusted publishing, so the first release uses a token.
   On crates.io, under Account Settings → API Tokens, make one with the
   `publish-new` and `publish-update` scopes, restricted to the crate `mull`,
   with a short expiry. Then:

   ```sh
   gh secret set CARGO_REGISTRY_TOKEN --env release
   ```

5. **Trusted publishing, after the first release.** On crates.io, open
   mull's settings → Trusted Publishing and add GitHub repository
   `bricef/mull`, workflow `release.yml`, environment `release`. Then switch
   the workflow over and drop the token:

   ```sh
   gh variable set CRATES_IO_TRUSTED_PUBLISHING --body true
   gh secret delete CARGO_REGISTRY_TOKEN --env release
   ```

   Revoke the token on crates.io as well. From then on each release gets a
   short-lived token from `rust-lang/crates-io-auth-action`, and no
   long-lived secret is stored anywhere.
