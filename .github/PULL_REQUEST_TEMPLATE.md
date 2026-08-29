## What this changes

<!-- What is different afterwards, and why it is worth doing. If it fixes an
     issue, say "Fixes #123" so it closes itself. -->

## How you know it works

<!-- What you actually ran or clicked. Rendering and input are where this
     project's real bugs live, and no test suite can see them — if you changed a
     screen, say what you did in the running app. -->

## Checklist

- [ ] `cargo fmt --check`
- [ ] `cargo check --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] I ran the application and used the part I changed
- [ ] New behaviour has a test, and the test's comment says what breaks in the
      product if it goes red
- [ ] Any new dependency is explained above — what it does, and why nothing
      already in the tree would do

<!-- The title is checked against Conventional Commits: type(scope): summary,
     for example `fix(import): read single-column CSVs`. Types: feat, fix, docs,
     style, refactor, perf, test, build, ci, chore, revert. -->
