# Contributing

## Build and test

```
cargo build
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

These cover the library, the command line tool and the terminal app, and
need Rust 1.90 or newer and Git 2.38 or newer. The tests create
repositories in temporary directories and run the real `git`.

The property test runs 24 random histories by default.
`TSUGI_PROPTEST_CASES=500 cargo test -p tsugi-core --test properties` runs
more.

The desktop app has its own steps, in `apps/desktop`:

```
pnpm install
pnpm bindings     # regenerate the TypeScript types after changing Rust types
pnpm typecheck
pnpm test
pnpm tauri dev    # the app, with reloading
VITE_MOCK=1 pnpm dev   # the screens in a browser, with fake data
cargo test --package tsugi-desktop   # needs `pnpm build` once
```

The texts of the desktop app are in `apps/desktop/src/i18n/en.ts` and
`ja.ts`. A text that is added to one has to be added to the other; the type
check fails otherwise.

## Layout

```
crates/tsugi-core   the layer: everything tsugi does
crates/tsugi        the command line tool
crates/tsugi-tui    the terminal app
apps/desktop        the desktop app (Tauri and React)
docs/design.md      what is stored and what each operation guarantees
docs/cli.md         the JSON output of the command line tool
```

The command line tool and both apps call `tsugi-core` and contain no logic
of their own about repositories.

## Rules of the code

These follow from [docs/design.md](docs/design.md). A change that needs to
break one of them needs a change to that document first.

- Git is run, not reimplemented. Every Git operation goes through
  `crates/tsugi-core/src/git.rs`. Do not read or write files under `.git`
  other than `.git/tsugi/`.
- The index of a worktree is never modified. Snapshots are built with a
  temporary index.
- Anything that overwrites files or moves a branch takes a snapshot first.
- Every change to a repository is an operation: recorded as `started`
  before it begins, with the facts undo and recovery need written down as
  soon as they are known. A new kind of operation comes with its recovery,
  its undo, a failpoint at every step where an interruption leaves a
  different state, and a test in `crates/tsugi/tests/crash.rs` for each.
- What Git can express is read from Git every time, not cached in the
  database.
- Types that cross to TypeScript are generated (`pnpm bindings`), not
  written twice.

## Commits

One change per commit, with a message of the form `type(scope): summary`
(`feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`). The maintainer
writes them in Japanese; English is welcome.
