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

//! The board window and the `jam-factory-mcp` server behind it.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{
    AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use jam_board_core::mcp_client::McpStore;
use jam_board_core::server::Server;

use crate::navigation::{self, Navigation};

const SERVER_BINARY: &str = "jam-factory-mcp";

const DEFAULT_PORT: u16 = 8731;

const MAIN: &str = "main";

const INSTALL_HINT: &str = "Run `just install` in app/.";

const ASK_PAGE_TO_CLOSE: &str = "onCloseRequested()";

/// Opens the board window, or shows an error dialog and exits.
pub fn open_board_window(app: &AppHandle) {
    if let Err(message) = try_open_board_window(app) {
        let handle = app.clone();
        app.dialog()
            .message(message)
            .kind(MessageDialogKind::Error)
            .show(move |_| handle.exit(1));
    }
}

/// Spawns the server binary, serves the dashboard on port 8731 or a free port, and opens one window on it.
fn try_open_board_window(app: &AppHandle) -> Result<(), String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let program = find_server(&home, env::var_os("PATH").as_deref()).ok_or_else(|| {
        format!("Cannot find {SERVER_BINARY} in ~/.local/bin or on PATH.\n\n{INSTALL_HINT}")
    })?;

    let store = Arc::new(McpStore::spawn(program)?);
    let server = with_fallback_port(DEFAULT_PORT, |port| Server::start(store.clone(), port))
        .map_err(|e| format!("Cannot start the board server: {e}"))?;

    let port = server.port();
    let url = Url::parse(&format!("http://127.0.0.1:{port}/dashboard.html"))
        .map_err(|e| e.to_string())?;

    let window = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::External(url))
        .title("Jam Board")
        .inner_size(1200.0, 800.0)
        .on_navigation(move |url| match navigation::decide(url, port) {
            Navigation::Stay => true,
            Navigation::Browser => {
                navigation::open_in_browser(url);
                false
            }
            Navigation::Block => false,
        })
        .build()
        .map_err(|e| format!("Cannot open the board window: {e}"))?;
    ask_page_before_close(&window);

    app.manage(server);

    Ok(())
}

/// Closes the board window after the page agrees, or exits when the window is not open.
pub fn close_board_window(app: &AppHandle) {
    let closing = app
        .get_webview_window(MAIN)
        .is_some_and(|window| window.close().is_ok());
    if !closing {
        app.exit(0);
    }
}

/// Holds each close request of `window` and closes it when the page answers anything but `false`.
fn ask_page_before_close(window: &WebviewWindow) {
    let page = window.clone();

    window.on_window_event(move |event| {
        let WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        api.prevent_close();

        let target = page.clone();
        let asked = page.eval_with_callback(ASK_PAGE_TO_CLOSE, move |answer| {
            if may_close(&answer) {
                destroy(&target);
            }
        });
        if asked.is_err() {
            destroy(&page);
        }
    });
}

/// Returns true unless the page answered `false`. An empty answer comes from a page that failed to answer.
fn may_close(answer: &str) -> bool {
    answer != "false"
}

fn destroy(window: &WebviewWindow) {
    if let Err(e) = window.destroy() {
        eprintln!("jam-board: cannot close the board window: {e}");
    }
}

/// Calls `start` with `port`, then with port 0 when `port` is in use.
fn with_fallback_port<T>(port: u16, start: impl Fn(u16) -> io::Result<T>) -> io::Result<T> {
    match start(port) {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => start(0),
        result => result,
    }
}

/// Finds `jam-factory-mcp` in `{home}/.local/bin`, else in the first folder of `path_var` that has it.
pub fn find_server(home: &Path, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let local = home.join(".local").join("bin").join(SERVER_BINARY);
    if local.is_file() {
        return Some(local);
    }

    env::split_paths(path_var?)
        .map(|dir| dir.join(SERVER_BINARY))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::fs;
    use std::net::TcpListener;

    use tauri::utils::acl::RemoteUrlPattern;
    use tauri::utils::acl::capability::CapabilityFile;

    use super::*;

    fn binary_in(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let binary = dir.join(SERVER_BINARY);
        fs::write(&binary, "").unwrap();
        binary
    }

    fn path_var(dirs: &[&Path]) -> OsString {
        env::join_paths(dirs).unwrap()
    }

    #[test]
    fn prefers_local_bin() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let local = binary_in(&home.join(".local").join("bin"));
        let on_path = dir.path().join("bin");
        binary_in(&on_path);

        let found = find_server(&home, Some(&path_var(&[&on_path])));

        assert_eq!(found, Some(local));
    }

    #[test]
    fn falls_back_to_path() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        let first = dir.path().join("first");
        let expected = binary_in(&first);
        let second = dir.path().join("second");
        binary_in(&second);

        let found = find_server(&home, Some(&path_var(&[&empty, &first, &second])));

        assert_eq!(found, Some(expected));
    }

    #[test]
    fn none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        fs::create_dir_all(home.join(".local").join("bin").join(SERVER_BINARY)).unwrap();

        assert_eq!(find_server(&home, Some(&path_var(&[&empty]))), None);
        assert_eq!(find_server(&home, None), None);
    }

    #[test]
    fn uses_the_port_when_it_is_free() {
        let tried = RefCell::new(Vec::new());

        let port = with_fallback_port(DEFAULT_PORT, |port| {
            tried.borrow_mut().push(port);
            Ok(port)
        })
        .unwrap();

        assert_eq!(port, DEFAULT_PORT);
        assert_eq!(tried.into_inner(), vec![DEFAULT_PORT]);
    }

    #[test]
    fn falls_back_to_a_free_port_when_taken() {
        let taken = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let taken_port = taken.local_addr().unwrap().port();

        let listener =
            with_fallback_port(taken_port, |port| TcpListener::bind(("127.0.0.1", port))).unwrap();
        let port = listener.local_addr().unwrap().port();

        assert_ne!(port, taken_port);
        assert_ne!(port, 0);
    }

    #[test]
    fn other_errors_do_not_fall_back() {
        let tried = RefCell::new(Vec::new());

        let result: io::Result<()> = with_fallback_port(DEFAULT_PORT, |port| {
            tried.borrow_mut().push(port);
            Err(io::ErrorKind::PermissionDenied.into())
        });

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(tried.into_inner(), vec![DEFAULT_PORT]);
    }

    #[test]
    fn closes_unless_the_page_answers_false() {
        assert!(may_close("true"));
        assert!(may_close(""));
        assert!(!may_close("false"));
    }

    #[test]
    fn the_dashboard_answers_close_requests() {
        let dashboard = include_str!("../../core/dashboard/dashboard.html");

        assert!(dashboard.contains(&format!("function {ASK_PAGE_TO_CLOSE}")));
    }

    #[test]
    fn the_board_capability_covers_the_dashboard_on_any_port() {
        let file: CapabilityFile = include_str!("../capabilities/board.json").parse().unwrap();
        let CapabilityFile::Capability(capability) = file else {
            panic!("expected one capability");
        };
        assert_eq!(capability.windows, vec![MAIN]);

        let patterns: Vec<RemoteUrlPattern> = capability
            .remote
            .unwrap()
            .urls
            .iter()
            .map(|url| url.parse().unwrap())
            .collect();
        let allowed = |url: &str| patterns.iter().any(|p| p.test(&Url::parse(url).unwrap()));

        assert!(allowed("http://127.0.0.1:8731"));
        assert!(allowed("http://127.0.0.1:49152/dashboard.html#/o/r"));
        assert!(!allowed("http://example.org:8731/dashboard.html"));
        assert!(!allowed("https://127.0.0.1:8731/dashboard.html"));
    }
}
