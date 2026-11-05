// Copyright 2026 Renato Cavalcanti
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_jam-factory-mcp");
const TIMEOUT: Duration = Duration::from_secs(10);

const TOOLS: [&str; 13] = [
    "append_code",
    "append_diff",
    "append_file",
    "archive_session",
    "delete_file",
    "edit_file",
    "list_files",
    "list_sessions",
    "move_file",
    "read_file",
    "start_session",
    "update_session",
    "write_file",
];

/// An initialized server on a home folder, with its stdin and the lines of its stdout.
struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
    info: Value,
    home: Arc<TempDir>,
}

impl Server {
    fn start() -> Server {
        Server::start_on(Arc::new(tempfile::tempdir().unwrap()))
    }

    /// Starts a server on the home folder `home` and initializes it.
    fn start_on(home: Arc<TempDir>) -> Server {
        let mut child = Command::new(BINARY)
            .env("HOME", home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        let mut server = Server {
            child,
            stdin,
            lines,
            next_id: 0,
            info: Value::Null,
            home,
        };

        server.info = server.initialize();
        server.create("o", "r", "s");
        server
    }

    fn root(&self) -> PathBuf {
        self.home.path().join(".jam-factory").join("factory")
    }

    /// Creates the empty folder of the session `org/repo/main/session`.
    fn create(&self, org: &str, repo: &str, session: &str) {
        let dir = self
            .root()
            .join(org)
            .join(repo)
            .join("sessions")
            .join("main")
            .join(session);

        fs::create_dir_all(dir).unwrap();
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends the notification `method` without params.
    fn notify(&mut self, method: &str) {
        self.send(json!({ "jsonrpc": "2.0", "method": method }));
    }

    /// Sends the request `method` with `params` and returns the result of its reply.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;

        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));

        loop {
            let line = self.lines.recv_timeout(TIMEOUT).unwrap();
            let reply: Value = serde_json::from_str(&line).unwrap();

            if reply["id"] == id {
                assert!(reply.get("error").is_none(), "{method}: {reply}");
                return reply["result"].clone();
            }
        }
    }

    fn initialize(&mut self) -> Value {
        let params = json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "stdio-test", "version": "0" }
        });

        let result = self.request("initialize", params);
        self.notify("notifications/initialized");
        result
    }

    /// Calls the tool `name` with `arguments`. Returns its JSON reply, or its JSON error.
    fn call(&mut self, name: &str, arguments: Value) -> Result<Value, Value> {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1, "{name}: {result}");

        let text = content[0]["text"].as_str().unwrap();
        let reply: Value = serde_json::from_str(text).unwrap();

        if result["isError"] == true {
            Err(reply)
        } else {
            Ok(reply)
        }
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        self.call(name, arguments)
            .unwrap_or_else(|error| panic!("{name}: {error}"))
    }

    fn fail(&mut self, name: &str, arguments: Value) -> Value {
        match self.call(name, arguments) {
            Ok(reply) => panic!("{name} did not fail: {reply}"),
            Err(error) => error,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn in_session(fields: Value) -> Value {
    in_session_of("o", "r", "s", fields)
}

fn in_session_of(org: &str, repo: &str, session: &str, fields: Value) -> Value {
    let mut arguments = json!({ "org": org, "repo": repo, "branch": "main", "session": session });
    let extra = fields.as_object().unwrap().clone();

    arguments.as_object_mut().unwrap().extend(extra);
    arguments
}

fn names(sessions: &Value) -> Vec<String> {
    sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            format!(
                "{}/{}/{}/{}",
                entry["org"].as_str().unwrap(),
                entry["repo"].as_str().unwrap(),
                entry["branch"].as_str().unwrap(),
                entry["session"].as_str().unwrap()
            )
        })
        .collect()
}

/// The line of `worker` for its call `call`, marked `state`.
fn task_line(worker: usize, call: usize, state: &str) -> String {
    format!("task {worker}-{call}: {state}\n")
}

/// Runs git with `args` in `dir`, with `home` as its home folder, and returns its stdout.
fn git(home: &Path, dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .env("HOME", home)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();

    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// The 50 lines of `src/lib.rs`, with a new text on each line in `changed`.
fn lib_rs(changed: &[usize]) -> String {
    (1..=50)
        .map(|n| {
            if changed.contains(&n) {
                format!("// new line {n}\n")
            } else {
                format!("// line {n}\n")
            }
        })
        .collect()
}

/// Writes `lib` to `src/lib.rs` and `readme` to `README.md` in `dir`, and commits both.
fn commit(home: &Path, dir: &Path, lib: &str, readme: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src").join("lib.rs"), lib).unwrap();
    fs::write(dir.join("README.md"), readme).unwrap();

    git(home, dir, &["add", "."]);
    git(home, dir, &["commit", "-q", "-m", "change"]);
}

/// Returns a git repo with two commits, and writes a `session.json` whose `worktree` is that repo.
/// The second commit changes lines 2 and 41 of `src/lib.rs`, and adds a run of four backticks to `README.md`.
fn git_session(server: &mut Server) -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    let home = Arc::clone(&server.home);

    git(home.path(), repo.path(), &["init", "-q"]);
    commit(home.path(), repo.path(), &lib_rs(&[]), "# R\n");
    commit(home.path(), repo.path(), &lib_rs(&[2, 41]), "# R\n\n````\n");

    let session = json!({ "id": "s", "worktree": repo.path() });
    server.ok(
        "write_file",
        in_session(json!({ "path": "session.json", "content": session.to_string() })),
    );

    repo
}

/// The files of the session `o/r/main/s`.
fn session_files(server: &mut Server) -> Value {
    server.ok("list_files", in_session(json!({})))
}

#[test]
fn initialize_names_the_server() {
    let server = Server::start();

    assert_eq!(server.info["serverInfo"]["name"], "jam-factory-mcp");
}

#[test]
fn tools_list_has_the_thirteen_tools() {
    let mut server = Server::start();

    let reply = server.request("tools/list", json!({}));
    let tools = reply["tools"].as_array().unwrap();

    let mut listed: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    listed.sort();

    assert_eq!(listed, TOOLS);

    for tool in tools {
        let description = tool["description"].as_str().unwrap();
        assert!(description.ends_with('.'), "{tool}");
    }

    let write = tools
        .iter()
        .find(|tool| tool["name"] == "write_file")
        .unwrap();
    let schema = &write["inputSchema"];
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    required.sort();

    assert_eq!(schema["type"], "object");
    assert!(
        schema["properties"]["expected_version"].is_object(),
        "{schema}"
    );
    assert_eq!(
        required,
        ["branch", "content", "org", "path", "repo", "session"]
    );
}

#[test]
fn write_read_edit_append_round_trip() {
    let mut server = Server::start();

    let written = server.ok(
        "write_file",
        in_session(json!({ "path": "round-1/spec.md", "content": "one two\n" })),
    );
    let read = server.ok(
        "read_file",
        in_session(json!({ "path": "round-1/spec.md" })),
    );

    assert_eq!(read["content"], "one two\n");
    assert_eq!(read["version"], written["version"]);
    assert_eq!(read["version"].as_str().unwrap().len(), 64);

    let edited = server.ok(
        "edit_file",
        in_session(json!({ "path": "round-1/spec.md", "old_str": "two", "new_str": "2" })),
    );
    let appended = server.ok(
        "append_file",
        in_session(json!({ "path": "round-1/spec.md", "content": "three\n" })),
    );
    let read = server.ok(
        "read_file",
        in_session(json!({ "path": "round-1/spec.md" })),
    );

    assert_ne!(edited["version"], written["version"]);
    assert_eq!(read["content"], "one 2\nthree\n");
    assert_eq!(read["version"], appended["version"]);

    let rewritten = server.ok(
        "write_file",
        in_session(json!({
            "path": "round-1/spec.md",
            "content": "four\n",
            "expected_version": appended["version"],
        })),
    );
    let moved = server.ok(
        "move_file",
        in_session(json!({ "from": "round-1/spec.md", "to": "spec.md" })),
    );
    let files = server.ok("list_files", in_session(json!({})));
    let read = server.ok("read_file", in_session(json!({ "path": "spec.md" })));

    assert_eq!(moved["version"], rewritten["version"]);
    assert_eq!(files, json!({ "files": ["spec.md"] }));
    assert_eq!(read["content"], "four\n");

    let deleted = server.ok(
        "delete_file",
        in_session(json!({ "path": "spec.md", "expected_version": moved["version"] })),
    );
    let files = server.ok("list_files", in_session(json!({})));

    assert_eq!(deleted, json!({}));
    assert_eq!(files, json!({ "files": [] }));
}

#[test]
fn errors_carry_their_code() {
    let mut server = Server::start();

    let first = server.ok(
        "write_file",
        in_session(json!({ "path": "plan.md", "content": "a b a\n" })),
    );

    let not_found = server.fail("read_file", in_session(json!({ "path": "spec.md" })));
    let exists = server.fail(
        "write_file",
        in_session(json!({ "path": "plan.md", "content": "new\n" })),
    );
    let no_match = server.fail(
        "edit_file",
        in_session(json!({ "path": "plan.md", "old_str": "c", "new_str": "d" })),
    );
    let many_matches = server.fail(
        "edit_file",
        in_session(json!({ "path": "plan.md", "old_str": "a", "new_str": "d" })),
    );
    let bad_name = server.fail(
        "read_file",
        in_session_of("..", "r", "s", json!({ "path": "plan.md" })),
    );
    let bad_path = server.fail("read_file", in_session(json!({ "path": "../plan.md" })));

    server.ok(
        "append_file",
        in_session(json!({ "path": "plan.md", "content": "b\n" })),
    );
    let current = server.ok("read_file", in_session(json!({ "path": "plan.md" })));
    let conflict = server.fail(
        "write_file",
        in_session(json!({
            "path": "plan.md",
            "content": "stale\n",
            "expected_version": first["version"],
        })),
    );

    let errors = [
        (&not_found, "not_found"),
        (&exists, "exists"),
        (&no_match, "no_match"),
        (&many_matches, "many_matches"),
        (&bad_name, "bad_name"),
        (&bad_path, "bad_name"),
        (&conflict, "conflict"),
    ];

    for (error, code) in errors {
        assert_eq!(error["code"], code, "{error}");
        assert!(!error["message"].as_str().unwrap().is_empty(), "{error}");

        if code == "conflict" {
            assert_eq!(error["current_version"], current["version"], "{error}");
        } else {
            assert_eq!(error["current_version"], Value::Null, "{error}");
        }
    }
}

#[test]
fn update_and_archive_change_session_json() {
    let mut server = Server::start();

    let session = json!({ "id": "s", "stage": "spec", "counts": { "a": 1 } });
    let content = format!("{}\n", serde_json::to_string_pretty(&session).unwrap());
    server.ok(
        "write_file",
        in_session(json!({ "path": "session.json", "content": content })),
    );

    let updated = server.ok(
        "update_session",
        in_session(json!({ "fields": { "stage": "plan", "counts": { "b": 2 } } })),
    );
    let read = server.ok("read_file", in_session(json!({ "path": "session.json" })));
    let data: Value = serde_json::from_str(read["content"].as_str().unwrap()).unwrap();

    assert_eq!(read["version"], updated["version"]);
    assert_eq!(data["stage"], "plan");
    assert_eq!(data["counts"], json!({ "a": 1, "b": 2 }));
    assert!(data["updated"].is_string(), "{data}");

    let archived = server.ok("archive_session", in_session(json!({})));
    let file = server
        .root()
        .join("o")
        .join("r")
        .join("sessions")
        .join("main")
        .join("s")
        .join("session.json");
    let data: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
    let read = server.ok("read_file", in_session(json!({ "path": "session.json" })));

    assert_eq!(read["version"], archived["version"]);
    assert_eq!(data["archived"], true);
    assert_eq!(data["stage"], "plan");

    let missing = server.fail(
        "update_session",
        in_session_of(
            "o",
            "r",
            "missing",
            json!({ "fields": { "stage": "plan" } }),
        ),
    );

    assert_eq!(missing["code"], "not_found");
}

#[test]
fn list_sessions_filters() {
    let mut server = Server::start();

    for (org, repo, branch, category) in [
        ("o1", "r1", "main", "walk"),
        ("o1", "r2", "main", "session"),
        ("o2", "r1", "fix", "review"),
    ] {
        server.ok(
            "start_session",
            json!({
                "org": org,
                "repo": repo,
                "branch": branch,
                "category": category,
                "head": "3f00486",
                "fields": { "flow": category },
            }),
        );
    }

    let archived = server.ok("list_sessions", json!({ "org": "o1", "repo": "r2" }));
    let id = archived["sessions"][0]["session"]
        .as_str()
        .unwrap()
        .to_string();
    server.ok(
        "archive_session",
        json!({ "org": "o1", "repo": "r2", "branch": "main", "session": id }),
    );

    let all = server.ok("list_sessions", json!({}));
    let with_archived = server.ok("list_sessions", json!({ "include_archived": true }));
    let org = server.ok(
        "list_sessions",
        json!({ "org": "o1", "include_archived": true }),
    );
    let repo = server.ok("list_sessions", json!({ "repo": "r1" }));
    let branch = server.ok("list_sessions", json!({ "branch": "fix" }));
    let category = server.ok("list_sessions", json!({ "category": "walk" }));
    let bad = server.fail("list_sessions", json!({ "org": ".." }));
    let bad_category = server.fail("list_sessions", json!({ "category": "build" }));

    let all_names = names(&all);

    assert_eq!(all_names.len(), 2, "{all}");
    assert!(all_names[0].starts_with("o1/r1/main/walk-"), "{all}");
    assert!(all_names[0].ends_with("-3f00486"), "{all}");
    assert!(all_names[1].starts_with("o2/r1/fix/review-"), "{all}");
    assert_eq!(all["sessions"][0]["category"], "walk");
    assert_eq!(all["sessions"][0]["data"]["flow"], "walk");
    assert_eq!(names(&with_archived).len(), 3);
    assert_eq!(names(&org).len(), 2);
    assert_eq!(names(&repo).len(), 2);
    assert!(
        names(&branch)[0].starts_with("o2/r1/fix/review-"),
        "{branch}"
    );
    assert_eq!(names(&category).len(), 1);
    assert!(
        names(&category)[0].starts_with("o1/r1/main/walk-"),
        "{category}"
    );
    assert_eq!(bad["code"], "bad_name");
    assert_eq!(bad_category["code"], "bad_name");
}

#[test]
fn start_session_creates_a_session_and_refuses_a_second_live_one() {
    let mut server = Server::start();

    let start = json!({
        "org": "o",
        "repo": "r",
        "branch": "fix",
        "category": "session",
        "head": "3f00486",
        "fields": { "flow": "build", "stage": "spec" },
    });

    let started = server.ok("start_session", start.clone());
    let id = started["session"].as_str().unwrap();

    assert_eq!(started["org"], "o");
    assert_eq!(started["repo"], "r");
    assert_eq!(started["branch"], "fix");
    assert!(
        id.starts_with("session-") && id.ends_with("-3f00486"),
        "{started}"
    );

    let s = json!({ "org": "o", "repo": "r", "branch": "fix", "session": id });
    let mut read_args = s.clone();
    read_args["path"] = json!("session.json");

    let read = server.ok("read_file", read_args);
    let data: Value = serde_json::from_str(read["content"].as_str().unwrap()).unwrap();

    assert_eq!(read["version"], started["version"]);
    assert_eq!(data["id"], id);
    assert_eq!(data["category"], "session");
    assert_eq!(data["stage"], "spec");

    let second = server.fail("start_session", start.clone());
    assert_eq!(second["code"], "exists");
    assert!(second["message"].as_str().unwrap().contains(id), "{second}");

    server.ok("archive_session", s);
    server.ok("start_session", start.clone());

    let mut bad = start;
    bad["category"] = json!("build");
    assert_eq!(server.fail("start_session", bad)["code"], "bad_name");
}

#[test]
fn startup_moves_old_sessions_into_their_branch() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let old = home
        .path()
        .join(".jam-factory/factory/o/r/sessions/rate-limit");
    fs::create_dir_all(&old).unwrap();
    fs::write(
        old.join("session.json"),
        r#"{"branch": "octonato/rate-limit", "flow": "build", "updated": "2026-10-15T14:02:11Z"}"#,
    )
    .unwrap();

    let mut server = Server::start_on(home);
    let listed = server.ok("list_sessions", json!({ "org": "o", "repo": "r" }));

    assert_eq!(
        names(&listed),
        ["o/r/rate-limit/session-20261015-1402-migrated"]
    );
    assert_eq!(listed["sessions"][0]["category"], "session");
}

#[test]
fn file_tools_refuse_a_missing_session() {
    let mut server = Server::start();

    let write = server.fail(
        "write_file",
        in_session_of(
            "o",
            "r",
            "walk-x",
            json!({ "path": "a.md", "content": "a\n" }),
        ),
    );
    let append = server.fail(
        "append_file",
        in_session_of(
            "o",
            "r",
            "walk-x",
            json!({ "path": "a.md", "content": "a\n" }),
        ),
    );

    assert_eq!(write["code"], "not_found");
    assert_eq!(append["code"], "not_found");
}

#[test]
fn processes_append_without_loss() {
    let home = Arc::new(tempfile::tempdir().unwrap());

    thread::scope(|scope| {
        for worker in 0..4 {
            let home = Arc::clone(&home);

            scope.spawn(move || {
                let mut server = Server::start_on(home);

                for call in 0..25 {
                    let line = format!("{worker}-{call}\n");
                    server.ok(
                        "append_file",
                        in_session(json!({ "path": "runs.jsonl", "content": line })),
                    );
                }
            });
        }
    });

    let mut server = Server::start_on(home);
    let read = server.ok("read_file", in_session(json!({ "path": "runs.jsonl" })));
    let content = read["content"].as_str().unwrap();
    let lines: HashSet<&str> = content.lines().collect();

    assert_eq!(content.lines().count(), 100);
    assert_eq!(lines.len(), 100);
}

#[test]
fn processes_update_sessions_without_loss() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let mut server = Server::start_on(Arc::clone(&home));
    server.ok(
        "write_file",
        in_session(json!({ "path": "session.json", "content": "{}\n" })),
    );

    thread::scope(|scope| {
        for worker in 0..4 {
            let home = Arc::clone(&home);

            scope.spawn(move || {
                let mut server = Server::start_on(home);

                for call in 0..25 {
                    let key = format!("{worker}-{call}");
                    server.ok(
                        "update_session",
                        in_session(json!({ "fields": { key: call } })),
                    );
                }
            });
        }

        let home = Arc::clone(&home);

        scope.spawn(move || {
            let mut server = Server::start_on(home);

            for _ in 0..25 {
                server.ok("archive_session", in_session(json!({})));
            }
        });
    });

    let read = server.ok("read_file", in_session(json!({ "path": "session.json" })));
    let data: Value = serde_json::from_str(read["content"].as_str().unwrap()).unwrap();

    for worker in 0..4 {
        for call in 0..25 {
            let key = format!("{worker}-{call}");
            assert_eq!(data[&key], call, "{key}");
        }
    }

    assert_eq!(data["archived"], true);
}

#[test]
fn processes_edit_without_loss() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let mut server = Server::start_on(Arc::clone(&home));

    let mut todo = String::new();
    let mut done = String::new();

    for worker in 0..4 {
        for call in 0..25 {
            todo.push_str(&task_line(worker, call, "todo"));
            done.push_str(&task_line(worker, call, "done"));
        }
    }

    server.ok(
        "write_file",
        in_session(json!({ "path": "tasks.md", "content": todo })),
    );

    thread::scope(|scope| {
        for worker in 0..4 {
            let home = Arc::clone(&home);

            scope.spawn(move || {
                let mut server = Server::start_on(home);

                for call in 0..25 {
                    let old = task_line(worker, call, "todo");
                    let new = task_line(worker, call, "done");

                    server.ok(
                        "edit_file",
                        in_session(json!({ "path": "tasks.md", "old_str": old, "new_str": new })),
                    );
                }
            });
        }
    });

    let read = server.ok("read_file", in_session(json!({ "path": "tasks.md" })));

    assert_eq!(read["content"], done);
}

#[test]
fn append_diff_writes_git_output_verbatim() {
    let mut server = Server::start();
    let repo = git_session(&mut server);

    let diff = git(
        server.home.path(),
        repo.path(),
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-M",
            "HEAD~1",
            "HEAD",
            "--",
            "README.md",
            "src/lib.rs",
        ],
    );

    server.ok(
        "append_file",
        in_session(json!({ "path": "drafts/01.md", "content": "## Step 1: A\n" })),
    );
    let appended = server.ok(
        "append_diff",
        in_session(json!({
            "path": "drafts/01.md",
            "base": "HEAD~1",
            "head": "HEAD",
            "files": ["README.md", "src/lib.rs"],
        })),
    );
    let read = server.ok("read_file", in_session(json!({ "path": "drafts/01.md" })));

    assert!(diff.contains("+````\n"), "{diff}");
    assert_eq!(
        read["content"],
        format!("## Step 1: A\n\n`````diff\n{diff}`````\n")
    );
    assert_eq!(read["version"], appended["version"]);
}

#[test]
fn append_diff_keeps_only_hunks_in_lines() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    server.ok(
        "append_diff",
        in_session(json!({
            "path": "drafts/01.md",
            "base": "HEAD~1",
            "head": "HEAD",
            "files": ["README.md", "src/lib.rs"],
            "lines": [[40, 42]],
        })),
    );
    let read = server.ok("read_file", in_session(json!({ "path": "drafts/01.md" })));
    let content = read["content"].as_str().unwrap();

    assert!(
        content.starts_with("\n```diff\ndiff --git a/src/lib.rs b/src/lib.rs\n"),
        "{content}"
    );
    assert_eq!(content.matches("\n@@ ").count(), 1, "{content}");
    assert!(content.contains("+// new line 41\n"), "{content}");
    assert!(!content.contains("new line 2\n"), "{content}");
    assert!(!content.contains("README.md"), "{content}");
}

#[test]
fn append_diff_follows_a_file_renamed_by_its_new_path() {
    let mut server = Server::start();
    let repo = git_session(&mut server);
    let home = Arc::clone(&server.home);

    git(
        home.path(),
        repo.path(),
        &["mv", "src/lib.rs", "src/core.rs"],
    );
    fs::write(
        repo.path().join("src").join("core.rs"),
        lib_rs(&[2, 41, 45]),
    )
    .unwrap();
    fs::write(repo.path().join("README.md"), "# Renamed\n").unwrap();
    git(home.path(), repo.path(), &["add", "."]);
    git(home.path(), repo.path(), &["commit", "-q", "-m", "rename"]);

    let diff_args = |lines: Value| {
        in_session(json!({
            "path": "drafts/01.md",
            "base": "HEAD~1",
            "head": "HEAD",
            "files": ["src/core.rs"],
            "lines": lines,
        }))
    };

    server.ok("append_diff", diff_args(Value::Null));
    let outside = server.fail("append_diff", diff_args(json!([[1, 5]])));
    let read = server.ok("read_file", in_session(json!({ "path": "drafts/01.md" })));
    let content = read["content"].as_str().unwrap();

    assert!(
        content.starts_with("\n```diff\ndiff --git a/src/lib.rs b/src/core.rs\n"),
        "{content}"
    );
    assert!(
        content.contains("rename from src/lib.rs\nrename to src/core.rs\n"),
        "{content}"
    );
    assert_eq!(content.matches("\n@@ ").count(), 1, "{content}");
    assert!(content.contains("+// new line 45\n"), "{content}");
    assert!(!content.contains("new file mode"), "{content}");
    assert!(!content.contains("README.md"), "{content}");
    assert_eq!(outside["code"], "empty_diff", "{outside}");
}

#[test]
fn append_diff_with_no_match_writes_nothing() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    let no_hunk = server.fail(
        "append_diff",
        in_session(json!({
            "path": "drafts/01.md",
            "base": "HEAD~1",
            "head": "HEAD",
            "files": ["src/lib.rs"],
            "lines": [[100, 120]],
        })),
    );
    let no_change = server.fail(
        "append_diff",
        in_session(json!({
            "path": "drafts/01.md",
            "base": "HEAD",
            "head": "HEAD",
            "files": ["src/lib.rs"],
        })),
    );

    for error in [&no_hunk, &no_change] {
        assert_eq!(error["code"], "empty_diff", "{error}");
        assert_eq!(error["current_version"], Value::Null, "{error}");
    }

    assert_eq!(
        session_files(&mut server),
        json!({ "files": ["session.json"] })
    );
}

#[test]
fn append_code_writes_a_tagged_block() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    let appended = server.ok(
        "append_code",
        in_session(json!({ "path": "drafts/01.md", "file": "src/lib.rs", "start": 2, "end": 3 })),
    );
    let read = server.ok("read_file", in_session(json!({ "path": "drafts/01.md" })));

    assert_eq!(
        read["content"],
        "\n```rust path=src/lib.rs lines=2-3\n// new line 2\n// line 3\n```\n"
    );
    assert_eq!(read["version"], appended["version"]);
}

#[test]
fn append_code_reads_at_a_ref() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    server.ok(
        "append_code",
        in_session(json!({
            "path": "drafts/01.md",
            "file": "src/lib.rs",
            "start": 2,
            "end": 3,
            "ref": "HEAD~1",
        })),
    );
    let read = server.ok("read_file", in_session(json!({ "path": "drafts/01.md" })));

    assert_eq!(
        read["content"],
        "\n```rust path=src/lib.rs lines=2-3\n// line 2\n// line 3\n```\n"
    );
}

#[test]
fn tools_refuse_escaping_paths() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    for file in ["../x", "/etc/hosts"] {
        let diff = server.fail(
            "append_diff",
            in_session(json!({
                "path": "drafts/01.md",
                "base": "HEAD~1",
                "head": "HEAD",
                "files": ["src/lib.rs", file],
            })),
        );
        let code = server.fail(
            "append_code",
            in_session(json!({ "path": "drafts/01.md", "file": file, "start": 1, "end": 1 })),
        );

        assert_eq!(diff["code"], "bad_path", "{file}");
        assert_eq!(code["code"], "bad_path", "{file}");
    }

    let no_files = server.fail(
        "append_diff",
        in_session(json!({
            "path": "drafts/01.md",
            "base": "HEAD~1",
            "head": "HEAD",
            "files": [],
        })),
    );

    assert_eq!(no_files["code"], "bad_path");
    assert_eq!(
        session_files(&mut server),
        json!({ "files": ["session.json"] })
    );
}

#[test]
fn tools_need_a_worktree_in_session_json() {
    let mut server = Server::start();

    let sessions = [
        ("missing", "{\"id\": \"missing\"}\n"),
        ("number", "{\"worktree\": 3}\n"),
        ("broken", "not json\n"),
    ];

    for (session, content) in sessions {
        server.create("o", "r", session);
        server.ok(
            "write_file",
            in_session_of(
                "o",
                "r",
                session,
                json!({ "path": "session.json", "content": content }),
            ),
        );

        let diff = server.fail(
            "append_diff",
            in_session_of(
                "o",
                "r",
                session,
                json!({
                    "path": "drafts/01.md",
                    "base": "HEAD~1",
                    "head": "HEAD",
                    "files": ["src/lib.rs"],
                }),
            ),
        );
        let code = server.fail(
            "append_code",
            in_session_of(
                "o",
                "r",
                session,
                json!({ "path": "drafts/01.md", "file": "src/lib.rs", "start": 1, "end": 1 }),
            ),
        );
        let files = server.ok("list_files", in_session_of("o", "r", session, json!({})));

        assert_eq!(diff["code"], "bad_json", "{session}");
        assert_eq!(code["code"], "bad_json", "{session}");
        assert_eq!(files, json!({ "files": ["session.json"] }), "{session}");
    }
}

#[test]
fn append_code_refuses_a_bad_range() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    for (start, end) in [(0, 1), (3, 2), (1, 51)] {
        let error = server.fail(
            "append_code",
            in_session(json!({
                "path": "drafts/01.md",
                "file": "src/lib.rs",
                "start": start,
                "end": end,
            })),
        );

        assert_eq!(error["code"], "bad_range", "{start}-{end}");
    }

    assert_eq!(
        session_files(&mut server),
        json!({ "files": ["session.json"] })
    );
}

#[test]
fn tools_report_git_failures() {
    let mut server = Server::start();
    let _repo = git_session(&mut server);

    let bad_base = server.fail(
        "append_diff",
        in_session(json!({
            "path": "drafts/01.md",
            "base": "nope",
            "head": "HEAD",
            "files": ["src/lib.rs"],
        })),
    );
    let missing_file = server.fail(
        "append_code",
        in_session(json!({ "path": "drafts/01.md", "file": "src/gone.rs", "start": 1, "end": 1 })),
    );
    let bad_ref = server.fail(
        "append_code",
        in_session(json!({
            "path": "drafts/01.md",
            "file": "src/lib.rs",
            "start": 1,
            "end": 1,
            "ref": "nope",
        })),
    );

    for error in [&bad_base, &missing_file, &bad_ref] {
        assert_eq!(error["code"], "git_failed", "{error}");
    }

    assert_eq!(
        session_files(&mut server),
        json!({ "files": ["session.json"] })
    );
}

#[test]
fn bad_config_stops_the_server() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".jam-factory").join("config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "not json").unwrap();

    let output = Command::new(BINARY)
        .env("HOME", home.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains(&config.display().to_string()), "{stderr}");
}
