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
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_jam-factory-mcp");
const TIMEOUT: Duration = Duration::from_secs(10);

const TOOLS: [&str; 10] = [
    "append_file",
    "archive_session",
    "delete_file",
    "edit_file",
    "list_files",
    "list_sessions",
    "move_file",
    "read_file",
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
        server
    }

    fn root(&self) -> PathBuf {
        self.home.path().join(".jam-factory").join("factory")
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
    let mut arguments = json!({ "org": org, "repo": repo, "session": session });
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
                "{}/{}/{}",
                entry["org"].as_str().unwrap(),
                entry["repo"].as_str().unwrap(),
                entry["session"].as_str().unwrap()
            )
        })
        .collect()
}

/// The line of `worker` for its call `call`, marked `state`.
fn task_line(worker: usize, call: usize, state: &str) -> String {
    format!("task {worker}-{call}: {state}\n")
}

#[test]
fn initialize_names_the_server() {
    let server = Server::start();

    assert_eq!(server.info["serverInfo"]["name"], "jam-factory-mcp");
}

#[test]
fn tools_list_has_the_ten_tools() {
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
    assert_eq!(required, ["content", "org", "path", "repo", "session"]);
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

    for (org, repo, session) in [("o1", "r1", "s1"), ("o1", "r2", "s2"), ("o2", "r1", "s3")] {
        let content = format!("{{\"id\": \"{session}\"}}\n");
        server.ok(
            "write_file",
            in_session_of(
                org,
                repo,
                session,
                json!({ "path": "session.json", "content": content }),
            ),
        );
    }

    server.ok(
        "archive_session",
        in_session_of("o1", "r2", "s2", json!({})),
    );

    let all = server.ok("list_sessions", json!({}));
    let archived = server.ok("list_sessions", json!({ "include_archived": true }));
    let org = server.ok(
        "list_sessions",
        json!({ "org": "o1", "include_archived": true }),
    );
    let repo = server.ok("list_sessions", json!({ "repo": "r1" }));
    let both = server.ok("list_sessions", json!({ "org": "o2", "repo": "r1" }));
    let bad = server.fail("list_sessions", json!({ "org": ".." }));

    assert_eq!(names(&all), ["o1/r1/s1", "o2/r1/s3"]);
    assert_eq!(all["sessions"][0]["data"], json!({ "id": "s1" }));
    assert_eq!(names(&archived), ["o1/r1/s1", "o1/r2/s2", "o2/r1/s3"]);
    assert_eq!(names(&org), ["o1/r1/s1", "o1/r2/s2"]);
    assert_eq!(names(&repo), ["o1/r1/s1", "o2/r1/s3"]);
    assert_eq!(names(&both), ["o2/r1/s3"]);
    assert_eq!(bad["code"], "bad_name");
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
