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

//! The GitHub view of each board window.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewBuilder, WebviewUrl,
    Window, WindowEvent,
};

use jam_factory_store::root::resolve_sign_in_hosts;

use crate::navigation::{self, Navigation};
use crate::windows;

const GO_BACK: &str = "history.back()";

const NO_VIEW: &str = "The board window has no GitHub view";

/// The worktree path that each GitHub view last loaded, by window label.
static LOADED: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Mutex::default);

/// The labels of the windows whose GitHub view shows.
static SHOWN: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

/// The URL that each GitHub view loads while it stays hidden for a new worktree, by window label.
static PENDING: LazyLock<Mutex<HashMap<String, Url>>> = LazyLock::new(Mutex::default);

/// The state of a GitHub view's page for a worktree.
#[derive(Debug, PartialEq)]
enum Page {
    /// The view holds a committed page of the worktree.
    Ready,
    /// The view stays hidden until its page of the worktree commits.
    Pending,
    /// The view holds no page of the worktree.
    Stale,
}

/// Returns the label of the GitHub view of the window labelled `window_label`.
fn github_label(window_label: &str) -> String {
    format!("{window_label}-github")
}

/// Returns true when a GitHub view that last loaded the worktree `loaded` must load the URL of `path`.
fn needs_load(loaded: Option<&str>, path: &str) -> bool {
    loaded != Some(path)
}

/// Returns the state of the page of `path` in a GitHub view that last loaded `loaded` and has a pending page when `pending`.
fn page_state(loaded: Option<&str>, path: &str, pending: bool) -> Page {
    if needs_load(loaded, path) {
        Page::Stale
    } else if pending {
        Page::Pending
    } else {
        Page::Ready
    }
}

/// Shows the GitHub view of the calling window at these logical bounds, and loads `url` when the view is new or last loaded another path.
#[tauri::command]
pub async fn github_view_show(
    webview: Webview,
    path: String,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window = webview.window();
    let url = parse(&url)?;
    let position = LogicalPosition::new(x, y);
    let size = LogicalSize::new(width, height);

    shown().insert(window.label().to_string());

    match github_view(&window) {
        Some(view) => {
            let page = worktree_page(&window, &path);

            view.set_position(position)
                .and_then(|_| view.set_size(size))
                .map_err(|e| format!("Cannot show the GitHub view: {e}"))?;

            match page {
                Page::Ready => view
                    .show()
                    .map_err(|e| format!("Cannot show the GitHub view: {e}"))?,
                Page::Pending => {}
                Page::Stale => load_hidden(&window, &view, url)?,
            }
        }
        None => {
            add_github_view(&window, url, position, size)?;
            forget_on_destroy(&window);
        }
    }

    loaded().insert(window.label().to_string(), path);

    Ok(())
}

/// Loads `url` in the GitHub view of the calling window and records `path` as its worktree.
#[tauri::command]
pub async fn github_view_load(webview: Webview, path: String, url: String) -> Result<(), String> {
    let window = webview.window();
    let url = parse(&url)?;
    let view = github_view(&window).ok_or(NO_VIEW)?;

    match worktree_page(&window, &path) {
        Page::Ready => navigate(&view, url)?,
        Page::Pending | Page::Stale => load_hidden(&window, &view, url)?,
    }
    loaded().insert(window.label().to_string(), path);

    Ok(())
}

/// Hides the GitHub view of the calling window, if it has one.
#[tauri::command]
pub async fn github_view_hide(webview: Webview) -> Result<(), String> {
    let window = webview.window();
    shown().remove(window.label());

    let Some(view) = github_view(&window) else {
        return Ok(());
    };

    view.hide()
        .map_err(|e| format!("Cannot hide the GitHub view: {e}"))
}

/// Runs `action`, one of `back`, `reload` and `browser`, on the GitHub view of the calling window, if it has one.
#[tauri::command]
pub async fn github_view_action(webview: Webview, action: String) -> Result<(), String> {
    let run: fn(&Webview) -> Result<(), String> = match action.as_str() {
        "back" => go_back,
        "reload" => reload,
        "browser" => open_page_in_browser,
        _ => return Err(format!("Unknown GitHub view action: {action}")),
    };

    match github_view(&webview.window()) {
        Some(view) => run(&view),
        None => Ok(()),
    }
}

/// Reloads the GitHub view of the focused board window when it shows.
pub fn reload_focused(app: &AppHandle) {
    let Some(window) = focused_board_window(app) else {
        return;
    };
    if !shown().contains(window.label()) {
        return;
    }

    if let Some(view) = github_view(&window)
        && let Err(e) = reload(&view)
    {
        eprintln!("jam-board: {e}");
    }
}

/// Returns the focused window when it holds a board web view.
fn focused_board_window(app: &AppHandle) -> Option<Window> {
    app.get_focused_window()
        .filter(|window| windows::board_webview(window).is_some())
}

/// Returns the GitHub view of `window`.
fn github_view(window: &Window) -> Option<Webview> {
    window.get_webview(&github_label(window.label()))
}

/// Returns the state of the page of `path` in the GitHub view of `window`.
fn worktree_page(window: &Window, path: &str) -> Page {
    let loaded = loaded().get(window.label()).cloned();
    let pending = pending().contains_key(window.label());

    page_state(loaded.as_deref(), path, pending)
}

/// Hides `view` until the page of `url` commits, and loads `url` in it.
fn load_hidden(window: &Window, view: &Webview, url: Url) -> Result<(), String> {
    view.hide()
        .map_err(|e| format!("Cannot hide the GitHub view: {e}"))?;
    pending().insert(window.label().to_string(), url.clone());

    navigate(view, url)
}

/// Shows `view`, the GitHub view of the window labelled `window_label`, when its pending page commits and the board asked it to show.
fn show_on_commit(view: &Webview, window_label: &str) {
    let committed = pending().remove(window_label).is_some();
    if !committed || !shown().contains(window_label) {
        return;
    }

    if let Err(e) = view.show() {
        eprintln!("jam-board: Cannot show the GitHub view: {e}");
    }
}

/// Returns the URL that `view` loads while it stays hidden for a new worktree.
fn pending_url(view: &Webview) -> Option<Url> {
    let window = view.window();
    pending().get(window.label()).cloned()
}

/// Adds the GitHub view on `url` to `window`. It keeps GitHub and the configured sign-in hosts and sends other links to the browser.
fn add_github_view(
    window: &Window,
    url: Url,
    position: LogicalPosition<f64>,
    size: LogicalSize<f64>,
) -> Result<Webview, String> {
    let home = window.path().home_dir().map_err(|e| e.to_string())?;
    let sign_in_hosts = resolve_sign_in_hosts(&home).map_err(|e| e.to_string())?;
    let new_window_hosts = sign_in_hosts.clone();

    let label = github_label(window.label());
    let app = window.app_handle().clone();
    let opener = label.clone();
    let window_label = window.label().to_string();

    let view = WebviewBuilder::new(label, WebviewUrl::External(url))
        .on_navigation(
            move |url| match navigation::decide_github(url, &sign_in_hosts) {
                Navigation::Stay => true,
                Navigation::Browser => {
                    navigation::open_in_browser(url);
                    false
                }
                Navigation::Block => false,
            },
        )
        .on_new_window(move |url, _| {
            open_new_window(&app, &opener, url, &new_window_hosts);
            NewWindowResponse::Deny
        })
        .on_page_load(move |view, payload| {
            if matches!(payload.event(), PageLoadEvent::Started) {
                show_on_commit(&view, &window_label);
            }
        });

    window
        .add_child(view, position, size)
        .map_err(|e| format!("Cannot open the GitHub view: {e}"))
}

/// Loads `url` in the GitHub view labelled `label` when it stays there, or sends it to the browser.
fn open_new_window(app: &AppHandle, label: &str, url: Url, sign_in_hosts: &[String]) {
    match navigation::decide_github(&url, sign_in_hosts) {
        Navigation::Stay => {
            if let Some(view) = app.get_webview(label)
                && let Err(e) = navigate(&view, url)
            {
                eprintln!("jam-board: {e}");
            }
        }
        Navigation::Browser => navigation::open_in_browser(&url),
        Navigation::Block => {}
    }
}

/// Forgets the GitHub view of `window` once the window is destroyed.
fn forget_on_destroy(window: &Window) {
    let label = window.label().to_string();

    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            loaded().remove(&label);
            shown().remove(&label);
            pending().remove(&label);
        }
    });
}

fn parse(url: &str) -> Result<Url, String> {
    Url::parse(url).map_err(|e| format!("Cannot load {url} in the GitHub view: {e}"))
}

fn navigate(view: &Webview, url: Url) -> Result<(), String> {
    view.navigate(url)
        .map_err(|e| format!("Cannot load the GitHub page: {e}"))
}

/// Runs `history.back()` in `view`, unless it has a pending page.
fn go_back(view: &Webview) -> Result<(), String> {
    if pending_url(view).is_some() {
        return Ok(());
    }

    view.eval(GO_BACK)
        .map_err(|e| format!("Cannot go back in the GitHub view: {e}"))
}

/// Reloads `view`, or loads its pending page again.
fn reload(view: &Webview) -> Result<(), String> {
    if let Some(url) = pending_url(view) {
        return navigate(view, url);
    }

    view.reload()
        .map_err(|e| format!("Cannot reload the GitHub view: {e}"))
}

/// Opens the pending page of `view`, or the page it shows, in the default browser.
fn open_page_in_browser(view: &Webview) -> Result<(), String> {
    let url = match pending_url(view) {
        Some(url) => url,
        None => view
            .url()
            .map_err(|e| format!("Cannot read the GitHub view's page: {e}"))?,
    };

    navigation::open_in_browser(&url);
    Ok(())
}

fn loaded() -> MutexGuard<'static, HashMap<String, String>> {
    LOADED.lock().unwrap_or_else(PoisonError::into_inner)
}

fn shown() -> MutexGuard<'static, HashSet<String>> {
    SHOWN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pending() -> MutexGuard<'static, HashMap<String, Url>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_github_view_label_follows_the_window() {
        assert_eq!(github_label("main"), "main-github");
        assert_eq!(github_label("board-3"), "board-3-github");
    }

    #[test]
    fn a_show_for_another_path_loads_its_url() {
        assert!(needs_load(None, "/a/main"));
        assert!(!needs_load(Some("/a/main"), "/a/main"));
        assert!(needs_load(Some("/a/main"), "/a/feature x+y"));
    }

    #[test]
    fn a_view_stays_hidden_until_the_page_of_a_new_worktree_commits() {
        assert_eq!(page_state(Some("/a/main"), "/a/main", false), Page::Ready);
        assert_eq!(page_state(Some("/a/main"), "/a/main", true), Page::Pending);
        assert_eq!(
            page_state(Some("/a/main"), "/a/feature x+y", false),
            Page::Stale
        );
        assert_eq!(
            page_state(Some("/a/main"), "/a/feature x+y", true),
            Page::Stale
        );
        assert_eq!(page_state(None, "/a/main", false), Page::Stale);
    }
}
