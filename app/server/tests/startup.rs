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
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_jam-factory-server");
const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(50);

/// The `jam-factory-mcp` binary of the workspace debug build, if it is built.
fn mcp_binary() -> Option<PathBuf> {
    let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug/jam-factory-mcp");

    binary.is_file().then_some(binary)
}

/// A free port on `127.0.0.1`.
fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();

    listener.local_addr().unwrap().port()
}

/// A temp home folder whose config file holds `port`.
fn home_with_port(port: u16) -> TempDir {
    let home = tempfile::tempdir().unwrap();
    let config = config_path(home.path());

    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, format!(r#"{{ "port": {port} }}"#)).unwrap();

    home
}

fn config_path(home: &Path) -> PathBuf {
    home.join(".jam-factory").join("config.json")
}

/// Links `mcp` into `{home}/.local/bin`.
fn link_mcp_binary(home: &Path, mcp: &Path) {
    let bin = home.join(".local").join("bin");
    fs::create_dir_all(&bin).unwrap();

    symlink(mcp, bin.join("jam-factory-mcp")).unwrap();
}

/// Starts the server on `home`, with a bare `PATH` and its stderr piped.
fn start(home: &Path) -> Child {
    Command::new(BINARY)
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Waits up to `TIMEOUT` for `child` to exit, and returns its status and its stderr.
fn wait_for_exit(mut child: Child) -> (ExitStatus, String) {
    let start = Instant::now();

    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }

        if start.elapsed() > TIMEOUT {
            child.kill().unwrap();
            panic!("the server did not exit");
        }

        thread::sleep(POLL);
    };

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();

    (status, stderr)
}

/// The status code of `GET /dashboard.html` on `127.0.0.1:{port}`, or `None` when nothing answers.
fn dashboard_status(port: u16) -> Option<u16> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;

    let request = format!(
        "GET /dashboard.html HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;

    let mut status_line = String::new();
    BufReader::new(stream).read_line(&mut status_line).ok()?;

    status_line.split_whitespace().nth(1)?.parse().ok()
}

#[test]
fn serves_the_dashboard_on_the_config_port() {
    let Some(mcp) = mcp_binary() else {
        eprintln!("skipped: jam-factory-mcp is not built");
        return;
    };

    let port = free_port();
    let home = home_with_port(port);
    link_mcp_binary(home.path(), &mcp);

    let mut child = start(home.path());
    let start = Instant::now();

    let mut status = dashboard_status(port);
    while status != Some(200) && start.elapsed() < TIMEOUT {
        thread::sleep(POLL);
        status = dashboard_status(port);
    }

    child.kill().unwrap();
    child.wait().unwrap();

    assert_eq!(status, Some(200));
}

#[test]
fn taken_port_exits_with_an_error() {
    let Some(mcp) = mcp_binary() else {
        eprintln!("skipped: jam-factory-mcp is not built");
        return;
    };

    let taken = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = taken.local_addr().unwrap().port();
    let home = home_with_port(port);
    link_mcp_binary(home.path(), &mcp);

    let (status, stderr) = wait_for_exit(start(home.path()));

    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains(&port.to_string()), "{stderr}");
}

#[test]
fn missing_mcp_binary_exits_with_an_error() {
    let home = home_with_port(free_port());

    let (status, stderr) = wait_for_exit(start(home.path()));

    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains("jam-factory-mcp"), "{stderr}");
    assert!(stderr.contains("just install"), "{stderr}");
}

#[test]
fn bad_config_exits_with_an_error() {
    let home = home_with_port(0);
    let config = config_path(home.path());

    let (status, stderr) = wait_for_exit(start(home.path()));

    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains(&config.display().to_string()), "{stderr}");
}
