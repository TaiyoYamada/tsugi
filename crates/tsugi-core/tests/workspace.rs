mod common;

use std::fs;

use common::{TestRepo, commit_all, git, read, write};
use tsugi_core::{Error, OpState, Origin, Trigger};

#[test]
fn new_workspace_is_a_worktree_on_its_own_branch() {
    let t = TestRepo::new();
    let task = t.repo.task_new("Fix login").unwrap();
    let ws = t.repo.workspace_new("login", Some(&task), None).unwrap();

    assert_eq!(ws.path, t.root.parent().unwrap().join("repo.tsugi/login"));
    assert_eq!(ws.origin, Origin::Created);
    assert_eq!(ws.task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(
        git(&ws.path, &["symbolic-ref", "HEAD"]),
        "refs/heads/tsugi/login"
    );
    assert_eq!(read(&ws.path, "README.md"), "# test\n");
    assert_eq!(t.repo.workspace("login").unwrap(), ws);
    assert_eq!(t.repo.workspace(&ws.id[..6]).unwrap(), ws);
    assert_eq!(t.repo.workspace_containing(&ws.path).unwrap(), ws);
}

#[test]
fn new_workspace_rejects_bad_and_taken_names() {
    let t = TestRepo::new();
    t.repo.workspace_new("one", None, None).unwrap();
    assert!(matches!(
        t.repo.workspace_new("one", None, None),
        Err(Error::NameTaken(_))
    ));
    assert!(matches!(
        t.repo.workspace_new("Bad Name", None, None),
        Err(Error::InvalidName(_))
    ));
    assert!(matches!(
        t.repo.workspace_new("two", None, Some("no-such-branch")),
        Err(Error::Refused(_))
    ));
    git(&t.root, &["branch", "tsugi/three"]);
    assert!(matches!(
        t.repo.workspace_new("three", None, None),
        Err(Error::Refused(_))
    ));
    // A refused command leaves nothing behind.
    assert_eq!(t.repo.workspaces().unwrap().len(), 2);
    assert!(!t.root.parent().unwrap().join("repo.tsugi/two").exists());
}

#[test]
fn new_workspace_receives_configured_ignored_paths() {
    let t = TestRepo::with_config("[workspace]\ncopy = [\"ignored\", \"missing\"]\n");
    write(&t.root, "ignored/deps/lib.txt", "dependency\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        write(&t.root, "ignored/run.sh", "#!/bin/sh\n");
        let script = t.root.join("ignored/run.sh");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let ws = t.repo.workspace_new("deps", None, None).unwrap();
    assert_eq!(read(&ws.path, "ignored/deps/lib.txt"), "dependency\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(ws.path.join("ignored/run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }
    // The copy is independent of the original.
    write(&ws.path, "ignored/deps/lib.txt", "changed\n");
    assert_eq!(read(&t.root, "ignored/deps/lib.txt"), "dependency\n");
}

#[test]
fn copy_must_stay_inside_the_repository() {
    let t = TestRepo::with_config("[workspace]\ncopy = [\"../outside\"]\n");
    assert!(matches!(
        t.repo.workspace_new("x", None, None),
        Err(Error::Config(_))
    ));
}

#[test]
fn worktrees_made_by_git_are_adopted_and_retired() {
    let t = TestRepo::new();
    let path = t.dir.path().join("By Hand");
    git(
        &t.root,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "hand",
            path.to_str().unwrap(),
        ],
    );
    let repo = t.reopen();
    let statuses = repo.status(None).unwrap();
    let adopted = statuses
        .iter()
        .find(|s| s.workspace.name == "by-hand")
        .unwrap();
    assert_eq!(adopted.workspace.origin, Origin::Adopted);
    assert_eq!(adopted.branch.as_deref(), Some("refs/heads/hand"));
    assert!(!adopted.is_main);
    assert!(adopted.latest_snapshot.is_some());

    git(
        &t.root,
        &["worktree", "remove", "--force", path.to_str().unwrap()],
    );
    let names: Vec<String> = repo
        .status(None)
        .unwrap()
        .into_iter()
        .map(|s| s.workspace.name)
        .collect();
    assert_eq!(names, ["repo"]);
}

#[test]
fn removed_workspace_keeps_its_content_and_can_be_brought_back() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("gone", None, None).unwrap();
    write(&ws.path, "committed.txt", "c\n");
    commit_all(&ws.path, "agent commit");
    write(&ws.path, "uncommitted.txt", "u\n");

    let removal = t.repo.workspace_remove(&ws).unwrap();
    assert!(!ws.path.exists());
    assert!(t.repo.workspace("gone").is_err());
    // The branch has a commit that is not on main, so it stays.
    assert!(!removal.branch_deleted);
    assert!(t.repo.take_notices()[0].contains("kept the branch"));

    let undone = t.repo.undo().unwrap();
    assert_eq!(undone.detail.kind(), "ws_remove");
    let back = t.repo.workspace("gone").unwrap();
    assert_eq!(back.id, ws.id);
    assert_eq!(read(&back.path, "committed.txt"), "c\n");
    assert_eq!(read(&back.path, "uncommitted.txt"), "u\n");
    assert_eq!(
        git(&back.path, &["status", "--porcelain"]),
        "?? uncommitted.txt"
    );
}

#[test]
fn removing_an_untouched_workspace_deletes_its_branch() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("clean", None, None).unwrap();
    let removal = t.repo.workspace_remove(&ws).unwrap();
    assert!(removal.branch_deleted);
    assert_eq!(git(&t.root, &["branch", "--list", "tsugi/clean"]), "");
}

#[test]
fn the_main_worktree_cannot_be_removed() {
    let t = TestRepo::new();
    assert!(matches!(
        t.repo.workspace_remove(&t.main()),
        Err(Error::Refused(_))
    ));
}

#[test]
fn undo_of_new_workspace_removes_it() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("oops", None, None).unwrap();
    write(&ws.path, "work.txt", "w\n");
    t.repo.undo().unwrap();
    assert!(!ws.path.exists());
    // The work is still in a snapshot.
    let snapshots = t.repo.snapshots(&ws).unwrap();
    assert_eq!(snapshots[0].trigger, Trigger::PreRemove);
    assert_eq!(
        git(&t.root, &["show", &format!("{}:work.txt", snapshots[0].id)]),
        "w"
    );
    let operations = t.repo.operations(10).unwrap();
    assert_eq!(operations[0].detail.kind(), "undo");
    assert_eq!(operations[1].state, OpState::Undone);
    assert!(matches!(t.repo.undo(), Err(Error::NothingToUndo)));
}

#[test]
fn sync_merges_the_base_branch_into_the_workspace() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "feature.txt", "feature\n");
    write(&t.root, "main.txt", "main\n");
    let tip = commit_all(&t.root, "main moves");

    let outcome = t.repo.workspace_sync(&ws).unwrap();
    assert_eq!(outcome.onto, tip);
    assert_eq!(read(&ws.path, "main.txt"), "main\n");
    assert_eq!(read(&ws.path, "feature.txt"), "feature\n");
    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), tip);
    assert_eq!(git(&ws.path, &["status", "--porcelain"]), "?? feature.txt");

    t.repo.undo().unwrap();
    assert!(!ws.path.join("main.txt").exists());
    assert_eq!(read(&ws.path, "feature.txt"), "feature\n");
    assert_ne!(git(&ws.path, &["rev-parse", "HEAD"]), tip);
}

#[test]
fn sync_stops_on_conflict_without_changing_anything() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("feature", None, None).unwrap();
    write(&ws.path, "README.md", "# from the workspace\n");
    write(&t.root, "README.md", "# from main\n");
    commit_all(&t.root, "main edits the same line");
    let head = git(&ws.path, &["rev-parse", "HEAD"]);

    match t.repo.workspace_sync(&ws) {
        Err(Error::Conflict { files }) => assert_eq!(files, ["README.md"]),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(read(&ws.path, "README.md"), "# from the workspace\n");
    assert_eq!(git(&ws.path, &["rev-parse", "HEAD"]), head);
    assert_eq!(t.repo.operations(1).unwrap()[0].state, OpState::Failed);
}

#[test]
fn gc_deletes_old_snapshots_but_never_the_latest_of_a_live_workspace() {
    let t = TestRepo::with_config("[snapshot]\nkeep_days = 2\n");
    let main = t.main();
    let ws = t.repo.workspace_new("temp", None, None).unwrap();
    for content in ["1\n", "2\n", "3\n"] {
        write(&t.root, "a.txt", content);
        t.repo.snapshot(&main, Trigger::Manual).unwrap();
    }
    write(&ws.path, "b.txt", "b\n");
    t.repo.workspace_remove(&ws).unwrap();
    let taken = t.repo.snapshots(&main).unwrap()[0].created_at;
    const DAY: i64 = 24 * 60 * 60;

    // One day later everything is still there.
    assert_eq!(t.repo.gc_at(taken + DAY).unwrap().deleted, 0);

    // Three days later only the latest snapshot of the live workspace is.
    let outcome = t.repo.gc_at(taken + 3 * DAY).unwrap();
    assert_eq!(outcome.kept, 1);
    let left = t.repo.snapshots(&main).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].seq, 3);
    assert!(t.repo.snapshots(&ws).unwrap().is_empty());
    assert_eq!(
        git(
            &t.root,
            &["for-each-ref", "--format=%(refname)", "refs/tsugi/"]
        )
        .lines()
        .count(),
        2
    );
    // The next snapshot continues the numbering.
    write(&t.root, "a.txt", "4\n");
    assert_eq!(
        t.repo
            .snapshot(&main, Trigger::Manual)
            .unwrap()
            .snapshot
            .seq,
        4
    );
}

#[test]
fn gc_thins_automatic_snapshots_that_are_older_than_an_hour() {
    let t = TestRepo::new();
    let main = t.main();
    // Three automatic snapshots, one on request, two more automatic ones,
    // all within the same seconds.
    let triggers = [
        Trigger::Watch,
        Trigger::Auto,
        Trigger::Watch,
        Trigger::Manual,
        Trigger::PreRestore,
        Trigger::Watch,
        Trigger::Auto,
    ];
    for (index, trigger) in triggers.into_iter().enumerate() {
        write(&t.root, "a.txt", &format!("{index}\n"));
        t.repo.snapshot(&main, trigger).unwrap();
    }
    let taken = t.repo.snapshots(&main).unwrap()[0].created_at;
    // The test may cross a ten-minute boundary of the clock while it takes
    // the snapshots; then one more automatic snapshot is kept.
    let periods = {
        let snapshots = t.repo.snapshots(&main).unwrap();
        let mut periods: Vec<i64> = snapshots
            .iter()
            .filter(|s| matches!(s.trigger, Trigger::Watch | Trigger::Auto))
            .map(|s| s.created_at / 600)
            .collect();
        periods.dedup();
        periods.len()
    };

    // Within the hour nothing is thinned.
    assert_eq!(t.repo.gc_at(taken + 30 * 60).unwrap().deleted, 0);

    t.repo.gc_at(taken + 2 * 60 * 60).unwrap();
    let left = t.repo.snapshots(&main).unwrap();
    let kept: Vec<(u64, Trigger)> = left.iter().map(|s| (s.seq, s.trigger)).collect();
    // The newest automatic snapshot of the period, and everything that was
    // asked for or taken before an operation.
    assert!(kept.contains(&(7, Trigger::Auto)));
    assert!(kept.contains(&(5, Trigger::PreRestore)));
    assert!(kept.contains(&(4, Trigger::Manual)));
    assert_eq!(kept.len(), 2 + periods);
}

#[test]
fn uninstall_leaves_a_plain_repository() {
    let t = TestRepo::new();
    let ws = t.repo.workspace_new("stays", None, None).unwrap();
    write(&ws.path, "a.txt", "a\n");
    t.repo.snapshot(&ws, Trigger::Manual).unwrap();
    let state_dir = t.repo.state_dir().to_path_buf();

    let outcome = t.reopen().uninstall().unwrap();
    assert!(outcome.refs_deleted >= 2);
    assert_eq!(outcome.worktrees, std::slice::from_ref(&ws.path));
    assert!(!state_dir.exists());
    assert_eq!(git(&t.root, &["for-each-ref", "refs/tsugi/"]), "");
    assert_eq!(read(&ws.path, "a.txt"), "a\n");
    git(&t.root, &["fsck", "--no-dangling"]);
}
