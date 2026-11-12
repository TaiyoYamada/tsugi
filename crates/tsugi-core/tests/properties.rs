//! Runs random sequences of edits, git commands and tsugi operations and
//! checks, after every step, the properties that must always hold.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{TestRepo, git, write};
use proptest::prelude::*;
use tsugi_core::{Error, Snapshot, Trigger, Workspace};

#[derive(Debug, Clone)]
enum Step {
    Write {
        main: bool,
        file: usize,
        content: u8,
    },
    Delete {
        main: bool,
        file: usize,
    },
    Commit {
        main: bool,
    },
    /// What an agent does when it tidies up its history.
    ResetHard {
        main: bool,
    },
    Snapshot {
        main: bool,
    },
    /// Restores the snapshot at this position in the list, if there is one.
    Restore {
        main: bool,
        pick: usize,
    },
    Sync,
    Publish,
    Undo,
}

const FILES: [&str; 4] = ["a.txt", "b.txt", "dir/c.txt", "dir/sub/d.txt"];

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (any::<bool>(), 0..FILES.len(), any::<u8>())
            .prop_map(|(main, file, content)| Step::Write { main, file, content }),
        1 => (any::<bool>(), 0..FILES.len()).prop_map(|(main, file)| Step::Delete { main, file }),
        2 => any::<bool>().prop_map(|main| Step::Commit { main }),
        1 => any::<bool>().prop_map(|main| Step::ResetHard { main }),
        2 => any::<bool>().prop_map(|main| Step::Snapshot { main }),
        2 => (any::<bool>(), 0..8usize).prop_map(|(main, pick)| Step::Restore { main, pick }),
        1 => Just(Step::Sync),
        1 => Just(Step::Publish),
        1 => Just(Step::Undo),
    ]
}

/// The tree of the files on disk, computed without tsugi: a fresh index,
/// `git add --all`, `git write-tree`.
fn tree_on_disk(dir: &Path, scratch: &Path) -> String {
    let index = scratch.join("oracle-index");
    let _ = fs::remove_file(&index);
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir)
            .env("GIT_INDEX_FILE", &index)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    run(&["add", "--all"]);
    run(&["write-tree"])
}

/// Refusals are legitimate answers to a random request. Anything else is a
/// bug.
fn allow_refusal<T: std::fmt::Debug>(result: Result<T, Error>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(
            Error::Conflict { .. }
            | Error::NothingToPublish(_)
            | Error::NothingToUndo
            | Error::Refused(_),
        ) => None,
        // `git merge --ff-only` refusing to overwrite uncommitted changes in
        // the main worktree, and `git reset --keep` doing the same on undo.
        Err(Error::Git { args, .. })
            if args.starts_with("merge ") || args.starts_with("reset --keep") =>
        {
            None
        }
        Err(error) => panic!("unexpected failure: {error:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Every case builds a repository and runs dozens of git commands,
        // so the default is small. Set TSUGI_PROPTEST_CASES for a long run.
        cases: std::env::var("TSUGI_PROPTEST_CASES")
            .ok()
            .and_then(|cases| cases.parse().ok())
            .unwrap_or(24),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_histories_keep_every_promise(steps in prop::collection::vec(step(), 1..18)) {
        let t = TestRepo::new();
        let side = t.repo.workspace_new("side", None, None).unwrap();
        let main = t.main();
        let pick = |is_main: bool| -> &Workspace { if is_main { &main } else { &side } };
        let mut last_seq = [0u64; 2];

        for step in steps {
            // Undoing the creation of `side` removes it for good.
            let side_exists = t.repo.workspace("side").is_ok();
            let on_side = match &step {
                Step::Write { main, .. }
                | Step::Delete { main, .. }
                | Step::Commit { main }
                | Step::ResetHard { main }
                | Step::Snapshot { main }
                | Step::Restore { main, .. } => !main,
                Step::Sync | Step::Publish => true,
                Step::Undo => false,
            };
            if on_side && !side_exists {
                // tsugi says so instead of failing in some other way.
                prop_assert!(matches!(
                    t.repo.snapshot(&side, Trigger::Manual),
                    Err(Error::WorkspaceNotFound(_))
                ));
                continue;
            }
            match step {
                Step::Write { main, file, content } => {
                    write(&pick(main).path, FILES[file], &format!("{content}\n"));
                }
                Step::Delete { main, file } => {
                    let _ = fs::remove_file(pick(main).path.join(FILES[file]));
                }
                Step::Commit { main } => {
                    let dir = &pick(main).path;
                    git(dir, &["add", "--all"]);
                    git(dir, &["commit", "--quiet", "--allow-empty", "--message", "work"]);
                }
                Step::ResetHard { main } => {
                    git(&pick(main).path, &["reset", "--quiet", "--hard"]);
                }
                Step::Snapshot { main } => {
                    let workspace = pick(main);
                    let snapshot = t.repo.snapshot(workspace, Trigger::Manual).unwrap().snapshot;
                    // A snapshot is exactly what is on disk.
                    prop_assert_eq!(&snapshot.tree, &tree_on_disk(&workspace.path, t.dir.path()));
                }
                Step::Restore { main, pick: position } => {
                    let workspace = pick(main);
                    let snapshots = t.repo.snapshots(workspace).unwrap();
                    if let Some(target) = snapshots.get(position % snapshots.len().max(1)) {
                        let before = tree_on_disk(&workspace.path, t.dir.path());
                        let outcome = t.repo.restore(workspace, target).unwrap();
                        // The files are those of the snapshot, and what was
                        // there before is a snapshot of its own.
                        prop_assert_eq!(&tree_on_disk(&workspace.path, t.dir.path()), &target.tree);
                        prop_assert_eq!(&outcome.previous.tree, &before);
                    }
                }
                Step::Sync => {
                    let before = tree_on_disk(&side.path, t.dir.path());
                    if allow_refusal(t.repo.workspace_sync(&side)).is_none() {
                        // A refused sync changes nothing.
                        prop_assert_eq!(&tree_on_disk(&side.path, t.dir.path()), &before);
                    }
                }
                Step::Publish => {
                    let tip = git(&t.root, &["rev-parse", "main"]);
                    match allow_refusal(t.repo.publish(&side, Some("publish"), true)) {
                        Some(outcome) => {
                            // Exactly one commit was added to the base branch.
                            prop_assert_eq!(git(&t.root, &["rev-parse", "main"]), outcome.commit.clone());
                            prop_assert_eq!(git(&t.root, &["rev-parse", "main~1"]), tip);
                            if outcome.synced {
                                prop_assert_eq!(git(&side.path, &["rev-parse", "HEAD"]), outcome.commit);
                            }
                        }
                        None => prop_assert_eq!(git(&t.root, &["rev-parse", "main"]), tip),
                    }
                }
                Step::Undo => {
                    allow_refusal(t.repo.undo());
                }
            }

            // Sequence numbers only grow, and every snapshot can be read.
            for (index, workspace) in [&main, &side].into_iter().enumerate() {
                let snapshots: Vec<Snapshot> = t.repo.snapshots(workspace).unwrap();
                if let Some(newest) = snapshots.first() {
                    prop_assert!(newest.seq >= last_seq[index]);
                    last_seq[index] = newest.seq;
                }
            }
        }

        // Git agrees that nothing is broken, and no operation is left open.
        git(&t.root, &["fsck", "--no-dangling", "--no-progress"]);
        for workspace in [&main, &side] {
            for snapshot in t.repo.snapshots(workspace).unwrap() {
                git(&t.root, &["ls-tree", "-r", &snapshot.id]);
            }
        }
        let open = t.repo.operations(1000).unwrap().into_iter()
            .filter(|operation| operation.state == tsugi_core::OpState::Started)
            .count();
        prop_assert_eq!(open, 0);
    }
}
