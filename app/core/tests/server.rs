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

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use jam_board_core::server::Server;
use jam_board_core::server::dashboard::embedded;

struct Fixture {
    server: Server,
    root: PathBuf,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn port(&self) -> u16 {
        self.server.port()
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    fn mkdir(&self, path: &str) {
        fs::create_dir_all(self.root.join(path)).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).unwrap()
    }
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(dir.path().join("outside.txt"), "outside").unwrap();

    let server = Server::start(root.clone(), 0).unwrap();

    Fixture {
        server,
        root,
        _dir: dir,
    }
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
}

/// Sends a request with `Host: 127.0.0.1:<port>` and `headers`.
fn request(port: u16, method: &str, target: &str, headers: &[(&str, &str)]) -> Reply {
    let host = format!("127.0.0.1:{port}");
    let mut all = vec![("Host", host.as_str())];
    all.extend_from_slice(headers);

    send(port, method, target, &all)
}

/// Sends a request with exactly `headers`.
fn send(port: u16, method: &str, target: &str, headers: &[(&str, &str)]) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    let mut head = format!("{method} {target} HTTP/1.1\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();

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

const JOB_FILE: &str = "stack/jobs/a/job.json";
const JOB: &str = r#"{"id":"a","stage":"done","archived":false,"counts":{"nit":1}}"#;

/// A fixture whose root holds the job `stack/a`.
fn job_fixture() -> Fixture {
    let f = fixture();
    f.write(JOB_FILE, JOB);
    f
}

/// Whether `text` has the shape `YYYY-MM-DDTHH:MM:SSZ`.
fn is_utc_timestamp(text: &str) -> bool {
    let shape = "0000-00-00T00:00:00Z";

    text.len() == shape.len()
        && text.bytes().zip(shape.bytes()).all(|(c, s)| match s {
            b'0' => c.is_ascii_digit(),
            _ => c == s,
        })
}

#[test]
fn serves_a_file() {
    let f = fixture();
    f.write("stack/jobs/a/job.json", r#"{"id":"a"}"#);

    let reply = get(f.port(), "/stack/jobs/a/job.json");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), r#"{"id":"a"}"#);
    assert_eq!(reply.header("Content-Type"), Some("application/json"));
}

#[test]
fn missing_file_is_404() {
    let f = fixture();

    let reply = get(f.port(), "/nope.md");

    assert_eq!(reply.status, 404);
}

#[test]
fn lists_folders_with_trailing_slash() {
    let f = fixture();
    f.mkdir("jam-factory");
    f.mkdir("stack");
    f.write("config.json", r#"{ "port": 8731 }"#);

    let reply = get(f.port(), "/");

    assert_eq!(reply.status, 200);
    let body = reply.text();
    assert!(body.contains(r#"<a href="jam-factory/">"#), "{body}");
    assert!(body.contains(r#"<a href="stack/">"#), "{body}");
    assert!(body.contains(r#"<a href="config.json">"#), "{body}");
}

#[test]
fn percent_encodes_names() {
    let f = fixture();
    f.write("stack/jobs/my job/caf\u{e9}.md", "hi");

    let jobs = get(f.port(), "/stack/jobs/").text();
    assert!(jobs.contains(r#"<a href="my%20job/">"#), "{jobs}");

    let job = get(f.port(), "/stack/jobs/my%20job/").text();
    assert!(job.contains(r#"<a href="caf%C3%A9.md">"#), "{job}");

    let file = get(f.port(), "/stack/jobs/my%20job/caf%C3%A9.md");
    assert_eq!(file.status, 200);
    assert_eq!(file.text(), "hi");
}

#[test]
fn redirects_folder_without_slash() {
    let f = fixture();
    f.mkdir("stack");

    let reply = get(f.port(), "/stack");

    assert_eq!(reply.status, 301);
    assert_eq!(reply.header("Location"), Some("/stack/"));
}

#[test]
fn file_with_trailing_slash_is_404() {
    let f = fixture();
    f.write("config.json", r#"{ "port": 8731 }"#);

    let reply = get(f.port(), "/config.json/");

    assert_eq!(reply.status, 404);
}

#[test]
fn path_outside_root_is_404() {
    let f = fixture();
    f.mkdir("stack");

    for target in [
        "/../outside.txt",
        "/%2e%2e/outside.txt",
        "/stack/../../outside.txt",
    ] {
        let reply = get(f.port(), target);

        assert_eq!(reply.status, 404, "{target}");
    }
}

#[test]
fn query_string_is_ignored() {
    let f = fixture();
    f.write("config.json", r#"{ "port": 8731 }"#);

    let reply = get(f.port(), "/config.json?x=1");

    assert_eq!(reply.status, 200);
}

#[test]
fn embedded_dashboard_wins_over_disk() {
    let f = fixture();

    for (name, content_type) in [
        ("dashboard.html", "text/html; charset=utf-8"),
        ("marked.min.js", "text/javascript; charset=utf-8"),
        ("purify.min.js", "text/javascript; charset=utf-8"),
    ] {
        f.write(name, "stale");
        let path = format!("/{name}");

        let reply = get(f.port(), &path);

        assert_eq!(reply.status, 200, "{name}");
        assert_eq!(reply.body, embedded(&path).unwrap().bytes, "{name}");
        assert_eq!(reply.header("Content-Type"), Some(content_type), "{name}");
    }
}

#[test]
fn wrong_method_is_405() {
    let f = fixture();

    let reply = request(f.port(), "PUT", "/", &[]);

    assert_eq!(reply.status, 405);
}

#[test]
fn foreign_host_is_403() {
    let f = job_fixture();
    f.write("config.json", r#"{ "port": 8731 }"#);

    for host in [
        format!("evil.example:{}", f.port()),
        format!("127.0.0.1:{}", f.port().wrapping_add(1)),
        "localhost".to_string(),
    ] {
        let headers = [("Host", host.as_str())];

        let get = send(f.port(), "GET", "/config.json", &headers);
        let post = send(f.port(), "POST", "/archive/stack/a", &headers);

        assert_eq!(get.status, 403, "{host}");
        assert_eq!(post.status, 403, "{host}");
        assert_eq!(f.read(JOB_FILE), JOB, "{host}");
    }
}

#[test]
fn missing_host_is_403() {
    let f = job_fixture();
    f.write("config.json", r#"{ "port": 8731 }"#);

    let get = send(f.port(), "GET", "/config.json", &[]);
    let post = send(f.port(), "POST", "/archive/stack/a", &[]);

    assert_eq!(get.status, 403);
    assert_eq!(post.status, 403);
    assert_eq!(f.read(JOB_FILE), JOB);
}

#[test]
fn local_hosts_are_served() {
    for name in ["localhost", "127.0.0.1"] {
        let f = job_fixture();
        f.write("config.json", r#"{ "port": 8731 }"#);
        let host = format!("{name}:{}", f.port());
        let origin = format!("http://{host}");

        let get = send(f.port(), "GET", "/config.json", &[("Host", &host)]);
        let post = send(
            f.port(),
            "POST",
            "/archive/stack/a",
            &[("Host", &host), ("Origin", &origin)],
        );

        assert_eq!(get.status, 200, "{host}");
        assert_eq!(post.status, 204, "{host}");
    }
}

#[test]
fn taken_port_is_an_error() {
    let f = fixture();
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();

    let Err(err) = Server::start(f.root.clone(), port) else {
        panic!("started on taken port {port}");
    };

    assert_eq!(err.kind(), ErrorKind::AddrInUse);
}

#[test]
fn port_is_free_after_drop() {
    let Fixture { server, root, _dir } = fixture();
    let port = server.port();

    drop(server);

    let again = Server::start(root, port).unwrap();
    assert_eq!(again.port(), port);
}

#[test]
fn idle_connection_does_not_block_others() {
    let f = fixture();
    let _idle = TcpStream::connect(("127.0.0.1", f.port())).unwrap();
    let start = Instant::now();

    let reply = get(f.port(), "/");

    assert_eq!(reply.status, 200);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn every_response_closes_and_is_not_cached() {
    let f = fixture();
    f.write("config.json", r#"{ "port": 8731 }"#);

    for target in ["/config.json", "/nope.md", "/"] {
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

#[test]
fn archive_sets_fields_and_keeps_order() {
    let f = job_fixture();

    let reply = post(f.port(), "/archive/stack/a");

    assert_eq!(reply.status, 204);
    assert!(reply.body.is_empty());

    let text = f.read(JOB_FILE);
    let job: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).unwrap();
    let keys: Vec<&str> = job.keys().map(String::as_str).collect();
    assert_eq!(keys, ["id", "stage", "archived", "counts", "updated"]);
    assert_eq!(job["archived"], true);

    let updated = job["updated"].as_str().unwrap();
    assert!(is_utc_timestamp(updated), "{updated}");

    assert!(text.lines().nth(1).unwrap().starts_with("  "), "{text}");
    assert!(text.ends_with('\n'), "{text}");
    assert!(!f.root.join("stack/jobs/a/job.json.tmp").exists());
}

#[test]
fn archive_accepts_same_origin() {
    let f = job_fixture();
    let origin = format!("http://127.0.0.1:{}", f.port());

    let reply = request(f.port(), "POST", "/archive/stack/a", &[("Origin", &origin)]);

    assert_eq!(reply.status, 204);
}

#[test]
fn archive_rejects_foreign_origin() {
    let f = job_fixture();

    let reply = request(
        f.port(),
        "POST",
        "/archive/stack/a",
        &[("Origin", "http://evil.example")],
    );

    assert_eq!(reply.status, 403);
    assert_eq!(f.read(JOB_FILE), JOB);
}

#[test]
fn archive_rejects_bad_names() {
    let f = job_fixture();

    // Each target next to the `job.json` its names point at.
    let targets = [
        ("/archive/%2e%2e/a", "../jobs/a/job.json"),
        ("/archive/stack/..", "stack/job.json"),
        ("/archive/stack/.", "stack/jobs/job.json"),
        ("/archive/stack/a%2F..%2Fa", JOB_FILE),
        ("/archive/stack/a%5Cb", "stack/jobs/a\\b/job.json"),
        ("/archive//a", "jobs/a/job.json"),
    ];

    for (_, file) in targets {
        f.write(file, JOB);
    }

    for (target, file) in targets {
        let reply = post(f.port(), target);

        assert_eq!(reply.status, 404, "{target}");
        assert_eq!(f.read(file), JOB, "{target}");
    }
}

#[test]
fn archive_rejects_extra_segments() {
    let f = job_fixture();

    let reply = post(f.port(), "/archive/stack/a/extra");

    assert_eq!(reply.status, 404);
    assert_eq!(f.read(JOB_FILE), JOB);
}

#[test]
fn archive_missing_job_is_404() {
    let f = job_fixture();

    let reply = post(f.port(), "/archive/stack/b");

    assert_eq!(reply.status, 404);
}

#[test]
fn archive_bad_json_is_500() {
    let f = job_fixture();
    f.write(JOB_FILE, "not json");

    let reply = post(f.port(), "/archive/stack/a");

    assert_eq!(reply.status, 500);
    assert_eq!(f.read(JOB_FILE), "not json");
}

#[test]
fn open_rejects_foreign_origin() {
    let f = job_fixture();

    let reply = request(
        f.port(),
        "POST",
        "/open/zed/stack/a",
        &[("Origin", "http://evil.example")],
    );

    assert_eq!(reply.status, 403);
}

#[test]
fn open_missing_job_is_404() {
    let f = job_fixture();

    let reply = post(f.port(), "/open/zed/nope/nope");

    assert_eq!(reply.status, 404);
}

#[test]
fn post_elsewhere_is_404() {
    let f = job_fixture();

    let reply = post(f.port(), "/stack/");

    assert_eq!(reply.status, 404);
}
