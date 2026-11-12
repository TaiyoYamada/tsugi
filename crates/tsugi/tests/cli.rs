mod common;

use common::{Sandbox, commit_all, git, read, write};

#[test]
fn a_task_from_start_to_publish() {
    let s = Sandbox::new();

    let task = s.json(&["task", "new", "Add a greeting"]);
    let created = s.json(&["ws", "new", "--task", "Add a greeting", "-n", "2"]);
    assert_eq!(created[0]["name"], "add-a-greeting");
    assert_eq!(created[1]["name"], "add-a-greeting-2");
    assert_eq!(created[0]["task_id"], task["id"]);

    let first = s.workspace("add-a-greeting");
    let second = s.workspace("add-a-greeting-2");
    write(&first, "hello.txt", "hello\n");
    write(&second, "hello.txt", "hi\n");
    write(&second, "extra.txt", "extra\n");

    let status = s.json(&["status"]);
    assert_eq!(status.as_array().unwrap().len(), 3);
    assert_eq!(status[0]["is_main"], true);
    assert_eq!(status[1]["task"]["title"], "Add a greeting");
    assert_eq!(status[1]["branch"], "refs/heads/tsugi/add-a-greeting");
    assert_eq!(status[1]["changes"]["files"], 1);
    assert_eq!(status[2]["changes"]["files"], 2);
    assert_eq!(status[1]["latest_snapshot"]["trigger"], "auto");

    let conflicts = s.json(&["conflicts"]);
    assert_eq!(conflicts[0]["workspace"], "add-a-greeting");
    assert_eq!(conflicts[0]["other"], "add-a-greeting-2");
    assert_eq!(conflicts[0]["files"][0], "hello.txt");

    // Inside a workspace the workspace argument can be left out.
    let path = s
        .command(&["ws", "path"])
        .current_dir(&second)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&path.stdout).trim(),
        second.to_str().unwrap()
    );

    let diff = s.json(&["diff", "add-a-greeting-2"]);
    assert_eq!(diff["stat"]["insertions"], 2);
    assert!(
        s.ok(&["diff", "add-a-greeting-2", "--stat"])
            .contains("extra.txt")
    );

    let published = s.json(&["publish", "add-a-greeting-2"]);
    assert_eq!(published["base"], "main");
    assert_eq!(read(&s.root, "hello.txt"), "hi\n");
    assert_eq!(
        git(&s.root, &["log", "-1", "--format=%s"]),
        "Add a greeting"
    );

    // The other attempt now conflicts with main and cannot be published.
    let refused = s.run(&["--json", "publish", "add-a-greeting"]);
    assert!(!refused.status.success());
    let error: serde_json::Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(error["error"]["code"], "conflict");

    s.ok(&["ws", "rm", "add-a-greeting"]);
    assert!(!first.exists());
    s.ok(&["task", "close", "Add a greeting"]);
    assert!(s.json(&["task", "ls"])[0]["closed_at"].is_number());
}

#[test]
fn snapshots_can_be_listed_compared_and_restored() {
    let s = Sandbox::new();
    write(&s.root, "notes.txt", "one\n");
    assert_eq!(s.json(&["snapshot"])["snapshot"]["seq"], 1);
    write(&s.root, "notes.txt", "two\n");
    assert_eq!(s.json(&["snapshot"])["created"], true);
    assert_eq!(s.json(&["snapshot"])["created"], false);

    let log = s.json(&["log"]);
    assert_eq!(log[0]["seq"], 2);
    assert_eq!(log[1]["seq"], 1);
    assert_eq!(s.json(&["log", "-n", "1"]).as_array().unwrap().len(), 1);

    let diff = s.json(&["diff", "--from", "@1", "--to", "@2"]);
    assert!(diff["patch"].as_str().unwrap().contains("-one\n+two"));

    let restored = s.json(&["restore", "@1"]);
    assert_eq!(restored["restored"]["seq"], 1);
    assert_eq!(read(&s.root, "notes.txt"), "one\n");
    s.ok(&["undo"]);
    assert_eq!(read(&s.root, "notes.txt"), "two\n");
}

#[test]
fn errors_have_a_code_and_a_nonzero_exit() {
    let s = Sandbox::new();
    let output = s.run(&["--json", "ws", "path", "missing"]);
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["code"], "workspace_not_found");

    let output = s.run(&["restore", "repo@99"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no snapshot matches"));

    let outside = tempfile::tempdir().unwrap();
    let output = s
        .command(&["--json", "status"])
        .current_dir(outside.path())
        .output()
        .unwrap();
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(error["error"]["code"], "not_a_repository");
}

#[test]
fn it_works_when_called_from_a_git_hook() {
    // Git exports GIT_DIR and GIT_INDEX_FILE to hooks. tsugi must not let
    // them redirect its own git commands.
    let s = Sandbox::new();
    write(
        &s.root,
        ".git/hooks/post-commit",
        &format!(
            "#!/bin/sh\nHOME='{}' '{}' snapshot >/dev/null\n",
            s.dir.path().display(),
            env!("CARGO_BIN_EXE_tsugi")
        ),
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let hook = s.root.join(".git/hooks/post-commit");
        std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    write(&s.root, "a.txt", "a\n");
    let head = commit_all(&s.root, "commit with a hook");
    let log = s.json(&["log"]);
    assert_eq!(log[0]["head"], head.as_str());
    assert_eq!(git(&s.root, &["status", "--porcelain"]), "");
}

#[test]
fn uninstall_leaves_no_trace() {
    let s = Sandbox::new();
    s.ok(&["ws", "new", "kept"]);
    s.ok(&["snapshot", "kept"]);
    let output = s.ok(&["uninstall"]);
    assert!(output.contains("repo.tsugi/kept"));
    assert!(!s.root.join(".git/tsugi").exists());
    assert_eq!(git(&s.root, &["for-each-ref", "refs/tsugi/"]), "");
    assert!(s.workspace("kept").exists());
    let after = s.run(&["status"]);
    assert!(String::from_utf8_lossy(&after.stderr).contains("tsugi init"));
}
