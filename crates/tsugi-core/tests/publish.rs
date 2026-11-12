mod common;

use common::{TestRepo, commit_all, git, read, write};
use tsugi_core::{Error, OpState};

#[test]
fn publish_puts_one_commit_on_the_base_branch() {
    let t = TestRepo::new();
    let task = t.repo.task_new("Add greeting").unwrap();
    let ws = t.repo.workspace_new("greet", Some(&task), None).unwrap();
    write(&ws.path, "hello.txt", "hello\n");
    commit_all(&ws.path, "wip 1");
    write(&ws.path, "hello.txt", "hello, world\n");
    commit_all(&ws.path, "wip 2");
    write(&ws.path, "uncommitted.txt", "also published\n");
    let before = git(&t.root, &["rev-parse", "main"]);

    let outcome = t.repo.publish(&ws, None, true).unwrap();
    assert!(outcome.synced);
    assert_eq!(outcome.stat.files, 2);

    // One commit, whose parent is the old tip, with the task as message.
    assert_eq!(git(&t.root, &["rev-parse", "main"]), outcome.commit);
    assert_eq!(git(&t.root, &["rev-parse", "main~1"]), before);
    assert_eq!(
        git(&t.root, &["log", "-1", "--format=%s", "main"]),
        "Add greeting"
    );
    assert_eq!(git(&t.root, &["log", "-1", "--format=%an", "main"]), "Test");
    assert_eq!(
        git(&t.root, &["log", "--format=%s", "main"])
            .lines()
            .count(),
        2
    );

    // The main worktree has main checked out; its files followed.
    assert_eq!(read(&t.root, "hello.txt"), "hello, world\n");
    assert_eq!(read(&t.root, "uncommitted.txt"), "also published\n");
    assert_eq!(git(&t.root, &["status", "--porcelain"]), "");

    // The workspace sits on the published commit with nothing left to do.
    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), outcome.commit);
    assert_eq!(git(&ws.path, &["status", "--porcelain"]), "");
    assert!(matches!(
        t.repo.publish(&ws, None, true),
        Err(Error::NothingToPublish(_))
    ));
}

#[test]
fn publish_merges_with_a_base_branch_that_moved() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "feature.txt", "feature\n");
    write(&t.root, "main.txt", "main\n");
    commit_all(&t.root, "main moves");

    let outcome = t.repo.publish(&ws, Some("Add feature"), true).unwrap();
    assert_eq!(outcome.stat.files, 1);
    assert_eq!(read(&t.root, "feature.txt"), "feature\n");
    // The workspace received what happened on main.
    assert_eq!(read(&ws.path, "main.txt"), "main\n");

    // Work continues in the same workspace and publishes again cleanly.
    write(&ws.path, "feature.txt", "feature v2\n");
    t.repo.publish(&ws, Some("Improve feature"), true).unwrap();
    assert_eq!(read(&t.root, "feature.txt"), "feature v2\n");
}

#[test]
fn publish_updates_a_base_branch_that_is_not_checked_out() {
    let t = TestRepo::new();
    git(&t.root, &["branch", "release"]);
    let ws = t.repo.workspace_new("fix", None, Some("release")).unwrap();
    write(&ws.path, "fix.txt", "fix\n");
    let outcome = t.repo.publish(&ws, Some("Fix"), true).unwrap();
    assert_eq!(git(&t.root, &["rev-parse", "release"]), outcome.commit);
    assert!(!t.root.join("fix.txt").exists());
}

#[test]
fn publish_stops_on_conflict() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "README.md", "# workspace\n");
    write(&t.root, "README.md", "# main\n");
    let tip = commit_all(&t.root, "main edits the same line");

    assert!(matches!(
        t.repo.publish(&ws, None, true),
        Err(Error::Conflict { .. })
    ));
    assert_eq!(git(&t.root, &["rev-parse", "main"]), tip);
    assert_eq!(read(&ws.path, "README.md"), "# workspace\n");
}

#[test]
fn publish_refuses_to_overwrite_uncommitted_changes_on_the_base() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "README.md", "# workspace\n");
    write(&t.root, "README.md", "# uncommitted on main\n");
    let tip = git(&t.root, &["rev-parse", "main"]);

    // The snapshot of the main worktree is not part of main, so the merge is
    // clean; moving main would overwrite the uncommitted file.
    assert!(t.repo.publish(&ws, None, true).is_err());
    assert_eq!(git(&t.root, &["rev-parse", "main"]), tip);
    assert_eq!(read(&t.root, "README.md"), "# uncommitted on main\n");
    assert_eq!(t.repo.operations(1).unwrap()[0].state, OpState::Failed);
}

#[test]
fn failed_verification_leaves_the_base_branch_alone() {
    let t = TestRepo::with_config("[publish]\nverify = [\"test -f required.txt\"]\n");
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "other.txt", "x\n");
    let tip = git(&t.root, &["rev-parse", "main"]);

    match t.repo.publish(&ws, None, true) {
        Err(Error::VerifyFailed { command, code, log }) => {
            assert_eq!(command, "test -f required.txt");
            assert_eq!(code, Some(1));
            assert!(log.exists());
        }
        other => panic!("expected a failed verification, got {other:?}"),
    }
    assert_eq!(git(&t.root, &["rev-parse", "main"]), tip);
    assert_eq!(t.repo.operations(1).unwrap()[0].state, OpState::Failed);
    // The temporary worktree is gone and was never taken for a workspace.
    assert_eq!(git(&t.root, &["worktree", "list"]).lines().count(), 2);
    assert_eq!(t.repo.workspaces().unwrap().len(), 2);

    // Skipping verification is an explicit choice.
    t.repo.publish(&ws, None, false).unwrap();
    assert_ne!(git(&t.root, &["rev-parse", "main"]), tip);
}

#[test]
fn verification_runs_on_the_merged_commit_and_is_remembered() {
    let t = TestRepo::with_config(
        "[publish]\nverify = [\"test -f feature.txt && test -f main.txt && echo ran >> ../../verify-count\"]\n",
    );
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "feature.txt", "feature\n");
    write(&t.root, "main.txt", "main\n");
    commit_all(&t.root, "main moves");

    let outcome = t.repo.publish(&ws, None, true).unwrap();
    assert_eq!(outcome.verifications.len(), 1);
    assert!(outcome.verifications[0].passed);
    assert!(!outcome.verifications[0].cached);
    let count = t.repo.state_dir().join("verify-count");
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "ran\n");

    // Undo and publish the same content again: the tree is the same, so the
    // command is not run a second time.
    t.repo.undo().unwrap();
    let again = t.repo.publish(&ws, None, true).unwrap();
    assert!(again.verifications[0].cached);
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "ran\n");
}

#[test]
fn undo_of_publish_moves_the_base_branch_back() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "feature.txt", "feature\n");
    let agent_commit = commit_all(&ws.path, "agent commit");
    write(&ws.path, "more.txt", "more\n");
    let tip = git(&t.root, &["rev-parse", "main"]);
    t.repo.publish(&ws, None, true).unwrap();

    t.repo.undo().unwrap();
    assert_eq!(git(&t.root, &["rev-parse", "main"]), tip);
    assert!(!t.root.join("feature.txt").exists());
    // The workspace is as it was: the agent's commit and the loose file.
    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), agent_commit);
    assert_eq!(git(&ws.path, &["status", "--porcelain"]), "?? more.txt");
}

#[test]
fn undo_of_publish_is_refused_once_the_base_branch_moved_on() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "feature.txt", "feature\n");
    t.repo.publish(&ws, None, true).unwrap();
    write(&t.root, "later.txt", "later\n");
    let tip = commit_all(&t.root, "later commit");

    assert!(matches!(t.repo.undo(), Err(Error::Refused(_))));
    assert_eq!(git(&t.root, &["rev-parse", "main"]), tip);
}

#[test]
fn the_base_workspace_cannot_be_published() {
    let t = TestRepo::new();
    write(&t.root, "a.txt", "a\n");
    assert!(matches!(
        t.repo.publish(&t.main(), None, true),
        Err(Error::Refused(_))
    ));
}

#[test]
fn conflicts_are_found_between_workspaces_and_with_the_base() {
    let t = TestRepo::new();
    let one = t.repo.workspace_new("one", None, None).unwrap();
    let two = t.repo.workspace_new("two", None, None).unwrap();
    let three = t.repo.workspace_new("three", None, None).unwrap();
    write(&one.path, "shared.txt", "from one\n");
    write(&two.path, "shared.txt", "from two\n");
    write(&three.path, "separate.txt", "three\n");
    assert_eq!(t.repo.conflicts().unwrap().len(), 1);

    write(&t.root, "separate.txt", "main\n");
    commit_all(&t.root, "main adds the same file");
    let conflicts = t.repo.conflicts().unwrap();
    let found: Vec<(&str, &str, bool)> = conflicts
        .iter()
        .map(|c| (c.workspace.as_str(), c.other.as_str(), c.other_is_base))
        .collect();
    // The main worktree is a workspace too: `three` conflicts with it and
    // with the branch it has checked out.
    assert_eq!(
        found,
        [
            ("repo", "three", false),
            ("one", "two", false),
            ("three", "main", true)
        ]
    );
    assert_eq!(conflicts[1].files, ["shared.txt"]);
    // Nothing was written to any worktree.
    assert_eq!(read(&one.path, "shared.txt"), "from one\n");
    assert_eq!(read(&two.path, "shared.txt"), "from two\n");
}

#[test]
fn diff_shows_what_a_workspace_changed_since_it_left_the_base() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "README.md", "# test\nmore\n");
    write(&ws.path, "new.txt", "new\n");
    write(&t.root, "unrelated.txt", "main\n");
    commit_all(&t.root, "main moves");

    let diff = t.repo.diff_workspace(&ws).unwrap();
    assert_eq!(diff.stat.files, 2);
    assert_eq!(diff.stat.insertions, 2);
    assert_eq!(diff.stat.deletions, 0);
    assert!(diff.patch.contains("+more"));
    assert!(!diff.patch.contains("unrelated.txt"));
}
