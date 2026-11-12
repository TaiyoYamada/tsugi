//! Interrupts every operation at every step that matters and checks that the
//! next command leaves the repository in a state the design promises.

mod common;

use common::{Sandbox, commit_all, git, read, write};

/// Git must consider the repository intact after anything tsugi does.
fn assert_intact(s: &Sandbox) {
    git(&s.root, &["fsck", "--no-dangling", "--no-progress"]);
    git(&s.root, &["worktree", "prune", "--dry-run"]);
}

#[test]
fn snapshot_interrupted_before_its_refs_are_written() {
    let s = Sandbox::new();
    write(&s.root, "a.txt", "a\n");
    s.crash("snapshot.after-commit", &["snapshot"]);

    // The commit object exists but no ref points to it: the snapshot never
    // became visible, and the sequence number was not used up.
    assert_eq!(s.json(&["log"]).as_array().unwrap().len(), 0);
    let next = s.json(&["snapshot"]);
    assert_eq!(next["created"], true);
    assert_eq!(next["snapshot"]["seq"], 1);
    assert_intact(&s);
}

#[test]
fn restore_interrupted_before_any_file_changed() {
    let s = Sandbox::new();
    write(&s.root, "a.txt", "old\n");
    s.ok(&["snapshot"]);
    write(&s.root, "a.txt", "new\n");
    s.crash("restore.after-snapshot", &["restore", "@1"]);

    let next = s.run(&["status"]);
    assert!(String::from_utf8_lossy(&next.stderr).contains("did not finish"));
    assert_eq!(read(&s.root, "a.txt"), "new\n");
    let operation = s.last_operation();
    assert_eq!(operation["state"], "failed");
    assert_eq!(operation["detail"]["kind"], "restore");
    assert_intact(&s);
}

#[test]
fn restore_interrupted_after_the_files_changed() {
    let s = Sandbox::new();
    write(&s.root, "a.txt", "old\n");
    s.ok(&["snapshot"]);
    commit_all(&s.root, "later commit");
    write(&s.root, "a.txt", "new\n");
    let head = git(&s.root, &["rev-parse", "HEAD"]);
    s.crash("restore.after-files", &["restore", "@1"]);

    // Files are restored, the branch is not: exactly the half-way state the
    // note describes. Nothing is lost, because the state before is snapshot 2.
    assert_eq!(read(&s.root, "a.txt"), "old\n");
    assert_eq!(git(&s.root, &["rev-parse", "HEAD"]), head);
    let operation = s.last_operation();
    assert_eq!(operation["state"], "failed");
    assert!(operation["note"].as_str().unwrap().contains("snapshot 2"));

    s.ok(&["restore", "@2"]);
    assert_eq!(read(&s.root, "a.txt"), "new\n");
    assert_eq!(git(&s.root, &["rev-parse", "HEAD"]), head);
    assert_intact(&s);
}

#[test]
fn new_workspace_interrupted_is_removed_again() {
    for failpoint in ["ws_new.after-worktree", "ws_new.after-copy"] {
        let s = Sandbox::with_config("[workspace]\ncopy = [\"ignored\"]\n");
        write(&s.root, "ignored/dep.txt", "dep\n");
        s.crash(failpoint, &["ws", "new", "half"]);
        assert!(s.workspace("half").exists());

        let status = s.json(&["status"]);
        assert_eq!(status.as_array().unwrap().len(), 1, "{failpoint}");
        assert!(!s.workspace("half").exists(), "{failpoint}");
        assert_eq!(git(&s.root, &["branch", "--list", "tsugi/half"]), "");
        assert_eq!(s.last_operation()["state"], "failed");

        // The name is free again.
        s.ok(&["ws", "new", "half"]);
        assert_eq!(read(&s.workspace("half"), "ignored/dep.txt"), "dep\n");
        assert_intact(&s);
    }
}

#[test]
fn removal_interrupted_is_completed() {
    for failpoint in ["ws_remove.after-snapshot", "ws_remove.after-worktree"] {
        let s = Sandbox::new();
        s.ok(&["ws", "new", "leaving"]);
        write(&s.workspace("leaving"), "work.txt", "work\n");
        s.crash(failpoint, &["ws", "rm", "leaving"]);

        let status = s.json(&["status"]);
        assert_eq!(status.as_array().unwrap().len(), 1, "{failpoint}");
        assert!(!s.workspace("leaving").exists(), "{failpoint}");
        let operation = s.last_operation();
        assert_eq!(operation["state"], "done");
        assert_eq!(operation["detail"]["branch_deleted"], true);

        // The work is in the snapshot, and undo brings the workspace back.
        s.ok(&["undo"]);
        assert_eq!(read(&s.workspace("leaving"), "work.txt"), "work\n");
        assert_intact(&s);
    }
}

#[test]
fn sync_interrupted_can_be_rolled_back_by_hand() {
    for failpoint in ["sync.after-merge", "sync.after-files"] {
        let s = Sandbox::new();
        s.ok(&["ws", "new", "feature"]);
        let ws = s.workspace("feature");
        write(&ws, "feature.txt", "feature\n");
        write(&s.root, "main.txt", "main\n");
        commit_all(&s.root, "main moves");
        let head = git(&ws, &["rev-parse", "HEAD"]);
        s.crash(failpoint, &["ws", "sync", "feature"]);

        let operation = s.json(&["op", "log", "-n", "1"])[0].clone();
        assert_eq!(operation["state"], "failed", "{failpoint}");
        assert_eq!(git(&ws, &["rev-parse", "HEAD"]), head);
        assert_eq!(read(&ws, "feature.txt"), "feature\n");

        let pre = operation["detail"]["pre"].as_str().unwrap();
        let log = s.json(&["log", "feature"]);
        let seq = log
            .as_array()
            .unwrap()
            .iter()
            .find(|snapshot| snapshot["id"] == pre)
            .unwrap()["seq"]
            .as_u64()
            .unwrap();
        s.ok(&["restore", &format!("feature@{seq}")]);
        assert!(!ws.join("main.txt").exists(), "{failpoint}");

        // And the sync can simply be run again.
        s.ok(&["ws", "sync", "feature"]);
        assert_eq!(read(&ws, "main.txt"), "main\n");
        assert_eq!(read(&ws, "feature.txt"), "feature\n");
        assert_intact(&s);
    }
}

#[test]
fn publish_interrupted_before_the_base_branch_moved() {
    for failpoint in ["publish.before-verify", "publish.before-move"] {
        let s = Sandbox::with_config("[publish]\nverify = [\"true\"]\n");
        s.ok(&["ws", "new", "feature"]);
        write(&s.workspace("feature"), "feature.txt", "feature\n");
        let tip = git(&s.root, &["rev-parse", "main"]);
        s.crash(failpoint, &["publish", "feature"]);

        s.ok(&["status"]);
        assert_eq!(git(&s.root, &["rev-parse", "main"]), tip, "{failpoint}");
        assert_eq!(s.last_operation()["state"], "failed");
        assert_eq!(git(&s.root, &["worktree", "list"]).lines().count(), 2);

        s.ok(&["publish", "feature"]);
        assert_eq!(read(&s.root, "feature.txt"), "feature\n");
        assert_intact(&s);
    }
}

#[test]
fn publish_interrupted_after_the_base_branch_moved() {
    let s = Sandbox::new();
    s.ok(&["ws", "new", "feature"]);
    let ws = s.workspace("feature");
    write(&ws, "feature.txt", "feature\n");
    commit_all(&ws, "agent commit");
    s.crash("publish.after-move", &["publish", "feature"]);

    let next = s.run(&["status"]);
    assert!(String::from_utf8_lossy(&next.stderr).contains("tsugi ws sync"));
    let operation = s.last_operation();
    assert_eq!(operation["state"], "done");
    assert_eq!(operation["detail"]["base_moved"], true);
    assert_eq!(read(&s.root, "feature.txt"), "feature\n");

    // The base branch has the work; the workspace catches up with a sync.
    s.ok(&["ws", "sync", "feature"]);
    assert_eq!(
        git(&ws, &["rev-parse", "HEAD"]),
        git(&s.root, &["rev-parse", "main"])
    );
    assert_eq!(git(&ws, &["status", "--porcelain"]), "");
    assert_intact(&s);
}
