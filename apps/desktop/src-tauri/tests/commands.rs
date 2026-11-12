//! Calls the commands the way the window does: by name, with JSON
//! arguments, through Tauri's IPC. This checks the wiring (names, argument
//! names, the shape of results and errors); what the commands do is tested
//! in `tsugi-core`.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::{WebviewWindow, WebviewWindowBuilder};

/// Where the pages of the app itself are served from. Commands invoked
/// from any other origin are refused.
#[cfg(any(windows, target_os = "android"))]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

fn window() -> WebviewWindow<MockRuntime> {
    let app = tsugi_desktop_lib::with_commands(mock_builder())
        .build(mock_context(noop_assets()))
        .unwrap();
    WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap()
}

/// Invokes a command and returns what the window would receive: the value
/// the promise resolves with, or the value it rejects with.
fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, args: Value) -> Result<Value, Value> {
    tauri::test::get_ipc_response(
        window,
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: LOCAL_ORIGIN.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize::<Value>().unwrap())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn the_window_can_drive_a_repository_from_setup_to_undo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repo");
    fs::create_dir(&path).unwrap();
    git(&path, &["init", "--quiet", "--initial-branch=main"]);
    git(&path, &["config", "user.name", "Test"]);
    git(&path, &["config", "user.email", "test@example.com"]);
    git(&path, &["config", "commit.gpgsign", "false"]);
    fs::write(path.join("README.md"), "# test\n").unwrap();
    git(&path, &["add", "--all"]);
    git(&path, &["commit", "--quiet", "--message", "init"]);
    let window = window();

    // Not set up yet: the picker relies on this exact code.
    let refused = invoke(&window, "open_repo", json!({ "path": path })).unwrap_err();
    assert_eq!(refused["code"], "not_initialized", "{refused}");
    assert!(refused["message"].is_string());

    let repo = invoke(&window, "init_repo", json!({ "path": path })).unwrap();
    let repo = repo.as_str().unwrap();
    let overview = invoke(&window, "overview", json!({ "repo": repo })).unwrap();
    assert_eq!(overview["default_base"], "main");
    assert_eq!(overview["statuses"].as_array().unwrap().len(), 1);
    assert_eq!(overview["statuses"][0]["is_main"], true);

    let task = invoke(
        &window,
        "task_new",
        json!({ "repo": repo, "title": "Greet" }),
    )
    .unwrap();
    let created = invoke(
        &window,
        "workspace_new",
        json!({ "repo": repo, "name": "greet", "task": task["value"]["id"], "base": null }),
    )
    .unwrap();
    let workspace = created["value"]["id"].as_str().unwrap();
    let workspace_path = created["value"]["path"].as_str().unwrap();
    fs::write(Path::new(workspace_path).join("hello.txt"), "hello\n").unwrap();

    let first = invoke(
        &window,
        "snapshot",
        json!({ "repo": repo, "workspace": workspace }),
    )
    .unwrap();
    assert_eq!(first["value"]["created"], true);
    fs::write(Path::new(workspace_path).join("hello.txt"), "hello again\n").unwrap();

    let detail = invoke(
        &window,
        "detail",
        json!({ "repo": repo, "workspace": workspace }),
    )
    .unwrap();
    assert_eq!(detail["diff"]["files"][0]["path"], "hello.txt");
    let snapshots = detail["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 1, "detail lists snapshots, it takes none");

    // While a watcher runs, `overview` reports the latest snapshots and
    // takes none itself.
    let second = invoke(
        &window,
        "snapshot",
        json!({ "repo": repo, "workspace": workspace }),
    )
    .unwrap();
    let overview = invoke(&window, "overview", json!({ "repo": repo })).unwrap();
    assert_eq!(
        overview["statuses"][1]["latest_snapshot"]["id"],
        second["value"]["snapshot"]["id"]
    );
    let between = invoke(
        &window,
        "diff",
        json!({
            "repo": repo,
            "from": first["value"]["snapshot"]["id"],
            "to": second["value"]["snapshot"]["id"],
        }),
    )
    .unwrap();
    assert!(between["patch"].as_str().unwrap().contains("+hello again"));

    let restored = invoke(
        &window,
        "restore",
        json!({ "repo": repo, "workspace": workspace, "snapshot": first["value"]["snapshot"]["id"] }),
    )
    .unwrap();
    assert_eq!(restored["value"]["restored"]["seq"], 1);
    assert_eq!(
        fs::read_to_string(Path::new(workspace_path).join("hello.txt")).unwrap(),
        "hello\n"
    );

    let published = invoke(
        &window,
        "publish",
        json!({ "repo": repo, "workspace": workspace, "message": null, "verify": true }),
    )
    .unwrap();
    assert_eq!(published["value"]["base"], "main");
    assert_eq!(git(&path, &["log", "-1", "--format=%s"]), "Greet");

    let undone = invoke(&window, "undo", json!({ "repo": repo })).unwrap();
    assert_eq!(undone["value"]["detail"]["kind"], "publish");
    assert!(!path.join("hello.txt").exists());

    let synced = invoke(
        &window,
        "workspace_sync",
        json!({ "repo": repo, "workspace": workspace }),
    )
    .unwrap();
    assert!(synced["value"]["onto"].is_string());
    let removed = invoke(
        &window,
        "workspace_remove",
        json!({ "repo": repo, "workspace": workspace }),
    )
    .unwrap();
    assert_eq!(removed["value"]["name"], "greet");
    let operations = invoke(&window, "operations", json!({ "repo": repo, "limit": 3 })).unwrap();
    assert_eq!(operations.as_array().unwrap().len(), 3);
    assert_eq!(operations[0]["detail"]["kind"], "ws_remove");

    // Opening the repository started a watcher in this process.
    assert_eq!(overview["watching"], true);
    let watching = invoke(
        &window,
        "set_watching",
        json!({ "repo": repo, "on": false }),
    )
    .unwrap();
    assert_eq!(watching, false);
    let watching = invoke(&window, "set_watching", json!({ "repo": repo, "on": true })).unwrap();
    assert_eq!(watching, true);

    let missing = invoke(
        &window,
        "detail",
        json!({ "repo": repo, "workspace": "nope" }),
    )
    .unwrap_err();
    assert_eq!(missing["code"], "workspace_not_found");
}
