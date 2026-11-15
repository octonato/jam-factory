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

//! Runs a program in a worktree over a WebSocket on `GET /api/lazygit/<org>/<repo>/<branch>/<id>` and `GET /api/terminal`.

use std::env;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use jam_factory_store::sessions::SESSION_FILE;
use serde_json::Value;
use tungstenite::handshake::derive_accept_key;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, Role};
use tungstenite::{Error, Message, WebSocket};

use super::api::{configured_worktree, query_field, segments, session_ref, worktree};
use super::http::{Request, Response};
use super::pty::{Program, Pty};
use super::repos::RepoList;
use crate::store::Store;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

/// The login shell when `SHELL` is unset or empty.
const DEFAULT_SHELL: &str = "/bin/zsh";

/// The close reason for a path that is not a folder of a configured worktree.
const NOT_A_WORKTREE: &str = "not a configured worktree";

/// The read timeout of the socket while it relays.
const POLL: Duration = Duration::from_millis(10);

/// The longest wait for the client to finish the close handshake.
const CLOSE_WAIT: Duration = Duration::from_secs(1);

/// The longest close reason, in bytes.
const MAX_REASON: usize = 123;

/// Whether `request` is a `GET` on `/api/lazygit/` or `/api/terminal` that asks for a WebSocket.
pub fn is_upgrade(request: &Request) -> bool {
    request.method == "GET"
        && (lazygit_route(request) || terminal_route(request))
        && request
            .header("Upgrade")
            .is_some_and(|upgrade| upgrade.eq_ignore_ascii_case("websocket"))
}

/// Whether the path of `request` starts with `/api/lazygit`.
fn lazygit_route(request: &Request) -> bool {
    segments(&request.target).starts_with(&["", "api", "lazygit"])
}

/// Whether the path of `request` is `/api/terminal`.
fn terminal_route(request: &Request) -> bool {
    segments(&request.target) == ["", "api", "terminal"]
}

/// Upgrades `request` on `stream` and runs its program in the worktree that its path names.
pub fn serve(store: &dyn Store, repos: &dyn RepoList, request: &Request, stream: TcpStream) {
    if terminal_route(request) {
        let shell = env::var_os("SHELL");
        serve_worktree_with(repos, request, stream, shell.as_deref());
    } else {
        serve_with(store, request, stream, &lazygit());
    }
}

/// Upgrades `request` on `stream` and runs `program` in the worktree of its session.
pub fn serve_with(store: &dyn Store, request: &Request, stream: TcpStream, program: &Program) {
    let target = session_worktree(store, &request.target).map(|dir| (dir, program.clone()));

    bridge(request, stream, target);
}

/// Upgrades `request` on `stream` and runs the `program` of its query in the configured worktree at its `path`.
pub fn serve_worktree_with(
    repos: &dyn RepoList,
    request: &Request,
    mut stream: TcpStream,
    shell: Option<&OsStr>,
) {
    let name = query_field(&request.target, "program").unwrap_or_default();

    let Some(program) = program_for(&name, shell) else {
        let _ = Response::error(400).write_to(&mut stream);
        return;
    };

    let target = configured_worktree(repos, &request.target)
        .map(|dir| (dir, program))
        .ok_or_else(|| NOT_A_WORKTREE.to_string());

    bridge(request, stream, target);
}

/// The program named `name`: the login shell `shell` for `shell`, Claude in that interactive login shell for `claude`, or lazygit for `lazygit`.
fn program_for(name: &str, shell: Option<&OsStr>) -> Option<Program> {
    match name {
        "shell" => Some(Program {
            name: login_shell(shell),
            args: vec!["-l".to_string()],
        }),
        "claude" => Some(Program {
            name: login_shell(shell),
            args: ["-l", "-i", "-c", "claude"].map(str::to_string).to_vec(),
        }),
        "lazygit" => Some(lazygit()),
        _ => None,
    }
}

/// The shell `shell`, or `/bin/zsh` when it is unset or empty.
fn login_shell(shell: Option<&OsStr>) -> String {
    shell
        .filter(|shell| !shell.is_empty())
        .map_or(DEFAULT_SHELL.to_string(), |shell| {
            shell.to_string_lossy().into_owned()
        })
}

/// The program `lazygit`.
fn lazygit() -> Program {
    Program {
        name: "lazygit".to_string(),
        args: Vec::new(),
    }
}

/// Upgrades `request` on `stream` and runs the program of `target` in its folder, or closes with 1011 and the reason.
fn bridge(request: &Request, mut stream: TcpStream, target: Result<(PathBuf, Program), String>) {
    let key = match handshake_key(request) {
        Ok(key) => key,
        Err(status) => {
            let _ = Response::error(status).write_to(&mut stream);
            return;
        }
    };

    if write_switch(&mut stream, key).is_err() || stream.set_read_timeout(Some(POLL)).is_err() {
        return;
    }

    let mut socket = WebSocket::from_raw_socket(stream, Role::Server, None);

    let pty = target.and_then(|(dir, program)| {
        let (cols, rows) = size(&request.target);
        Pty::spawn(&program, &dir, cols, rows)
    });

    match pty {
        Ok(mut pty) => {
            relay(&mut socket, &mut pty);
            finish(&mut socket);
        }
        Err(reason) => {
            close(&mut socket, CloseCode::Error, &reason);
            finish(&mut socket);
        }
    }
}

/// The `Sec-WebSocket-Key` of `request`, or the status that refuses the upgrade.
fn handshake_key(request: &Request) -> Result<&str, u16> {
    if !board_origin(request) {
        return Err(403);
    }

    if request.header("Sec-WebSocket-Version") != Some("13") {
        return Err(400);
    }

    request.header("Sec-WebSocket-Key").ok_or(400)
}

/// Whether the request has an `Origin` equal to `http://<Host>`.
fn board_origin(request: &Request) -> bool {
    let origin = request.header("Origin");
    let host = request.header("Host");

    origin
        .zip(host)
        .is_some_and(|(origin, host)| origin == format!("http://{host}"))
}

/// Writes the `101 Switching Protocols` reply to the handshake with `key`.
fn write_switch(stream: &mut TcpStream, key: &str) -> io::Result<()> {
    let accept = derive_accept_key(key.as_bytes());

    let head = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\
         \r\n"
    );

    stream.write_all(head.as_bytes())
}

/// The worktree of the session named by the path in `target`.
fn session_worktree(store: &dyn Store, target: &str) -> Result<PathBuf, String> {
    let segments = segments(target);

    let ["", "api", "lazygit", org, repo, branch, id] = segments.as_slice() else {
        return Err("the path names no session".to_string());
    };

    let session = session_ref([org, repo, branch, id])
        .and_then(|s| store.read_file(&s, SESSION_FILE))
        .map_err(|error| error.to_string())?;

    worktree(&session.content).ok_or_else(|| "session has no worktree".to_string())
}

/// The `cols` and `rows` of the query in `target`, each 80 or 24 when missing or invalid.
fn size(target: &str) -> (u16, u16) {
    let field = |name: &str| {
        query_field(target, name)
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|value| *value > 0)
    };

    let cols = field("cols").unwrap_or(DEFAULT_COLS);
    let rows = field("rows").unwrap_or(DEFAULT_ROWS);
    (cols, rows)
}

/// The `cols` and `rows` of the resize message `text`, when both are positive integers.
fn resize(text: &str) -> Option<(u16, u16)> {
    let message: Value = serde_json::from_str(text).ok()?;

    let field = |name: &str| {
        let value = message.get(name)?.as_u64()?;
        u16::try_from(value).ok().filter(|value| *value > 0)
    };

    Some((field("cols")?, field("rows")?))
}

/// The longest prefix of `text` that fits in a close frame, cut on a char boundary.
fn close_reason(text: &str) -> &str {
    &text[..text.floor_char_boundary(MAX_REASON)]
}

/// Relays between `socket` and `pty` until the program ends, or the socket closes or fails.
fn relay(socket: &mut WebSocket<TcpStream>, pty: &mut Pty) {
    loop {
        loop {
            match pty.output().try_recv() {
                Ok(chunk) => {
                    if socket.send(Message::Binary(chunk.into())).is_err() {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    close(socket, CloseCode::Normal, "");
                    return;
                }
            }
        }

        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                let _ = pty.write(&bytes);
            }
            Ok(Message::Text(text)) => {
                if let Some((cols, rows)) = resize(text.as_str()) {
                    let _ = pty.resize(cols, rows);
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(Error::Io(error)) if waits(&error) => {}
            Err(_) => return,
        }
    }
}

/// Whether `error` is a read timeout.
fn waits(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// Sends a close frame with `code` and `reason`.
fn close(socket: &mut WebSocket<TcpStream>, code: CloseCode, reason: &str) {
    let frame = CloseFrame {
        code,
        reason: close_reason(reason).into(),
    };

    let _ = socket.close(Some(frame));
}

/// Reads `socket` until the close handshake ends, the socket fails or `CLOSE_WAIT` passes.
fn finish(socket: &mut WebSocket<TcpStream>) {
    let deadline = Instant::now() + CLOSE_WAIT;

    while Instant::now() < deadline {
        match socket.read() {
            Ok(_) => {}
            Err(Error::Io(error)) if waits(&error) => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::net::TcpListener;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::thread;

    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use tungstenite::{Bytes, ClientRequestBuilder, HandshakeError};

    use super::*;
    use crate::server::fake_store::FakeStore;
    use crate::server::http::read_request;
    use crate::server::repos::FixedRepos;

    const TIMEOUT: Duration = Duration::from_secs(5);
    const TARGET: &str = "/api/lazygit/o/stack/main/a";

    /// The program `sh -c script`.
    fn sh(script: &str) -> Program {
        Program {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
        }
    }

    /// A store whose session `o/stack/main/a` has `dir` as its worktree.
    fn store(dir: &Path) -> FakeStore {
        let store = FakeStore::default();
        let session = serde_json::json!({ "worktree": dir });
        store.write(SESSION_FILE, &session.to_string());
        store
    }

    fn request(method: &str, target: &str, headers: &[(&str, &str)]) -> Request {
        Request {
            method: method.to_string(),
            target: target.to_string(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            body: Vec::new(),
        }
    }

    /// A client socket on `target`, which `serve_with` answers with `store` and `program` on another thread.
    fn connect(store: FakeStore, target: &str, program: Program) -> WebSocket<TcpStream> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream).unwrap();
            serve_with(&store, &request, stream, &program);
        });

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(TIMEOUT)).unwrap();

        let uri = format!("ws://127.0.0.1:{port}{target}").parse().unwrap();
        let client = ClientRequestBuilder::new(uri)
            .with_header("Origin", format!("http://127.0.0.1:{port}"));

        let (socket, _) = tungstenite::client(client, stream).unwrap();
        socket
    }

    /// A client socket on `target`, which `serve_worktree_with` answers with `repos` and `shell` on another thread, or the status that refuses it.
    fn connect_worktree(
        repos: FixedRepos,
        target: &str,
        shell: &Path,
    ) -> Result<WebSocket<TcpStream>, u16> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let shell = shell.as_os_str().to_owned();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream).unwrap();
            serve_worktree_with(&repos, &request, stream, Some(&shell));
        });

        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(TIMEOUT)).unwrap();

        let uri = format!("ws://127.0.0.1:{port}{target}").parse().unwrap();
        let client = ClientRequestBuilder::new(uri)
            .with_header("Origin", format!("http://127.0.0.1:{port}"));

        match tungstenite::client(client, stream) {
            Ok((socket, _)) => Ok(socket),
            Err(HandshakeError::Failure(Error::Http(response))) => Err(response.status().as_u16()),
            Err(error) => panic!("no handshake: {error}"),
        }
    }

    /// The `/api/terminal` target that runs `program` in the folder `path`.
    fn terminal_target(path: &Path, program: &str) -> String {
        let path = utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string();
        format!("/api/terminal?path={path}&program={program}")
    }

    /// Writes the executable shell script `body` to `file`.
    fn write_program(file: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(file, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A stub shell in `dir` that creates `dir/ran` and prints its folder.
    fn stub_shell(dir: &Path) -> PathBuf {
        let shell = dir.join("shell");
        let ran = dir.join("ran");
        write_program(&shell, &format!("touch '{}'\npwd", ran.display()));
        shell
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

    /// The canonical main checkout `o/r/main` under `dir` and its linked worktree `o/r/work tree's`, or `None` without git.
    fn repo_with_worktree(dir: &Path) -> Option<(PathBuf, PathBuf)> {
        if Command::new("git").arg("--version").output().is_err() {
            return None;
        }

        let dir = fs::canonicalize(dir).unwrap();
        let main = dir.join("o/r/main");
        let linked = dir.join("o/r/work tree's");

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
            &["worktree", "add", "-q", "-b", "work", "../work tree's"],
        );

        Some((main, linked))
    }

    /// The binary frames of `socket` up to the first one that makes them hold `needle`.
    fn read_until(socket: &mut WebSocket<TcpStream>, needle: &str) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();

        while !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()) {
            match socket.read() {
                Ok(Message::Binary(chunk)) => bytes.extend_from_slice(&chunk),
                Ok(_) => {}
                Err(error) => {
                    let text = String::from_utf8_lossy(&bytes);
                    panic!("no {needle:?} in {text:?}: {error}");
                }
            }
        }

        bytes
    }

    /// The code and reason of the close frame that `socket` gets.
    fn close_of(socket: &mut WebSocket<TcpStream>) -> (u16, String) {
        loop {
            match socket.read() {
                Ok(Message::Close(Some(frame))) => {
                    return (frame.code.into(), frame.reason.to_string());
                }
                Ok(Message::Close(None)) => panic!("a close without a frame"),
                Ok(_) => {}
                Err(error) => panic!("no close frame: {error}"),
            }
        }
    }

    /// The pid that the stub `echo $$; exec sleep 60` prints on `socket`.
    fn pid_of(socket: &mut WebSocket<TcpStream>) -> u32 {
        let line = read_until(socket, "\n");
        String::from_utf8(line).unwrap().trim().parse().unwrap()
    }

    /// Whether the process `pid` exists.
    fn alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    /// Waits up to 5 s for the process `pid` to end.
    fn assert_ends(pid: u32) {
        let deadline = Instant::now() + TIMEOUT;

        while alive(pid) {
            assert!(Instant::now() < deadline, "{pid} still runs");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn is_upgrade_needs_get_lazygit_and_the_upgrade_header() {
        let upgrade = [("Upgrade", "WebSocket")];
        let files = request("GET", "/api/files/o/stack/main/a", &upgrade);

        assert!(is_upgrade(&request("GET", TARGET, &upgrade)));
        assert!(!is_upgrade(&request("POST", TARGET, &upgrade)));
        assert!(!is_upgrade(&request("GET", TARGET, &[])));
        assert!(!is_upgrade(&files));
    }

    #[test]
    fn is_upgrade_matches_the_terminal_route() {
        let upgrade = [("Upgrade", "websocket")];
        let target = "/api/terminal?path=%2Fa&program=shell";

        assert!(is_upgrade(&request("GET", target, &upgrade)));
        assert!(!is_upgrade(&request("POST", target, &upgrade)));
        assert!(!is_upgrade(&request("GET", target, &[])));
        assert!(!is_upgrade(&request("GET", "/api/terminal/x", &upgrade)));
    }

    #[test]
    fn program_for_maps_names() {
        let program = |name: &str, shell: Option<&str>| {
            program_for(name, shell.map(OsStr::new)).map(|program| (program.name, program.args))
        };
        let login = |name: &str| Some((name.to_string(), vec!["-l".to_string()]));
        let claude = |name: &str| {
            let args = ["-l", "-i", "-c", "claude"].map(str::to_string).to_vec();
            Some((name.to_string(), args))
        };
        let lazygit = Some(("lazygit".to_string(), Vec::new()));

        assert_eq!(program("shell", Some("/bin/bash")), login("/bin/bash"));
        assert_eq!(program("shell", None), login("/bin/zsh"));
        assert_eq!(program("shell", Some("")), login("/bin/zsh"));
        assert_eq!(program("claude", Some("/bin/fish")), claude("/bin/fish"));
        assert_eq!(program("claude", None), claude("/bin/zsh"));
        assert_eq!(program("lazygit", None), lazygit);
        assert_eq!(program("lazygit", Some("/bin/bash")), lazygit);
        assert_eq!(program("vim", None), None);
        assert_eq!(program("vim", Some("/bin/bash")), None);
    }

    #[test]
    fn size_reads_the_query() {
        assert_eq!(size(&format!("{TARGET}?cols=120&rows=40")), (120, 40));
        assert_eq!(size(TARGET), (80, 24));
        assert_eq!(size(&format!("{TARGET}?cols=x&rows=0")), (80, 24));
        assert_eq!(size(&format!("{TARGET}?cols=100")), (100, 24));
    }

    #[test]
    fn resize_reads_cols_and_rows() {
        assert_eq!(resize(r#"{"cols":90,"rows":20}"#), Some((90, 20)));
        assert_eq!(resize(r#"{"cols":0,"rows":20}"#), None);
        assert_eq!(resize(r#"{"cols":"90","rows":20}"#), None);
        assert_eq!(resize("nope"), None);
    }

    #[test]
    fn close_reason_cuts_to_123_bytes_on_a_char_boundary() {
        let long = "\u{e9}".repeat(100);

        let reason = close_reason(&long);

        assert!(reason.len() <= 123, "{}", reason.len());
        assert_eq!(reason, "\u{e9}".repeat(61));
        assert_eq!(close_reason("short"), "short");
    }

    #[test]
    fn keys_reach_the_program_and_its_exit_closes_with_1000() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh(r#"printf ready; read line; printf "got:%s" "$line""#);
        let mut socket = connect(store(dir.path()), TARGET, program);

        read_until(&mut socket, "ready");
        let keys = Message::Binary(Bytes::from_static(b"hi\r"));
        socket.send(keys).unwrap();
        read_until(&mut socket, "got:hi");

        assert_eq!(close_of(&mut socket).0, 1000);
    }

    #[test]
    fn query_sets_the_size_and_text_frames_resize() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh("stty size; read line; stty size; read line");
        let target = format!("{TARGET}?cols=120&rows=40");
        let mut socket = connect(store(dir.path()), &target, program);

        read_until(&mut socket, "40 120");

        socket.send(Message::text("nope")).unwrap();
        socket
            .send(Message::text(r#"{"cols":100,"rows":30}"#))
            .unwrap();
        socket
            .send(Message::Binary(Bytes::from_static(b"\r")))
            .unwrap();

        read_until(&mut socket, "30 100");
    }

    #[test]
    fn missing_size_is_80_by_24() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh("stty size; read line; stty size; read line");
        let mut socket = connect(store(dir.path()), TARGET, program);

        read_until(&mut socket, "24 80");
    }

    #[test]
    fn large_and_multibyte_output_arrives_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh(r#"head -c 300000 /dev/zero | tr "\0" a; printf "\303\251END""#);
        let mut socket = connect(store(dir.path()), TARGET, program);

        let bytes = read_until(&mut socket, "END");

        let mut expected = vec![b'a'; 300_000];
        expected.extend_from_slice(&[0xC3, 0xA9]);
        expected.extend_from_slice(b"END");

        let tail = &bytes[bytes.len().saturating_sub(8)..];
        assert!(bytes == expected, "{} bytes ending {tail:?}", bytes.len());
    }

    #[test]
    fn closing_the_socket_kills_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh("echo $$; exec sleep 60");
        let mut socket = connect(store(dir.path()), TARGET, program);
        let pid = pid_of(&mut socket);

        socket.close(None).unwrap();

        assert_ends(pid);
    }

    #[test]
    fn dropping_the_connection_kills_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh("echo $$; exec sleep 60");
        let mut socket = connect(store(dir.path()), TARGET, program);
        let pid = pid_of(&mut socket);

        drop(socket);

        assert_ends(pid);
    }

    #[test]
    fn unknown_session_or_missing_worktree_closes_with_1011() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh("touch ran");

        let target = "/api/lazygit/o/stack/main/b";
        let mut socket = connect(store(dir.path()), target, program.clone());
        let (code, reason) = close_of(&mut socket);

        assert_eq!(code, 1011);
        assert!(!reason.is_empty());

        let empty = FakeStore::default();
        empty.write(SESSION_FILE, "{}");
        let mut socket = connect(empty, TARGET, program);
        let (code, reason) = close_of(&mut socket);

        assert_eq!(code, 1011);
        assert_eq!(reason, "session has no worktree");

        assert!(!dir.path().join("ran").exists());
    }

    #[test]
    fn missing_program_closes_with_1011() {
        let dir = tempfile::tempdir().unwrap();
        let program = Program {
            name: "no-such-program-jam".to_string(),
            args: vec![],
        };
        let mut socket = connect(store(dir.path()), TARGET, program);

        let (code, reason) = close_of(&mut socket);

        assert_eq!(code, 1011);
        assert!(!reason.is_empty());
    }

    #[test]
    fn terminal_runs_the_shell_in_a_configured_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let shell = stub_shell(dir.path());

        let repos = FixedRepos(vec![main]);
        let target = terminal_target(&linked, "shell");
        let mut socket = connect_worktree(repos, &target, &shell).unwrap();

        read_until(&mut socket, &linked.to_string_lossy());
    }

    #[test]
    fn unknown_program_is_400() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let shell = stub_shell(dir.path());

        let repos = FixedRepos(vec![main]);
        let target = terminal_target(&linked, "vim");
        let status = connect_worktree(repos, &target, &shell).err();

        assert_eq!(status, Some(400));
        assert!(!dir.path().join("ran").exists());
    }

    #[test]
    fn unconfigured_path_closes_with_1011() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, _)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let shell = stub_shell(dir.path());
        let parent = main.parent().unwrap().to_path_buf();

        let repos = FixedRepos(vec![main]);
        let target = terminal_target(&parent, "shell");
        let mut socket = connect_worktree(repos, &target, &shell).unwrap();
        let (code, reason) = close_of(&mut socket);

        assert_eq!(code, 1011);
        assert_eq!(reason, "not a configured worktree");
        assert!(!dir.path().join("ran").exists());
    }

    #[test]
    fn missing_worktree_folder_closes_with_1011() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let shell = stub_shell(dir.path());
        fs::remove_dir_all(&linked).unwrap();

        let repos = FixedRepos(vec![main]);
        let target = terminal_target(&linked, "shell");
        let mut socket = connect_worktree(repos, &target, &shell).unwrap();
        let (code, reason) = close_of(&mut socket);

        assert_eq!(code, 1011);
        assert_eq!(reason, "not a configured worktree");
        assert!(!dir.path().join("ran").exists());
    }
}
