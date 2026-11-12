mod common;

use std::fs;

use common::{TestRepo, commit_all, git, read, tree_files, write};
use tsugi_core::{Error, Repo, Trigger};

#[test]
fn discover_requires_init() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Repo::discover(dir.path()),
        Err(Error::NotARepository(_))
    ));
    git(dir.path(), &["init", "--quiet"]);
    assert!(matches!(
        Repo::discover(dir.path()),
        Err(Error::NotInitialized)
    ));
}

#[test]
fn init_adopts_the_main_worktree() {
    let t = TestRepo::new();
    let workspaces = t.repo.workspaces().unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "repo");
    assert_eq!(workspaces[0].path, t.root);
    assert_eq!(workspaces[0].base, "main");
}

#[test]
fn snapshot_holds_the_working_tree_and_leaves_the_index_alone() {
    let t = TestRepo::with_config("[snapshot]\nmax_file_bytes = 100\n");
    let main = t.main();
    write(&t.root, "tracked.txt", "one\n");
    commit_all(&t.root, "add tracked");

    write(&t.root, "tracked.txt", "two\n");
    write(&t.root, "staged.txt", "staged\n");
    git(&t.root, &["add", "staged.txt"]);
    write(&t.root, "untracked/new.txt", "new\n");
    write(&t.root, "ignored/build.txt", "ignored\n");
    write(&t.root, "debug.log", "ignored\n");
    write(&t.root, "large.bin", &"x".repeat(101));
    fs::remove_file(t.root.join("README.md")).unwrap();
    let status_before = git(&t.root, &["status", "--porcelain"]);

    let outcome = t.repo.snapshot(&main, Trigger::Manual).unwrap();
    assert!(outcome.created);
    let snapshot = outcome.snapshot;
    assert_eq!(snapshot.seq, 1);
    assert_eq!(snapshot.trigger, Trigger::Manual);
    assert_eq!(snapshot.branch.as_deref(), Some("refs/heads/main"));
    assert_eq!(
        snapshot.head.as_deref(),
        Some(git(&t.root, &["rev-parse", "HEAD"]).as_str())
    );
    assert_eq!(
        tree_files(&t.root, &snapshot.tree),
        [
            ".gitignore",
            "staged.txt",
            "tracked.txt",
            "tsugi.toml",
            "untracked/new.txt"
        ]
    );
    assert_eq!(
        git(&t.root, &["show", &format!("{}:tracked.txt", snapshot.id)]),
        "two"
    );
    assert_eq!(git(&t.root, &["status", "--porcelain"]), status_before);
}

#[test]
fn snapshot_is_skipped_when_nothing_changed() {
    let t = TestRepo::new();
    let main = t.main();
    write(&t.root, "a.txt", "a\n");
    let first = t.repo.snapshot(&main, Trigger::Manual).unwrap();
    let second = t.repo.snapshot(&main, Trigger::Manual).unwrap();
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.snapshot, second.snapshot);

    // A commit changes HEAD but not the tree; that is a new state.
    commit_all(&t.root, "add a");
    let third = t.repo.snapshot(&main, Trigger::Manual).unwrap();
    assert!(third.created);
    assert_eq!(third.snapshot.seq, 2);
    assert_eq!(third.snapshot.tree, first.snapshot.tree);
    assert_eq!(t.repo.snapshots(&main).unwrap().len(), 2);
}

#[test]
fn snapshot_keeps_commits_reachable_after_they_leave_the_branch() {
    let t = TestRepo::new();
    let main = t.main();
    write(&t.root, "a.txt", "a\n");
    let lost = commit_all(&t.root, "will be dropped");
    t.repo.snapshot(&main, Trigger::Manual).unwrap();
    git(&t.root, &["reset", "--quiet", "--hard", "HEAD~1"]);
    git(&t.root, &["reflog", "expire", "--expire=now", "--all"]);
    git(&t.root, &["gc", "--quiet", "--prune=now"]);
    assert_eq!(git(&t.root, &["cat-file", "-t", &lost]), "commit");
}

#[test]
fn snapshot_works_before_the_first_commit() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--initial-branch=main"]);
    write(dir.path(), "a.txt", "a\n");
    let repo = Repo::init(dir.path()).unwrap();
    let main = repo.workspaces().unwrap().remove(0);
    let snapshot = repo.snapshot(&main, Trigger::Manual).unwrap().snapshot;
    assert_eq!(snapshot.head, None);
    assert_eq!(tree_files(dir.path(), &snapshot.tree), ["a.txt"]);
}

#[test]
fn restore_brings_back_files_and_branch() {
    let t = TestRepo::new();
    let main = t.main();
    write(&t.root, "a.txt", "first\n");
    write(&t.root, "only-then.txt", "x\n");
    write(&t.root, "ignored/keep.txt", "kept\n");
    let then = t.repo.snapshot(&main, Trigger::Manual).unwrap().snapshot;
    let head_then = git(&t.root, &["rev-parse", "HEAD"]);

    fs::remove_file(t.root.join("only-then.txt")).unwrap();
    write(&t.root, "a.txt", "second\n");
    write(&t.root, "later/b.txt", "b\n");
    commit_all(&t.root, "later work");
    write(&t.root, "uncommitted.txt", "u\n");

    let outcome = t.repo.restore(&main, &then).unwrap();
    assert!(outcome.branch_moved);
    assert_eq!(read(&t.root, "a.txt"), "first\n");
    assert_eq!(read(&t.root, "only-then.txt"), "x\n");
    assert_eq!(read(&t.root, "ignored/keep.txt"), "kept\n");
    assert!(!t.root.join("later").exists());
    assert!(!t.root.join("uncommitted.txt").exists());
    assert_eq!(git(&t.root, &["rev-parse", "HEAD"]), head_then);

    // The state before the restore is a snapshot too.
    assert_eq!(outcome.previous.trigger, Trigger::PreRestore);
    t.repo.restore(&main, &outcome.previous).unwrap();
    assert_eq!(read(&t.root, "a.txt"), "second\n");
    assert_eq!(read(&t.root, "uncommitted.txt"), "u\n");
    assert_eq!(read(&t.root, "later/b.txt"), "b\n");
}

#[test]
fn restore_leaves_refs_alone_on_another_branch() {
    let t = TestRepo::new();
    let main = t.main();
    write(&t.root, "a.txt", "on main\n");
    let on_main = t.repo.snapshot(&main, Trigger::Manual).unwrap().snapshot;
    git(&t.root, &["checkout", "--quiet", "-b", "other"]);
    commit_all(&t.root, "on other");
    let head = git(&t.root, &["rev-parse", "HEAD"]);

    let outcome = t.repo.restore(&main, &on_main).unwrap();
    assert!(!outcome.branch_moved);
    assert_eq!(git(&t.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(read(&t.root, "a.txt"), "on main\n");
}

#[test]
fn find_snapshot_understands_selectors() {
    let t = TestRepo::new();
    let main = t.main();
    write(&t.root, "a.txt", "1\n");
    let one = t.repo.snapshot(&main, Trigger::Manual).unwrap().snapshot;
    write(&t.root, "a.txt", "2\n");
    let two = t.repo.snapshot(&main, Trigger::Manual).unwrap().snapshot;

    let find =
        |selector: &str, current| t.repo.find_snapshot(selector, current).map(|found| found.1);
    assert_eq!(find("repo@1", None).unwrap(), one);
    assert_eq!(find("repo@latest", None).unwrap(), two);
    assert_eq!(find("repo", None).unwrap(), two);
    assert_eq!(find("@1", Some(&main)).unwrap(), one);
    assert!(matches!(
        find("repo@9", None),
        Err(Error::SnapshotNotFound(_))
    ));
    assert!(matches!(find("@1", None), Err(Error::SnapshotNotFound(_))));
    assert!(matches!(
        find("nope@1", None),
        Err(Error::WorkspaceNotFound(_))
    ));
}
