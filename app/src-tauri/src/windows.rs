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

//! The board windows on the Jam Factory server.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tauri::{
    AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use jam_factory_store::root::resolve_port;

use crate::navigation::{self, Navigation};
use crate::probe;

const MAIN: &str = "main";

const EXTRA_PREFIX: &str = "board-";

const STANDALONE: &str = "standalone";

static NEXT_EXTRA: AtomicUsize = AtomicUsize::new(1);

const SERVER_DEADLINE: Duration = Duration::from_secs(3);

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

/// Waits for the server on the configured port and opens one window on its dashboard.
fn try_open_board_window(app: &AppHandle) -> Result<(), String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let port = resolve_port(&home).map_err(|e| e.to_string())?;

    if !probe::wait_for(port, SERVER_DEADLINE) {
        return Err(format!(
            "Cannot reach the Jam Factory server on port {port}. See ~/.jam-factory/logs/server.log, or run `just install` in app/."
        ));
    }

    let url = Url::parse(&format!("http://127.0.0.1:{port}/dashboard.html"))
        .map_err(|e| e.to_string())?;

    build_board_window(app, MAIN, url, port)
}

/// Opens a new board window on the page at `hash` of the dashboard that `window` shows.
#[tauri::command]
pub async fn open_window(window: WebviewWindow, hash: String) -> Result<(), String> {
    let url = standalone_url(window.url().map_err(|e| e.to_string())?, &hash);
    let port = url
        .port_or_known_default()
        .ok_or("The board window has no port")?;

    let label = format!(
        "{EXTRA_PREFIX}{}",
        NEXT_EXTRA.fetch_add(1, Ordering::Relaxed)
    );
    build_board_window(window.app_handle(), &label, url, port)
}

/// Returns `url` with the query `standalone` and the fragment `hash`.
fn standalone_url(mut url: Url, hash: &str) -> Url {
    url.set_query(Some(STANDALONE));
    url.set_fragment(Some(hash.trim_start_matches('#')));
    url
}

/// Returns true when the window labelled `label` shows on every Desktop.
fn on_all_workspaces(label: &str) -> bool {
    label == MAIN
}

/// Opens a window labelled `label` on `url`. It keeps the dashboard served on `port` and sends other links to the browser.
fn build_board_window(app: &AppHandle, label: &str, url: Url, port: u16) -> Result<(), String> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .title("Jam Board")
        .inner_size(1200.0, 800.0)
        .visible_on_all_workspaces(on_all_workspaces(label))
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

    Ok(())
}

/// Closes each board window after its page agrees, or exits when no window is open.
pub fn close_board_windows(app: &AppHandle) {
    let mut closing = false;
    for window in app.webview_windows().values() {
        closing |= window.close().is_ok();
    }

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

#[cfg(test)]
mod tests {
    use tauri::utils::acl::RemoteUrlPattern;
    use tauri::utils::acl::capability::CapabilityFile;

    use super::*;

    #[test]
    fn closes_unless_the_page_answers_false() {
        assert!(may_close("true"));
        assert!(may_close(""));
        assert!(!may_close("false"));
    }

    #[test]
    fn a_standalone_url_carries_the_query_and_the_hash() {
        let base = Url::parse("http://127.0.0.1:8731/dashboard.html?standalone#/old").unwrap();
        let url = standalone_url(base, "#/worktree/%2Fa%2Fb");
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8731/dashboard.html?standalone#/worktree/%2Fa%2Fb"
        );
    }

    #[test]
    fn only_the_main_window_shows_on_all_workspaces() {
        assert!(on_all_workspaces(MAIN));
        assert!(!on_all_workspaces("board-1"));
    }

    #[test]
    fn the_dashboard_invokes_the_open_window_command() {
        let dashboard = include_str!("../../core/dashboard/dashboard.html");

        assert!(dashboard.contains(".invoke('open_window', { hash })"));
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
        assert_eq!(
            capability.windows,
            vec![MAIN.to_string(), format!("{EXTRA_PREFIX}*")]
        );
        assert!(
            capability
                .permissions
                .iter()
                .any(|p| p.identifier().get() == "allow-open-window")
        );
        assert!(
            capability
                .permissions
                .iter()
                .any(|p| p.identifier().get() == "core:window:allow-set-title")
        );

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
