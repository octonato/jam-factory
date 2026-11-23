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

//! The GitHub views of each board window.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use serde::Deserialize;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewBuilder, WebviewUrl,
    Window, WindowEvent,
};

use jam_factory_store::root::resolve_sign_in_hosts;

use crate::navigation::{self, Navigation};
use crate::windows;

/// The path that each GitHub view last loaded, by view label.
static LOADED: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Mutex::default);

/// The labels of the GitHub views that show.
static SHOWN: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

/// The URL that each GitHub view loads while it stays hidden for a new path, by view label.
static PENDING: LazyLock<Mutex<HashMap<String, Url>>> = LazyLock::new(Mutex::default);

/// The GitHub views of each board window.
const VIEWS: [View; 2] = [View::Home, View::Worktree];

/// A GitHub view of a board window.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum View {
    /// The view of the board's main page.
    Home,
    /// The view of a worktree's GitHub item.
    Worktree,
}

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

/// Returns the label of the GitHub view `view` of the window labelled `window_label`.
fn view_label(window_label: &str, view: View) -> String {
    match view {
        View::Home => format!("{window_label}-github-home"),
        View::Worktree => format!("{window_label}-github"),
    }
}

/// Returns the GitHub view of a window that is not `view`.
fn other(view: View) -> View {
    match view {
        View::Home => View::Worktree,
        View::Worktree => View::Home,
    }
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

/// Shows the GitHub view `view` of the calling window at these logical bounds of the board page's viewport and at `zoom`, hides its other view, and loads `url` when the view is new or last loaded another path.
/// `viewport_height` is the height of the board page's viewport.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn github_view_show(
    webview: Webview,
    view: View,
    path: String,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_height: f64,
    zoom: f64,
) -> Result<(), String> {
    let window = webview.window();
    let url = parse(&url)?;
    let origin = viewport_origin(&window, &webview, viewport_height)?;
    let position = LogicalPosition::new(origin.x + x, origin.y + y);
    let size = LogicalSize::new(width, height);

    hide(&window, other(view))?;

    let label = view_label(window.label(), view);
    shown().insert(label.clone());

    match window.get_webview(&label) {
        Some(github) => {
            let page = worktree_page(&label, &path);

            github
                .set_position(position)
                .and_then(|_| github.set_size(size))
                .and_then(|_| github.set_zoom(zoom))
                .map_err(|e| format!("Cannot show the GitHub view: {e}"))?;

            match page {
                Page::Ready => github
                    .show()
                    .map_err(|e| format!("Cannot show the GitHub view: {e}"))?,
                Page::Pending => {}
                Page::Stale => load_hidden(&label, &github, url)?,
            }
        }
        None => {
            let github = add_github_view(&window, &label, url, position, size)?;

            // The first GitHub view of the window registers the cleanup of both views.
            if github_view(&window, other(view)).is_none() {
                forget_on_destroy(&window);
            }

            github
                .set_zoom(zoom)
                .map_err(|e| format!("Cannot zoom the GitHub view: {e}"))?;
        }
    }

    loaded().insert(label, path);

    Ok(())
}

/// Hides the GitHub view `view` of the calling window, if it has one.
#[tauri::command]
pub async fn github_view_hide(webview: Webview, view: View) -> Result<(), String> {
    hide(&webview.window(), view)
}

/// Loads `url`, a page of `https://github.com/`, in the GitHub view `view` of the calling window.
#[tauri::command]
pub async fn github_view_navigate(webview: Webview, view: View, url: String) -> Result<(), String> {
    let url = parse(&url)?;
    if !is_github_page(&url) {
        return Err(format!(
            "Cannot load {url} in the GitHub view: it is not a page of https://github.com/"
        ));
    }

    let Some(github) = github_view(&webview.window(), view) else {
        return Err("Cannot load the GitHub page: the GitHub view is not open".to_string());
    };

    navigate(&github, url)
}

/// Reloads the GitHub view that shows in the focused board window.
pub fn reload_focused(app: &AppHandle) {
    let Some(view) = windows::focused_board_window(app).and_then(|window| shown_view(&window))
    else {
        return;
    };

    if let Err(e) = reload(&view) {
        eprintln!("jam-board: {e}");
    }
}

/// Returns the URL of the GitHub view that shows in `window`: its pending page, or else its current page.
pub fn shown_url(window: &Window) -> Option<Url> {
    let view = shown_view(window)?;

    pending_url(view.label()).or_else(|| view.url().ok())
}

/// Returns the GitHub view that shows in `window`.
fn shown_view(window: &Window) -> Option<Webview> {
    let label = VIEWS
        .map(|view| view_label(window.label(), view))
        .into_iter()
        .find(|label| shown().contains(label))?;

    window.get_webview(&label)
}

/// Hides the GitHub view `view` of `window`, if it has one.
fn hide(window: &Window, view: View) -> Result<(), String> {
    shown().remove(&view_label(window.label(), view));

    let Some(github) = github_view(window, view) else {
        return Ok(());
    };

    github
        .hide()
        .map_err(|e| format!("Cannot hide the GitHub view: {e}"))
}

/// Returns the logical position in `window` of the top left corner of the viewport of `board`, whose page reports a viewport of height `viewport_height`.
fn viewport_origin(
    window: &Window,
    board: &Webview,
    viewport_height: f64,
) -> Result<LogicalPosition<f64>, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let bounds = board
        .bounds()
        .map_err(|e| format!("Cannot read the board page bounds: {e}"))?;

    let position = bounds.position.to_logical::<f64>(scale);
    let size = bounds.size.to_logical::<f64>(scale);

    Ok(LogicalPosition::new(
        position.x,
        position.y + top_inset(size.height, viewport_height),
    ))
}

/// Returns the space above a viewport of height `viewport_height` that ends at the bottom of a web view of height `frame_height`.
fn top_inset(frame_height: f64, viewport_height: f64) -> f64 {
    (frame_height - viewport_height).max(0.0)
}

/// Returns the GitHub view `view` of `window`.
fn github_view(window: &Window, view: View) -> Option<Webview> {
    window.get_webview(&view_label(window.label(), view))
}

/// Returns the state of the page of `path` in the GitHub view labelled `label`.
fn worktree_page(label: &str, path: &str) -> Page {
    let loaded = loaded().get(label).cloned();
    let pending = pending().contains_key(label);

    page_state(loaded.as_deref(), path, pending)
}

/// Hides `view`, the GitHub view labelled `label`, until the page of `url` commits, and loads `url` in it.
fn load_hidden(label: &str, view: &Webview, url: Url) -> Result<(), String> {
    view.hide()
        .map_err(|e| format!("Cannot hide the GitHub view: {e}"))?;
    pending().insert(label.to_string(), url.clone());

    navigate(view, url)
}

/// Shows `view`, the GitHub view labelled `label`, when its pending page commits and the board asked it to show.
fn show_on_commit(view: &Webview, label: &str) {
    let committed = pending().remove(label).is_some();
    if !committed || !shown().contains(label) {
        return;
    }

    if let Err(e) = view.show() {
        eprintln!("jam-board: Cannot show the GitHub view: {e}");
    }
}

/// Returns the URL that the GitHub view labelled `label` loads while it stays hidden for a new path.
fn pending_url(label: &str) -> Option<Url> {
    pending().get(label).cloned()
}

/// Adds the GitHub view labelled `label` on `url` to `window`. It keeps GitHub and the configured sign-in hosts and sends other links to the browser.
fn add_github_view(
    window: &Window,
    label: &str,
    url: Url,
    position: LogicalPosition<f64>,
    size: LogicalSize<f64>,
) -> Result<Webview, String> {
    let home = window.path().home_dir().map_err(|e| e.to_string())?;
    let sign_in_hosts = resolve_sign_in_hosts(&home).map_err(|e| e.to_string())?;
    let new_window_hosts = sign_in_hosts.clone();

    let app = window.app_handle().clone();
    let opener = label.to_string();

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
                show_on_commit(&view, view.label());
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

/// Forgets the GitHub views of `window` once the window is destroyed.
fn forget_on_destroy(window: &Window) {
    let labels = VIEWS.map(|view| view_label(window.label(), view));

    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            for label in &labels {
                loaded().remove(label);
                shown().remove(label);
                pending().remove(label);
            }
        }
    });
}

fn parse(url: &str) -> Result<Url, String> {
    Url::parse(url).map_err(|e| format!("Cannot load {url} in the GitHub view: {e}"))
}

/// Returns true when `url` is an `https` page of the host `github.com` on its default port.
fn is_github_page(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some("github.com") && url.port().is_none()
}

fn navigate(view: &Webview, url: Url) -> Result<(), String> {
    view.navigate(url)
        .map_err(|e| format!("Cannot load the GitHub page: {e}"))
}

/// Reloads `view`, or loads its pending page again.
fn reload(view: &Webview) -> Result<(), String> {
    if let Some(url) = pending_url(view.label()) {
        return navigate(view, url);
    }

    view.reload()
        .map_err(|e| format!("Cannot reload the GitHub view: {e}"))
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
    fn each_view_has_its_own_label() {
        assert_eq!(view_label("main", View::Worktree), "main-github");
        assert_eq!(view_label("board-3", View::Worktree), "board-3-github");
        assert_eq!(view_label("main", View::Home), "main-github-home");
        assert_ne!(
            view_label("main", View::Home),
            view_label("main", View::Worktree)
        );
    }

    #[test]
    fn each_view_has_an_other() {
        assert_eq!(other(View::Home), View::Worktree);
        assert_eq!(other(View::Worktree), View::Home);
    }

    #[test]
    fn the_view_names_come_from_the_board() {
        assert_eq!(
            serde_json::from_str::<View>("\"home\"").unwrap(),
            View::Home
        );
        assert_eq!(
            serde_json::from_str::<View>("\"worktree\"").unwrap(),
            View::Worktree
        );
        assert!(serde_json::from_str::<View>("\"other\"").is_err());
    }

    #[test]
    fn only_github_pages_are_navigated() {
        let is_github = |url: &str| is_github_page(&Url::parse(url).unwrap());

        for url in [
            "https://github.com/",
            "https://github.com/lightbend/akka-runtime",
            "https://github.com:443/o/r",
            "https://GitHub.com/o/r",
        ] {
            assert!(is_github(url), "{url}");
        }

        for url in [
            "http://github.com/o/r",
            "https://gist.github.com/o/r",
            "https://github.com.example.org/o/r",
            "https://github.com:8443/o/r",
            "file:///github.com/o/r",
            "javascript:alert(1)",
        ] {
            assert!(!is_github(url), "{url}");
        }
    }

    #[test]
    fn the_viewport_starts_below_the_inset_of_the_board_page() {
        assert_eq!(top_inset(800.0, 800.0), 0.0);
        assert_eq!(top_inset(800.0, 772.0), 28.0);
        assert_eq!(top_inset(800.0, 801.0), 0.0);
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
