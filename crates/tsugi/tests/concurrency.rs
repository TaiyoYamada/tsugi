//! Several processes at once, and the watcher.

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::{Sandbox, commit_all, git, write};

#[test]
fn processes_snapshotting_at_once_never_share_a_sequence_number() {
    let s = Sandbox::new();
    std::thread::scope(|scope| {
        // An agent that edits and commits while tsugi takes snapshots.
        scope.spawn(|| {
            for round in 0..15 {
                write(&s.root, "work.txt", &format!("round {round}\n"));
                if round % 3 == 0 {
                    commit_all(&s.root, &format!("round {round}"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..8 {
                    s.ok(&["snapshot"]);
                }
            });
        }
    });

    let log = s.json(&["log", "-n", "1000"]);
    let snapshots = log.as_array().unwrap();
    let sequence: Vec<u64> = snapshots
        .iter()
        .map(|snapshot| snapshot["seq"].as_u64().unwrap())
        .collect();
    let distinct: HashSet<&u64> = sequence.iter().collect();
    assert_eq!(distinct.len(), sequence.len());
    assert_eq!(sequence[0] as usize, sequence.len(), "no gaps");
    // `commit_all` panics if git fails, so reaching this line also means
    // tsugi never got in the way of the agent's own git commands.
    git(&s.root, &["fsck", "--no-dangling", "--no-progress"]);
}

#[test]
fn commands_run_while_a_publish_is_being_verified() {
    let s = Sandbox::with_config("[publish]\nverify = [\"sleep 3\"]\n");
    s.ok(&["ws", "new", "slow"]);
    s.ok(&["ws", "new", "other"]);
    write(&s.workspace("slow"), "slow.txt", "slow\n");

    std::thread::scope(|scope| {
        let publish = scope.spawn(|| s.ok(&["publish", "slow"]));
        std::thread::sleep(Duration::from_millis(1200));

        // The publish is in the middle of `sleep 3`. Nothing waits for it,
        // and nothing mistakes it for an interrupted operation.
        let started = Instant::now();
        write(&s.workspace("other"), "other.txt", "other\n");
        assert_eq!(s.json(&["snapshot", "other"])["created"], true);
        let operations = s.json(&["op", "log", "-n", "1"]);
        assert_eq!(operations[0]["state"], "started");
        assert!(started.elapsed() < Duration::from_millis(1500));

        publish.join().unwrap();
    });
    assert_eq!(s.last_operation()["state"], "done");
    assert!(s.root.join("slow.txt").exists());
}

#[test]
fn the_watcher_snapshots_after_files_change_and_after_commits() {
    let s = Sandbox::new();
    s.ok(&["ws", "new", "watched"]);
    let ws = s.workspace("watched");
    assert_eq!(s.json(&["watch", "start"])["started"], true);
    assert_eq!(s.json(&["watch", "start"])["started"], false);
    assert_eq!(s.json(&["watch", "status"])["running"], true);

    // Polls `log` without `status`, so that only the watcher can have taken
    // the snapshot.
    let wait_for = |what: &str, found: &dyn Fn(&serde_json::Value) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let log = s.json(&["log", "watched"]);
            if log.as_array().unwrap().iter().any(found) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the watcher took no snapshot after {what}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };

    // The first refresh of the watcher must have picked the workspace up.
    std::thread::sleep(Duration::from_millis(1500));
    write(&ws, "edited.txt", "edited\n");
    wait_for("an edit", &|snapshot| {
        snapshot["trigger"] == "watch"
            && git(
                &s.root,
                &["ls-tree", "--name-only", snapshot["id"].as_str().unwrap()],
            )
            .contains("edited.txt")
    });

    let head = commit_all(&ws, "agent commit");
    wait_for("a commit", &|snapshot| {
        snapshot["trigger"] == "watch" && snapshot["head"] == head.as_str()
    });

    // Changes to ignored files do not produce snapshots.
    let before = s.json(&["log", "watched"]).as_array().unwrap().len();
    write(&ws, "ignored/build.o", "object\n");
    std::thread::sleep(Duration::from_millis(3000));
    assert_eq!(
        s.json(&["log", "watched"]).as_array().unwrap().len(),
        before
    );

    assert_eq!(s.json(&["watch", "stop"])["stopped"], true);
    assert_eq!(s.json(&["watch", "status"])["running"], false);
    assert_eq!(s.json(&["watch", "stop"])["stopped"], false);
}
