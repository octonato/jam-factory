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

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tauri::{
    AppHandle, LogicalPosition, Manager, Url, Webview, WebviewBuilder, WebviewUrl, Window,
    WindowEvent,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use jam_factory_store::root::resolve_port;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::navigation::{self, Navigation};
use crate::probe;

const MAIN: &str = "main";

const EXTRA_PREFIX: &str = "board-";

const STANDALONE: &str = "standalone";

static NEXT_EXTRA: AtomicUsize = AtomicUsize::new(1);

/// The root hash of each open standalone window, without the leading `#`, by label.
static ROOTS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Mutex::default);

const SERVER_DEADLINE: Duration = Duration::from_secs(3);

const ASK_PAGE_TO_CLOSE: &str = "onCloseRequested()";

const LINK_SCHEME: &str = "jam-board";

const LINK_HOST: &str = "open";

/// The bytes that `encodeURIComponent` encodes.
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

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

    build_board_window(app, MAIN, url, port)?;

    Ok(())
}

/// Focuses the main board window, or reopens it on the server that `webview` shows.
#[tauri::command]
pub async fn open_main_window(webview: Webview) -> Result<(), String> {
    let app = webview.app_handle();
    if let Some(main) = app.get_window(MAIN) {
        return focus(&main);
    }

    let url = main_url(webview.url().map_err(|e| e.to_string())?);
    let port = url
        .port_or_known_default()
        .ok_or("The board window has no port")?;
    let main = build_board_window(app, MAIN, url, port)?;
    focus(&main)
}

/// Returns `url` without the standalone query or the current page's fragment.
fn main_url(mut url: Url) -> Url {
    url.set_query(None);
    url.set_fragment(None);
    url
}

/// Focuses the standalone window rooted at `hash`, or opens one on the dashboard that `webview` shows.
#[tauri::command]
pub async fn open_window(webview: Webview, hash: String) -> Result<(), String> {
    let dashboard = webview.url().map_err(|e| e.to_string())?;

    open_standalone(webview.app_handle(), dashboard, &hash)
}

/// Opens or focuses the standalone window of the worktree that each `jam-board://open?path=…` link in `urls` names.
pub fn open_links(app: &AppHandle, urls: &[Url]) {
    for url in urls {
        if let Err(message) = open_link(app, url) {
            app.dialog()
                .message(message)
                .kind(MessageDialogKind::Error)
                .show(|_| {});
        }
    }
}

/// Opens or focuses the standalone window of the worktree that `url` names, on the dashboard of the configured port.
fn open_link(app: &AppHandle, url: &Url) -> Result<(), String> {
    let path = linked_worktree(url).ok_or_else(|| format!("Cannot open the link {url}"))?;

    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let port = resolve_port(&home).map_err(|e| e.to_string())?;
    let dashboard = Url::parse(&format!("http://127.0.0.1:{port}/dashboard.html"))
        .map_err(|e| e.to_string())?;

    open_standalone(app, dashboard, &worktree_hash(&path))
}

/// Returns the absolute worktree path of a `jam-board://open?path=…` link.
fn linked_worktree(url: &Url) -> Option<String> {
    if url.scheme() != LINK_SCHEME || url.host_str() != Some(LINK_HOST) {
        return None;
    }

    url.query_pairs()
        .find(|(key, _)| key == "path")
        .map(|(_, path)| path.into_owned())
        .filter(|path| path.starts_with('/'))
}

/// Returns the dashboard hash of the worktree page of `path`, encoded as `encodeURIComponent` does.
fn worktree_hash(path: &str) -> String {
    format!("#/worktree/{}", utf8_percent_encode(path, URI_COMPONENT))
}

/// Focuses the standalone window rooted at `hash`, or opens one on `dashboard`.
fn open_standalone(app: &AppHandle, dashboard: Url, hash: &str) -> Result<(), String> {
    let known = window_with_root(&roots(), hash);
    if let Some(label) = known {
        if let Some(open) = app.get_window(&label) {
            return focus(&open);
        }
        forget_window(&mut roots(), &label);
    }

    let url = standalone_url(dashboard, hash);
    let port = url
        .port_or_known_default()
        .ok_or("The board window has no port")?;

    let label = format!(
        "{EXTRA_PREFIX}{}",
        NEXT_EXTRA.fetch_add(1, Ordering::Relaxed)
    );
    let opened = build_board_window(app, &label, url, port)?;

    roots().insert(label, root_of(hash).to_string());
    forget_on_destroy(&opened);

    Ok(())
}

/// Returns `url` with the query `standalone` and the fragment `hash`.
fn standalone_url(mut url: Url, hash: &str) -> Url {
    url.set_query(Some(STANDALONE));
    url.set_fragment(Some(root_of(hash)));
    url
}

/// Returns `hash` without its leading `#`.
fn root_of(hash: &str) -> &str {
    hash.trim_start_matches('#')
}

fn roots() -> MutexGuard<'static, HashMap<String, String>> {
    ROOTS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Returns the label of the window in `roots` rooted at `hash`.
fn window_with_root(roots: &HashMap<String, String>, hash: &str) -> Option<String> {
    let root = root_of(hash);

    roots
        .iter()
        .find(|(_, known)| known.as_str() == root)
        .map(|(label, _)| label.clone())
}

/// Removes the window labelled `label` from `roots`.
fn forget_window(roots: &mut HashMap<String, String>, label: &str) {
    roots.remove(label);
}

/// Removes `window` from the roots once it is destroyed.
fn forget_on_destroy(window: &Window) {
    let label = window.label().to_string();

    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            forget_window(&mut roots(), &label);
        }
    });
}

/// Unminimizes, shows and focuses `window`.
fn focus(window: &Window) -> Result<(), String> {
    let focused = window
        .unminimize()
        .and_then(|_| window.show())
        .and_then(|_| window.set_focus());

    focused.map_err(|e| format!("Cannot focus the board window: {e}"))
}

/// Returns true when the window labelled `label` shows on every Desktop.
fn on_all_workspaces(label: &str) -> bool {
    label == MAIN
}

/// Opens a window labelled `label` on `url`. Its board web view keeps the dashboard served on `port` and sends other links to the browser.
fn build_board_window(app: &AppHandle, label: &str, url: Url, port: u16) -> Result<Window, String> {
    let window = Window::builder(app, label)
        .title("Jam Board")
        .inner_size(1200.0, 800.0)
        .visible_on_all_workspaces(on_all_workspaces(label))
        .build()
        .map_err(|e| format!("Cannot open the board window: {e}"))?;

    let page = add_board_webview(&window, url, port).inspect_err(|_| destroy(&window))?;
    if let Err(e) = page.set_focus() {
        eprintln!("jam-board: cannot focus the board page: {e}");
    }
    ask_page_before_close(&window, &page);

    Ok(window)
}

/// Adds the board web view on `url` over the whole of `window`, labelled as `window`.
fn add_board_webview(window: &Window, url: Url, port: u16) -> Result<Webview, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);

    let page = WebviewBuilder::new(window.label(), WebviewUrl::External(url))
        .auto_resize()
        .on_navigation(move |url| match navigation::decide(url, port) {
            Navigation::Stay => true,
            Navigation::Browser => {
                navigation::open_in_browser(url);
                false
            }
            Navigation::Block => false,
        });

    window
        .add_child(page, LogicalPosition::new(0., 0.), size)
        .map_err(|e| format!("Cannot open the board window: {e}"))
}

/// Returns the board web view of `window`.
pub fn board_webview(window: &Window) -> Option<Webview> {
    window
        .webviews()
        .into_iter()
        .find(|webview| is_board_webview(window.label(), webview.label()))
}

/// Returns true when the web view labelled `webview_label` is the board web view of the window labelled `window_label`.
fn is_board_webview(window_label: &str, webview_label: &str) -> bool {
    webview_label == window_label
}

/// Closes each board window after its page agrees, or exits when no window is open.
pub fn close_board_windows(app: &AppHandle) {
    let mut closing = false;
    for window in app.windows().values() {
        closing |= window.close().is_ok();
    }

    if !closing {
        app.exit(0);
    }
}

/// Holds each close request of `window` and closes it when `page` answers anything but `false`.
fn ask_page_before_close(window: &Window, page: &Webview) {
    let closing = window.clone();
    let page = page.clone();

    window.on_window_event(move |event| {
        let WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        api.prevent_close();

        let target = closing.clone();
        let asked = page.eval_with_callback(ASK_PAGE_TO_CLOSE, move |answer| {
            if may_close(&answer) {
                destroy(&target);
            }
        });
        if asked.is_err() {
            destroy(&closing);
        }
    });
}

/// Returns true unless the page answered `false`. An empty answer comes from a page that failed to answer.
fn may_close(answer: &str) -> bool {
    answer != "false"
}

fn destroy(window: &Window) {
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
    fn the_main_url_keeps_the_server_and_clears_the_standalone_route() {
        for port in [8731, 49152] {
            let base = Url::parse(&format!(
                "http://127.0.0.1:{port}/dashboard.html?standalone#/worktree/%2Fa%2Fb"
            ))
            .unwrap();
            assert_eq!(
                main_url(base).as_str(),
                format!("http://127.0.0.1:{port}/dashboard.html")
            );
        }
    }

    #[test]
    fn a_link_names_an_absolute_worktree_path() {
        let path = |link: &str| linked_worktree(&Url::parse(link).unwrap());

        assert_eq!(
            path("jam-board://open?path=%2Fa%2Ffeature+x%2By"),
            Some("/a/feature x+y".to_string())
        );
        assert_eq!(path("jam-board://open?path=rel"), None);
        assert_eq!(path("jam-board://open"), None);
        assert_eq!(path("jam-board://close?path=%2Fa"), None);
        assert_eq!(path("https://open?path=%2Fa"), None);
    }

    #[test]
    fn the_worktree_hash_encodes_as_the_dashboard_does() {
        assert_eq!(
            worktree_hash("/a/feature x+y/(1)!~*'_-."),
            "#/worktree/%2Fa%2Ffeature%20x%2By%2F(1)!~*'_-."
        );
    }

    #[test]
    fn the_dashboard_builds_worktree_hashes_with_encode_uri_component() {
        let dashboard = include_str!("../../core/dashboard/dashboard.html");

        assert!(dashboard.contains("return `#/worktree/${encodeURIComponent(path)}`"));
    }

    #[test]
    fn the_app_registers_the_link_scheme() {
        let plist = include_str!("../Info.plist");

        assert!(plist.contains(&format!("<string>{LINK_SCHEME}</string>")));
    }

    #[test]
    fn finds_the_window_with_the_same_root() {
        let roots = HashMap::from([
            ("board-1".to_string(), "/worktree/%2Fa".to_string()),
            ("board-2".to_string(), "/prs/%2Fm".to_string()),
        ]);
        assert_eq!(
            window_with_root(&roots, "#/prs/%2Fm"),
            Some("board-2".to_string())
        );
        assert_eq!(
            window_with_root(&roots, "/worktree/%2Fa"),
            Some("board-1".to_string())
        );
        assert_eq!(window_with_root(&roots, "#/worktree/%2Fb"), None);
    }

    #[test]
    fn a_forgotten_window_is_not_found() {
        let mut roots = HashMap::from([("board-1".to_string(), "/worktree/%2Fa".to_string())]);
        forget_window(&mut roots, "board-1");
        assert_eq!(window_with_root(&roots, "#/worktree/%2Fa"), None);
    }

    #[test]
    fn only_the_main_window_shows_on_all_workspaces() {
        assert!(on_all_workspaces(MAIN));
        assert!(!on_all_workspaces("board-1"));
    }

    #[test]
    fn the_board_webview_shares_the_window_label() {
        assert!(is_board_webview("main", "main"));
        assert!(!is_board_webview("main", "main-github"));
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
                .any(|p| p.identifier().get() == "allow-open-main-window")
        );
        assert!(
            capability
                .permissions
                .iter()
                .any(|p| p.identifier().get() == "core:window:allow-set-title")
        );
        for github_view in ["allow-github-view-show", "allow-github-view-hide"] {
            assert!(
                capability
                    .permissions
                    .iter()
                    .any(|p| p.identifier().get() == github_view),
                "{github_view}"
            );
        }

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
        assert!(!allowed("https://github.com/o/r"));
    }
}
