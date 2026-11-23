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

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jam_board_core::server::Server;
use jam_board_core::server::dashboard::embedded;
use jam_board_core::server::repos::FixedRepos;
use jam_board_core::store::Store;
use jam_board_core::version::BUILD_VERSION;
use jam_factory_store::name::check_path;
use jam_factory_store::sessions::SESSION_FILE;
use jam_factory_store::{Category, FileRead, SessionEntry, SessionRef, StoreError};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Map, Value, json};
use tungstenite::{ClientRequestBuilder, Message};

/// The content of a fake file and the number of times it was written.
#[derive(Default)]
struct FakeFile {
    content: String,
    writes: u32,
}

impl FakeFile {
    /// Replaces the content and counts the write.
    fn set(&mut self, content: &str) {
        self.content = content.to_string();
        self.writes += 1;
    }

    /// `v<n>`, where `n` is the number of writes.
    fn version(&self) -> String {
        format!("v{}", self.writes)
    }
}

/// A store over an in-memory map from each session to its files, which fails every call once `failure` is set.
#[derive(Default)]
struct FakeStore {
    sessions: Mutex<HashMap<SessionRef, BTreeMap<String, FakeFile>>>,
    failure: Mutex<Option<StoreError>>,
}

impl FakeStore {
    fn write(&self, s: &SessionRef, path: &str, content: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        let files = sessions.entry(s.clone()).or_default();
        files.entry(path.to_string()).or_default().set(content);
    }

    fn read(&self, s: &SessionRef, path: &str) -> Option<String> {
        let sessions = self.sessions.lock().unwrap();
        let file = sessions.get(s)?.get(path)?;
        Some(file.content.clone())
    }

    fn fail_with(&self, error: StoreError) {
        *self.failure.lock().unwrap() = Some(error);
    }

    fn check(&self) -> Result<(), StoreError> {
        match self.failure.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Store for FakeStore {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>, StoreError> {
        self.check()?;

        let sessions = self.sessions.lock().unwrap();

        let mut entries: Vec<SessionEntry> = sessions
            .iter()
            .filter_map(|(s, files)| entry(s, &files.get(SESSION_FILE)?.content))
            .collect();
        entries.sort_by(|a, b| (&a.org, &a.repo, &a.session).cmp(&(&b.org, &b.repo, &b.session)));

        Ok(entries)
    }

    fn list_files(&self, s: &SessionRef) -> Result<Vec<String>, StoreError> {
        self.check()?;

        let sessions = self.sessions.lock().unwrap();
        let files = sessions.get(s).ok_or_else(|| not_found(s, ""))?;

        Ok(files.keys().cloned().collect())
    }

    fn read_file(&self, s: &SessionRef, path: &str) -> Result<FileRead, StoreError> {
        self.check()?;
        check_path(path)?;

        let sessions = self.sessions.lock().unwrap();
        let file = sessions
            .get(s)
            .and_then(|files| files.get(path))
            .ok_or_else(|| not_found(s, path))?;

        Ok(FileRead {
            content: file.content.clone(),
            version: file.version(),
        })
    }

    fn write_file(
        &self,
        s: &SessionRef,
        path: &str,
        content: &str,
        expected_version: &str,
    ) -> Result<String, StoreError> {
        self.check()?;
        check_path(path)?;

        let mut sessions = self.sessions.lock().unwrap();
        let file = sessions
            .get_mut(s)
            .and_then(|files| files.get_mut(path))
            .ok_or_else(|| not_found(s, path))?;

        let current = file.version();
        if expected_version != current {
            return Err(StoreError::Conflict { current });
        }

        file.set(content);

        Ok(file.version())
    }

    fn archive_session(&self, s: &SessionRef) -> Result<(), StoreError> {
        self.check()?;

        let text = self
            .read(s, SESSION_FILE)
            .ok_or_else(|| not_found(s, SESSION_FILE))?;
        let mut session: Map<String, Value> =
            serde_json::from_str(&text).map_err(|e| StoreError::BadJson(e.to_string()))?;

        session.insert("archived".to_string(), Value::Bool(true));
        self.write(s, SESSION_FILE, &Value::Object(session).to_string());

        Ok(())
    }
}

/// The list entry of the session `s` with the `session.json` text `text`, or `None` when it is archived.
fn entry(s: &SessionRef, text: &str) -> Option<SessionEntry> {
    let (data, error) = match serde_json::from_str::<Value>(text) {
        Ok(data) if data["archived"] == true => return None,
        Ok(data) => (Some(data), None),
        Err(e) => (None, Some(e.to_string())),
    };

    Some(SessionEntry {
        org: s.org().to_string(),
        repo: s.repo().to_string(),
        branch: s.branch().to_string(),
        session: s.session().to_string(),
        category: Category::of(s.session()).unwrap_or(Category::Session),
        data,
        error,
    })
}

fn not_found(s: &SessionRef, path: &str) -> StoreError {
    StoreError::NotFound(format!("{s}/{path} does not exist"))
}

fn session(org: &str, repo: &str, id: &str) -> SessionRef {
    SessionRef::new(org, repo, "main", id).unwrap()
}

struct Fixture {
    server: Server,
    store: Arc<FakeStore>,
}

impl Fixture {
    fn port(&self) -> u16 {
        self.server.port()
    }

    /// The `session.json` text of the session `o/stack/a`.
    fn session_json(&self) -> String {
        self.store.read(&session_a(), SESSION_FILE).unwrap()
    }

    /// The `spec.md` text of the session `o/stack/a`.
    fn spec(&self) -> String {
        self.store.read(&session_a(), "spec.md").unwrap()
    }
}

/// A server on a free port over an empty fake store.
fn fixture() -> Fixture {
    let store = Arc::new(FakeStore::default());
    let server = Server::start(store.clone(), Arc::new(FixedRepos(vec![])), 0).unwrap();

    Fixture { server, store }
}

const SESSION: &str = r#"{"id":"a","stage":"done","archived":false}"#;

fn session_a() -> SessionRef {
    session("o", "stack", "a")
}

/// A fixture whose store holds the session `o/stack/a`.
fn session_fixture() -> Fixture {
    let f = fixture();
    f.store.write(&session_a(), SESSION_FILE, SESSION);
    f
}

const SPEC: &str = "/api/file/o/stack/main/a/spec.md";

/// A fixture whose session `o/stack/a` also holds `spec.md` with the text `old`, at version `v1`.
fn spec_fixture() -> Fixture {
    let f = session_fixture();
    f.store.write(&session_a(), "spec.md", "old");
    f
}

struct Reply {
    status: u16,
    headers: String,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

/// Sends a request with `Host: 127.0.0.1:<port>` and `headers`.
fn request(port: u16, method: &str, target: &str, headers: &[(&str, &str)]) -> Reply {
    request_body(port, method, target, headers, "")
}

/// Sends a request with `Host: 127.0.0.1:<port>`, `headers` and `body`.
fn request_body(
    port: u16,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let host = format!("127.0.0.1:{port}");
    let mut all = vec![("Host", host.as_str())];
    all.extend_from_slice(headers);

    send_body(port, method, target, &all, body)
}

/// Sends a request with exactly `headers`.
fn send(port: u16, method: &str, target: &str, headers: &[(&str, &str)]) -> Reply {
    send_body(port, method, target, headers, "")
}

/// Sends a request with exactly `headers`, plus `Content-Length` and `body` when `body` is not empty.
fn send_body(port: u16, method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    let mut head = format!("{method} {target} HTTP/1.1\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");

    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();

    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();

    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let body = bytes[end + 4..].to_vec();
    let status = headers.split(' ').nth(1).unwrap().parse().unwrap();

    Reply {
        status,
        headers,
        body,
    }
}

fn get(port: u16, target: &str) -> Reply {
    request(port, "GET", target, &[])
}

fn post(port: u16, target: &str) -> Reply {
    request(port, "POST", target, &[])
}

fn put(port: u16, target: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    request_body(port, "PUT", target, headers, body)
}

const LAZYGIT: &str = "/api/lazygit/o/stack/main/a";
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

/// The WebSocket upgrade headers with `version`, plus `Origin` and `Sec-WebSocket-Key` when given.
fn upgrade<'a>(
    origin: Option<&'a str>,
    version: &'a str,
    key: Option<&'a str>,
) -> Vec<(&'a str, &'a str)> {
    let mut headers = vec![
        ("Upgrade", "websocket"),
        ("Connection", "Upgrade"),
        ("Sec-WebSocket-Version", version),
    ];
    headers.extend(origin.map(|origin| ("Origin", origin)));
    headers.extend(key.map(|key| ("Sec-WebSocket-Key", key)));
    headers
}

/// Runs `git args` in `dir`, without the user and system git config.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success(), "git {args:?}");
}

/// The canonical main checkout `o/r/main` under `dir` and its linked worktree `o/r/feature x`, or `None` without git.
fn repo_with_worktree(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    if Command::new("git").arg("--version").output().is_err() {
        return None;
    }

    let dir = fs::canonicalize(dir).unwrap();
    let main = dir.join("o/r/main");
    let linked = dir.join("o/r/feature x");

    git(&dir, &["init", "-q", "-b", "main", "o/r/main"]);
    git(
        &main,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
    git(
        &main,
        &["worktree", "add", "-q", "-b", "feature", "../feature x"],
    );

    Some((main, linked))
}

/// A server on a free port over an empty fake store, with `main` as its one configured repo.
fn repo_fixture(main: PathBuf) -> Fixture {
    let store = Arc::new(FakeStore::default());
    let server = Server::start(store.clone(), Arc::new(FixedRepos(vec![main])), 0).unwrap();

    Fixture { server, store }
}

/// The `/api/terminal` target that runs `program` in the folder `path`.
fn terminal_target(path: &Path, program: &str) -> String {
    let path = utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string();
    format!("/api/terminal?path={path}&program={program}")
}

#[test]
fn sessions_route_returns_the_entries() {
    let f = session_fixture();
    f.store
        .write(&session("p", "web", "b"), SESSION_FILE, "not json");
    f.store.write(
        &session("o", "stack", "old"),
        SESSION_FILE,
        r#"{"archived":true}"#,
    );

    let reply = get(f.port(), "/api/sessions");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Content-Type"), Some("application/json"));

    let error = serde_json::from_str::<Value>("not json")
        .unwrap_err()
        .to_string();
    assert_eq!(
        reply.json(),
        json!({ "sessions": [
            {
                "org": "o", "repo": "stack", "branch": "main", "session": "a", "category": "session",
                "data": serde_json::from_str::<Value>(SESSION).unwrap(),
            },
            {
                "org": "p", "repo": "web", "branch": "main", "session": "b", "category": "session",
                "error": error,
            },
        ] })
    );
}

#[test]
fn version_route_returns_the_build_version() {
    let f = fixture();

    let reply = get(f.port(), "/api/version");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Content-Type"), Some("application/json"));
    assert_eq!(reply.json(), json!({ "version": BUILD_VERSION }));
}

#[test]
fn get_repos_lists_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let Some((main, linked)) = repo_with_worktree(dir.path()) else {
        eprintln!("skipped: git is missing");
        return;
    };

    let store = Arc::new(FakeStore::default());
    let repos = Arc::new(FixedRepos(vec![main.clone()]));
    let server = Server::start(store, repos, 0).unwrap();

    let reply = get(server.port(), "/api/repos");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Content-Type"), Some("application/json"));
    assert_eq!(
        reply.json(),
        json!({
            "error": null,
            "repos": [{
                "org": "o",
                "repo": "r",
                "github": "o/r",
                "path": main,
                "branch": "main",
                "error": null,
                "worktrees": [
                    { "path": linked, "branch": "feature", "head": null },
                ],
            }],
        })
    );
}

#[test]
fn file_route_serves_content_with_its_type() {
    let f = session_fixture();
    f.store.write(&session_a(), "spec.md", "# Spec");
    f.store.write(&session_a(), "runs.jsonl", "{}\n{}\n");

    for (path, content, content_type) in [
        ("spec.md", "# Spec", "text/markdown; charset=utf-8"),
        ("session.json", SESSION, "application/json"),
        ("runs.jsonl", "{}\n{}\n", "application/jsonl"),
    ] {
        let reply = get(f.port(), &format!("/api/file/o/stack/main/a/{path}"));

        assert_eq!(reply.status, 200, "{path}");
        assert_eq!(reply.text(), content, "{path}");
        assert_eq!(reply.header("Content-Type"), Some(content_type), "{path}");
    }
}

#[test]
fn file_route_reads_nested_paths() {
    let f = session_fixture();
    f.store
        .write(&session_a(), "round-1/reviews/review.md", "fine");

    let reply = get(
        f.port(),
        "/api/file/o/stack/main/a/round-1/reviews/review.md",
    );

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "fine");
}

#[test]
fn file_route_returns_the_version_as_etag() {
    let f = spec_fixture();

    let reply = get(f.port(), SPEC);

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("ETag"), Some("\"v1\""));
}

#[test]
fn files_route_lists_paths() {
    let f = session_fixture();
    f.store.write(&session_a(), "round-1/plan.md", "");
    f.store.write(&session_a(), "spec.md", "");

    let reply = get(f.port(), "/api/files/o/stack/main/a");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("Content-Type"), Some("application/json"));
    assert_eq!(
        reply.json(),
        json!({ "files": ["round-1/plan.md", "session.json", "spec.md"] })
    );
}

#[test]
fn odd_names_round_trip() {
    let f = fixture();
    let odd = session("caf\u{e9}", "a b%c", "s 1");
    f.store.write(&odd, SESSION_FILE, r#"{"id":"s 1"}"#);
    f.store.write(&odd, "notes/caf\u{e9} 1.md", "hi");

    let sessions = get(f.port(), "/api/sessions").json();
    assert_eq!(sessions["sessions"][0]["org"], "caf\u{e9}");
    assert_eq!(sessions["sessions"][0]["repo"], "a b%c");
    assert_eq!(sessions["sessions"][0]["session"], "s 1");

    let files = get(f.port(), "/api/files/caf%C3%A9/a%20b%25c/main/s%201").json();
    assert_eq!(
        files,
        json!({ "files": ["notes/caf\u{e9} 1.md", "session.json"] })
    );

    let file = get(
        f.port(),
        "/api/file/caf%C3%A9/a%20b%25c/main/s%201/notes/caf%C3%A9%201.md",
    );
    assert_eq!(file.status, 200);
    assert_eq!(file.text(), "hi");

    let archived = post(f.port(), "/api/archive/caf%C3%A9/a%20b%25c/main/s%201");
    assert_eq!(archived.status, 204);
    assert_eq!(
        f.store.read(&odd, SESSION_FILE).unwrap(),
        r#"{"id":"s 1","archived":true}"#
    );
}

#[test]
fn archive_route_archives() {
    let f = session_fixture();

    let reply = post(f.port(), "/api/archive/o/stack/main/a");

    assert_eq!(reply.status, 204);
    assert!(reply.body.is_empty());
    assert_eq!(
        f.session_json(),
        r#"{"id":"a","stage":"done","archived":true}"#
    );

    let sessions = get(f.port(), "/api/sessions").json();
    assert_eq!(sessions, json!({ "sessions": [] }));
}

#[test]
fn archive_accepts_same_origin() {
    let f = session_fixture();
    let origin = format!("http://127.0.0.1:{}", f.port());

    let reply = request(
        f.port(),
        "POST",
        "/api/archive/o/stack/main/a",
        &[("Origin", &origin)],
    );

    assert_eq!(reply.status, 204);
}

#[test]
fn archive_rejects_bad_names() {
    let f = session_fixture();

    for target in [
        "/api/archive/o/%2e%2e/main/a",
        "/api/archive/o/stack/main/..",
        "/api/archive/o/stack/main/.",
        "/api/archive/o/stack/main/a%2F..%2Fa",
        "/api/archive/o/stack/main/a%5Cb",
        "/api/archive/o//main/a",
        "/api/archive/o/%FF/main/a",
    ] {
        let reply = post(f.port(), target);

        assert_eq!(reply.status, 404, "{target}");
        assert_eq!(f.session_json(), SESSION, "{target}");
    }
}

#[test]
fn archive_rejects_extra_segments() {
    let f = session_fixture();

    let reply = post(f.port(), "/api/archive/o/stack/main/a/extra");

    assert_eq!(reply.status, 404);
    assert_eq!(f.session_json(), SESSION);
}

#[test]
fn unknown_session_is_404() {
    let f = session_fixture();

    for target in [
        "/api/files/o/stack/main/nope",
        "/api/file/o/stack/main/nope/spec.md",
        "/api/file/o/stack/main/a/nope.md",
        "/api/file/o/stack/main/a/",
        "/api/file/o/stack/main/a/.lock",
        "/api/file/o/stack/main/a",
        "/api/files/o/stack",
        "/api/nope",
    ] {
        let reply = get(f.port(), target);

        assert_eq!(reply.status, 404, "{target}");
    }

    for target in [
        "/api/archive/o/stack/main/nope",
        "/api/open/zed/o/stack/main/nope",
    ] {
        let reply = post(f.port(), target);

        assert_eq!(reply.status, 404, "{target}");
    }
}

#[test]
fn store_failure_is_500_with_the_message() {
    let f = session_fixture();
    f.store
        .fail_with(StoreError::Io("the disk is full".to_string()));

    for (method, target) in [
        ("GET", "/api/sessions"),
        ("GET", "/api/files/o/stack/main/a"),
        ("GET", "/api/file/o/stack/main/a/session.json"),
        ("POST", "/api/archive/o/stack/main/a"),
        ("POST", "/api/open/zed/o/stack/main/a"),
    ] {
        let reply = request(f.port(), method, target, &[]);

        assert_eq!(reply.status, 500, "{target}");
        assert_eq!(reply.text(), "the disk is full", "{target}");
    }
}

#[test]
fn archive_bad_json_is_500() {
    let f = session_fixture();
    f.store.write(&session_a(), SESSION_FILE, "not json");

    let reply = post(f.port(), "/api/archive/o/stack/main/a");

    assert_eq!(reply.status, 500);
    assert!(!reply.body.is_empty());
    assert_eq!(f.session_json(), "not json");
}

#[test]
fn foreign_host_is_403() {
    let f = session_fixture();

    for host in [
        format!("evil.example:{}", f.port()),
        format!("127.0.0.1:{}", f.port().wrapping_add(1)),
        "localhost".to_string(),
    ] {
        let headers = [("Host", host.as_str())];

        let sessions = send(f.port(), "GET", "/api/sessions", &headers);
        let dashboard = send(f.port(), "GET", "/dashboard.html", &headers);
        let archive = send(f.port(), "POST", "/api/archive/o/stack/main/a", &headers);

        assert_eq!(sessions.status, 403, "{host}");
        assert_eq!(dashboard.status, 403, "{host}");
        assert_eq!(archive.status, 403, "{host}");
        assert_eq!(f.session_json(), SESSION, "{host}");
    }
}

#[test]
fn missing_host_is_403() {
    let f = session_fixture();

    let get = send(f.port(), "GET", "/api/sessions", &[]);
    let post = send(f.port(), "POST", "/api/archive/o/stack/main/a", &[]);

    assert_eq!(get.status, 403);
    assert_eq!(post.status, 403);
    assert_eq!(f.session_json(), SESSION);
}

#[test]
fn foreign_origin_post_is_403() {
    let f = session_fixture();

    for target in [
        "/api/archive/o/stack/main/a",
        "/api/open/zed/o/stack/main/a",
    ] {
        let reply = request(
            f.port(),
            "POST",
            target,
            &[("Origin", "http://evil.example")],
        );

        assert_eq!(reply.status, 403, "{target}");
        assert_eq!(f.session_json(), SESSION, "{target}");
    }
}

#[test]
fn local_hosts_are_served() {
    for name in ["localhost", "127.0.0.1"] {
        let f = session_fixture();
        let host = format!("{name}:{}", f.port());
        let origin = format!("http://{host}");

        let get = send(f.port(), "GET", "/api/sessions", &[("Host", &host)]);
        let post = send(
            f.port(),
            "POST",
            "/api/archive/o/stack/main/a",
            &[("Host", &host), ("Origin", &origin)],
        );

        assert_eq!(get.status, 200, "{host}");
        assert_eq!(post.status, 204, "{host}");
    }
}

#[test]
fn dashboard_is_served() {
    let f = fixture();

    for (name, content_type) in [
        ("dashboard.html", "text/html; charset=utf-8"),
        ("marked.min.js", "text/javascript; charset=utf-8"),
        ("purify.min.js", "text/javascript; charset=utf-8"),
    ] {
        let path = format!("/{name}");

        let reply = get(f.port(), &path);

        assert_eq!(reply.status, 200, "{name}");
        assert_eq!(reply.body, embedded(&path).unwrap().bytes, "{name}");
        assert_eq!(reply.header("Content-Type"), Some(content_type), "{name}");
    }

    for target in ["/", "/nope.html", "/o/stack/sessions/a/session.json"] {
        assert_eq!(get(f.port(), target).status, 404, "{target}");
    }
}

#[test]
fn query_string_is_ignored() {
    let f = session_fixture();

    for target in [
        "/api/sessions?x=1",
        "/api/file/o/stack/main/a/session.json?x=1",
        "/dashboard.html?x=1",
    ] {
        let reply = get(f.port(), target);

        assert_eq!(reply.status, 200, "{target}");
    }
}

#[test]
fn wrong_method_is_405() {
    let f = fixture();

    let reply = request(f.port(), "DELETE", "/api/sessions", &[]);

    assert_eq!(reply.status, 405);
}

#[test]
fn post_elsewhere_is_404() {
    let f = session_fixture();

    for target in ["/api/sessions", "/dashboard.html", "/archive/stack/a"] {
        let reply = post(f.port(), target);

        assert_eq!(reply.status, 404, "{target}");
    }

    assert_eq!(f.session_json(), SESSION);
}

#[test]
fn put_writes_and_returns_the_new_version() {
    let f = spec_fixture();

    let reply = put(f.port(), SPEC, &[("If-Match", "\"v1\"")], "new");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("ETag"), Some("\"v2\""));
    assert_eq!(f.spec(), "new");

    let read = get(f.port(), SPEC);
    assert_eq!(read.header("ETag"), Some("\"v2\""));
    assert_eq!(read.text(), "new");
}

#[test]
fn put_accepts_a_bare_if_match() {
    let f = spec_fixture();

    let reply = put(f.port(), SPEC, &[("If-Match", "v1")], "new");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("ETag"), Some("\"v2\""));
    assert_eq!(f.spec(), "new");
}

#[test]
fn put_conflict_is_409_with_the_current_version() {
    let f = spec_fixture();

    let reply = put(f.port(), SPEC, &[("If-Match", "\"v0\"")], "new");

    assert_eq!(reply.status, 409);
    assert_eq!(reply.header("ETag"), Some("\"v1\""));
    assert_eq!(f.spec(), "old");
}

#[test]
fn put_without_if_match_is_428() {
    let f = spec_fixture();

    let reply = put(f.port(), SPEC, &[], "new");

    assert_eq!(reply.status, 428);
    assert_eq!(f.spec(), "old");
}

#[test]
fn put_non_markdown_is_403() {
    let f = spec_fixture();
    f.store.write(&session_a(), "spec.md.txt", "old");

    for path in [SESSION_FILE, "spec.md.txt"] {
        let target = format!("/api/file/o/stack/main/a/{path}");
        let before = f.store.read(&session_a(), path);

        let reply = put(f.port(), &target, &[("If-Match", "\"v1\"")], "{}");

        assert_eq!(reply.status, 403, "{path}");
        assert_eq!(f.store.read(&session_a(), path), before, "{path}");
    }
}

#[test]
fn put_foreign_origin_is_403() {
    let f = spec_fixture();
    let headers = [("If-Match", "\"v1\""), ("Origin", "http://evil")];

    let reply = put(f.port(), SPEC, &headers, "new");

    assert_eq!(reply.status, 403);
    assert_eq!(f.spec(), "old");
}

#[test]
fn put_accepts_same_origin() {
    let f = spec_fixture();
    let origin = format!("http://127.0.0.1:{}", f.port());
    let headers = [("If-Match", "\"v1\""), ("Origin", origin.as_str())];

    let reply = put(f.port(), SPEC, &headers, "new");

    assert_eq!(reply.status, 200);
    assert_eq!(f.spec(), "new");
}

#[test]
fn put_missing_file_is_404() {
    let f = spec_fixture();
    let headers = [("If-Match", "\"v1\"")];

    let missing_file = put(
        f.port(),
        "/api/file/o/stack/main/a/nope.md",
        &headers,
        "new",
    );
    let missing_session = put(
        f.port(),
        "/api/file/o/stack/main/nope/spec.md",
        &headers,
        "new",
    );

    assert_eq!(missing_file.status, 404);
    assert_eq!(missing_session.status, 404);
    assert_eq!(f.store.read(&session_a(), "nope.md"), None);
    assert_eq!(
        f.store.read(&session("o", "stack", "nope"), "spec.md"),
        None
    );
}

#[test]
fn put_bad_name_is_404() {
    let f = spec_fixture();

    for target in [
        "/api/file/o/%2e%2e/main/a/spec.md",
        "/api/file/o/stack/main/../spec.md",
        "/api/file/o/stack/main/a/%2e%2e/a/spec.md",
        "/api/file/o/stack/main/a/.hidden.md",
        "/api/file/o/stack/main/a/%FF.md",
        "/api/file/o//main/a/spec.md",
    ] {
        let reply = put(f.port(), target, &[("If-Match", "\"v1\"")], "new");

        assert_eq!(reply.status, 404, "{target}");
        assert_eq!(f.spec(), "old", "{target}");
    }
}

#[test]
fn put_elsewhere_is_404() {
    let f = spec_fixture();

    for target in [
        "/api/sessions",
        "/api/files/o/stack/main/a",
        "/api/file/o/stack/main/a",
        "/api/archive/o/stack/main/a",
        "/dashboard.html",
    ] {
        let reply = put(f.port(), target, &[("If-Match", "\"v1\"")], "new");

        assert_eq!(reply.status, 404, "{target}");
    }

    assert_eq!(f.session_json(), SESSION);
    assert_eq!(f.spec(), "old");
}

#[test]
fn lazygit_upgrade_needs_the_board_origin() {
    let f = session_fixture();

    let missing = request(f.port(), "GET", LAZYGIT, &upgrade(None, "13", Some(KEY)));
    let foreign = upgrade(Some("http://evil"), "13", Some(KEY));
    let foreign = request(f.port(), "GET", LAZYGIT, &foreign);

    assert_eq!(missing.status, 403);
    assert_eq!(foreign.status, 403);
}

#[test]
fn a_session_page_cannot_open_lazygit() {
    let f = session_fixture();
    f.store
        .write(&session_a(), "report.html", "<script></script>");

    let page = get(f.port(), "/api/file/o/stack/main/a/report.html");
    let sandboxed = upgrade(Some("null"), "13", Some(KEY));
    let sandboxed = request(f.port(), "GET", LAZYGIT, &sandboxed);

    assert_eq!(page.status, 200);
    assert_eq!(page.header("Content-Security-Policy"), Some("sandbox"));
    assert_eq!(sandboxed.status, 403);
}

#[test]
fn lazygit_upgrade_needs_version_13_and_a_key() {
    let f = session_fixture();
    let origin = format!("http://127.0.0.1:{}", f.port());

    let version_8 = upgrade(Some(&origin), "8", Some(KEY));
    let version_8 = request(f.port(), "GET", LAZYGIT, &version_8);
    let no_key = upgrade(Some(&origin), "13", None);
    let no_key = request(f.port(), "GET", LAZYGIT, &no_key);

    assert_eq!(version_8.status, 400);
    assert_eq!(no_key.status, 400);
}

#[test]
fn lazygit_on_a_foreign_host_is_403() {
    let f = session_fixture();
    let mut headers = vec![("Host", "evil:1")];
    headers.extend(upgrade(Some("http://evil:1"), "13", Some(KEY)));

    let reply = send(f.port(), "GET", LAZYGIT, &headers);

    assert_eq!(reply.status, 403);
}

#[test]
fn lazygit_without_upgrade_is_404() {
    let f = session_fixture();

    let reply = get(f.port(), LAZYGIT);

    assert_eq!(reply.status, 404);
}

#[test]
fn lazygit_on_an_unknown_session_closes_with_1011() {
    let f = session_fixture();
    let uri = format!("ws://127.0.0.1:{}/api/lazygit/o/stack/main/nope", f.port());
    let origin = format!("http://127.0.0.1:{}", f.port());
    let client = ClientRequestBuilder::new(uri.parse().unwrap()).with_header("Origin", origin);

    let stream = TcpStream::connect(("127.0.0.1", f.port())).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (mut socket, response) = tungstenite::client(client, stream).unwrap();

    assert_eq!(response.status(), 101);

    let message = socket.read().unwrap();
    let Message::Close(Some(frame)) = message else {
        panic!("no close frame: {message:?}");
    };
    assert_eq!(u16::from(frame.code), 1011);
}

#[test]
fn terminal_upgrade_needs_the_board_origin() {
    let dir = tempfile::tempdir().unwrap();
    let Some((main, linked)) = repo_with_worktree(dir.path()) else {
        eprintln!("skipped: git is missing");
        return;
    };
    let f = repo_fixture(main);
    let target = terminal_target(&linked, "shell");

    let missing = upgrade(None, "13", Some(KEY));
    let missing = request(f.port(), "GET", &target, &missing);
    let foreign = upgrade(Some("http://evil"), "13", Some(KEY));
    let foreign = request(f.port(), "GET", &target, &foreign);

    assert_eq!(missing.status, 403);
    assert_eq!(foreign.status, 403);
}

#[test]
fn a_session_page_cannot_open_a_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let Some((main, linked)) = repo_with_worktree(dir.path()) else {
        eprintln!("skipped: git is missing");
        return;
    };
    let f = repo_fixture(main);
    f.store.write(&session_a(), SESSION_FILE, SESSION);
    f.store
        .write(&session_a(), "report.html", "<script></script>");
    let target = terminal_target(&linked, "shell");

    let page = get(f.port(), "/api/file/o/stack/main/a/report.html");
    let sandboxed = upgrade(Some("null"), "13", Some(KEY));
    let sandboxed = request(f.port(), "GET", &target, &sandboxed);

    assert_eq!(page.status, 200);
    assert_eq!(page.header("Content-Security-Policy"), Some("sandbox"));
    assert_eq!(sandboxed.status, 403);
}

#[test]
fn terminals_route_lists_no_terminals_at_start() {
    let f = fixture();

    let reply = get(f.port(), "/api/terminals");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.json(), json!({ "terminals": [] }));
}

#[test]
fn terminal_posts_check_origin_id_action_and_name() {
    let f = fixture();
    let port = f.port();
    let foreign = [("Origin", "http://evil")];

    assert_eq!(post(port, "/api/terminals/1/kill").status, 404);
    assert_eq!(post(port, "/api/terminals/1/seen").status, 404);
    assert_eq!(post(port, "/api/terminals/x/kill").status, 404);
    assert_eq!(post(port, "/api/terminals/1/stop").status, 404);
    assert_eq!(post(port, "/api/terminals/1/name").status, 400);
    assert_eq!(
        request_body(port, "POST", "/api/terminals/1/name", &[], "build").status,
        404
    );
    assert_eq!(
        request(port, "POST", "/api/terminals/1/kill", &foreign).status,
        403
    );
    assert_eq!(
        request(port, "POST", "/api/terminals/1/seen", &foreign).status,
        403
    );
}

#[test]
fn taken_port_is_an_error() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();

    let store = Arc::new(FakeStore::default());
    let repos = Arc::new(FixedRepos(vec![]));

    let Err(err) = Server::start(store, repos, port) else {
        panic!("started on taken port {port}");
    };

    assert_eq!(err.kind(), ErrorKind::AddrInUse);
}

#[test]
fn port_is_free_after_drop() {
    let Fixture { server, store } = fixture();
    let port = server.port();

    drop(server);

    let again = Server::start(store, Arc::new(FixedRepos(vec![])), port).unwrap();
    assert_eq!(again.port(), port);
}

#[test]
fn idle_connection_does_not_block_others() {
    let f = fixture();
    let _idle = TcpStream::connect(("127.0.0.1", f.port())).unwrap();
    let start = Instant::now();

    let reply = get(f.port(), "/api/sessions");

    assert_eq!(reply.status, 200);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn every_response_closes_and_is_not_cached() {
    let f = session_fixture();

    for target in [
        "/api/sessions",
        "/api/file/o/stack/main/a/nope.md",
        "/dashboard.html",
    ] {
        let reply = get(f.port(), target);

        assert_eq!(reply.header("Cache-Control"), Some("no-store"), "{target}");
        assert_eq!(reply.header("Connection"), Some("close"), "{target}");
        assert_eq!(
            reply.header("Content-Length"),
            Some(reply.body.len().to_string().as_str()),
            "{target}"
        );
    }
}
