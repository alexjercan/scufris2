# Release checklist

Use this process for a stable `vX.Y.Z` release.

1. Start from clean, current `master`. Review the completed tasks, user-facing documentation, and release scope. Commit all required fixes before release preparation.
2. Select the next semantic version. Update `package.json` and `package-lock.json` with `npm version X.Y.Z --no-git-tag-version`. Update the workspace version in `Cargo.toml` and refresh `Cargo.lock` with `cargo check --workspace` from the repository root. Update `MARKETING_VERSION` in `surfaces/ios/project.yml`; `npm run version:check` reads it and fails the release when it lags. Move the `CHANGELOG.md` `Unreleased` entries under a `X.Y.Z` heading with the release date, and update the comparison links at the end of that file. Review the diff and commit the version change.
3. Install the locked dependencies with `npm ci`. Run `nix develop -c npm run check` with Scufris voice development variables unset. The checks run in the development shell because the briefing tests run `tools/briefing`, which imports markdown-it-py; `nix/python.nix` is the one place that dependency is declared, and CI runs them the same way.
4. For changes affected by voice configuration, run `npm run check` again in `nix develop` with its normal environment.
5. Run the full repository checks:

   ```bash
   nix develop -c npm run check
   python3 -m unittest discover -s tests -p 'test_*.py'
   ruff check .
   ruff format --check .
   shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging
   (cargo clippy --all-targets -- -D warnings && cargo test)
   nix fmt -- --check .
   nix flake check -L
   git diff --check
   ```

6. Confirm `master` is clean and contains the reviewed version commit. Create an annotated tag on that commit: `git tag -a vX.Y.Z -m "Scufris vX.Y.Z"`. Release tags are immutable. Never move, replace, or reuse one.
7. Push `master` first. Then push only the new tag. The tag starts `.github/workflows/release.yml`.
8. In GitHub Actions, verify the reusable check job passed, the tag matched the root package version, and the publication job created a source-only GitHub Release with generated notes. Do not add assets; Nix consumers use the tagged source flake.
9. Verify every workflow the release push started, not only `release`. Pushing `master` also starts `check`, `Documentation`, and `iOS`, and all of them run against the tagged commit. The Swift suite runs in `iOS` and nowhere else: it needs Xcode, so no step above reaches it and CI is its only gate. `gh run list --limit 5` after the push shows every one.
10. When `SERVICE_VERSION` changed in this release, ship the iOS surface too: `gh workflow run testflight.yml --ref vX.Y.Z`. The gateway compares the version with exact equality and negotiates nothing (`shared/control/src/service.rs`), so a phone still on the previous number is refused at its hello the moment the machine switches. `testflight.yml` is `workflow_dispatch` only; no push and no tag starts it. The release is not done until the build is uploaded, because every surface has to move together.
11. Deploy. A tag reaches nothing on its own: the consuming Nix configuration pins a tag, so until its input is bumped and switched, every unit on the machine still runs the previous release. Bump the `scufris2` input to `vX.Y.Z` in that configuration, run its `home-manager switch`, and confirm the units picked the new build up:

    ```bash
    systemctl --user show scufris-service -p ExecStart | grep -o '/nix/store/[^ ]*'
    systemctl --user status scufris-desktop scufris-surface-gateway
    ```

    When `SERVICE_VERSION` moved, do not switch until step 10's TestFlight build is installed on the phone. An upload is not an install, and the moment the machine switches the phone is refused at its hello.
