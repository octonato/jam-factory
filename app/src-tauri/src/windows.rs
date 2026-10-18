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

//! Project windows, the welcome window and the app menu.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::menu::{Menu, MenuItem, MenuItemKind};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use odeck_core::factory::{self, FactoryError};
use odeck_core::server::Server;

use crate::navigation::{self, Navigation};
use crate::recent_store::{RecentStore, display_path};
use crate::session_store::ProjectStore;

pub const OPEN_PROJECT: &str = "open-project";

const WELCOME: &str = "welcome";

/// Maps each project window label to its project path and its server.
#[derive(Default)]
pub struct Windows {
    projects: Mutex<HashMap<String, (PathBuf, Server)>>,
    next: AtomicU64,
}

impl Windows {
    fn label_for(&self, path: &Path) -> Option<String> {
        self.projects
            .lock()
            .unwrap()
            .iter()
            .find(|(_, (p, _))| p.as_path() == path)
            .map(|(label, _)| label.clone())
    }

    fn insert(&self, path: &Path, server: Server) -> String {
        let label = format!("project-{}", self.next.fetch_add(1, Ordering::Relaxed));
        self.projects
            .lock()
            .unwrap()
            .insert(label.clone(), (path.to_path_buf(), server));
        label
    }

    /// Forgets the window `label` and stops its server.
    fn remove(&self, label: &str) -> Option<PathBuf> {
        let (path, _server) = self.projects.lock().unwrap().remove(label)?;
        Some(path)
    }

    fn is_empty(&self) -> bool {
        self.projects.lock().unwrap().is_empty()
    }
}

/// A failure to open a project window.
#[derive(Debug)]
pub enum OpenError {
    /// The picked folder is not a usable project.
    Factory(FactoryError),
    /// The project's server cannot start.
    Server(String),
    /// The project window cannot be built.
    Window(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Factory(e) => write!(f, "{e}"),
            OpenError::Server(message) | OpenError::Window(message) => f.write_str(message),
        }
    }
}

/// Opens a window for the project at `picked`, or focuses the one already open.
pub fn open_project_window(app: &AppHandle, picked: PathBuf) -> Result<(), OpenError> {
    let factory = factory::open(&picked).map_err(OpenError::Factory)?;

    let windows = app.state::<Windows>();
    if let Some(window) = windows
        .label_for(&factory.project)
        .and_then(|label| app.get_webview_window(&label))
    {
        let _ = window.set_focus();
        app.state::<RecentStore>().touch(&factory.project);
        return Ok(());
    }

    let server = Server::start(factory.root.clone(), factory.port).map_err(|e| {
        OpenError::Server(format!(
            "Cannot serve {} on port {}: {}. Change the port in {}.",
            factory.root.display(),
            factory.port,
            e,
            factory.config_file().display()
        ))
    })?;

    let port = server.port();
    let url = Url::parse(&format!("http://127.0.0.1:{port}/dashboard.html"))
        .map_err(|e| OpenError::Window(e.to_string()))?;
    let label = windows.insert(&factory.project, server);

    let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(format!("Jam Board — {}", factory.name()))
        .inner_size(1200.0, 800.0)
        .on_navigation(move |url| match navigation::decide(url, port) {
            Navigation::Stay => true,
            Navigation::Browser => {
                navigation::open_in_browser(url);
                false
            }
            Navigation::Block => false,
        })
        .build();
    if let Err(e) = built {
        windows.remove(&label);
        return Err(OpenError::Window(e.to_string()));
    }

    app.state::<ProjectStore>().add(&factory.project);
    app.state::<RecentStore>().touch(&factory.project);
    if let Some(welcome) = app.get_webview_window(WELCOME) {
        let _ = welcome.close();
    }

    Ok(())
}

/// Opens the welcome window, or focuses it when it is already open.
pub fn open_welcome_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WELCOME) {
        let _ = window.set_focus();
        return;
    }

    let built = WebviewWindowBuilder::new(app, WELCOME, WebviewUrl::App("index.html".into()))
        .title("octodeck")
        .inner_size(600.0, 560.0)
        .build();
    if let Err(e) = built {
        eprintln!("octodeck: cannot open the welcome window: {e}");
    }
}

/// Shows a folder picker and opens the picked project.
pub fn pick_project(app: &AppHandle) {
    let handle = app.clone();
    app.dialog().file().pick_folder(move |folder| {
        let Some(path) = folder.and_then(|f| f.into_path().ok()) else {
            return;
        };

        if let Err(e) = open_project_window(&handle, path) {
            show_error(&handle, e.to_string());
        }
    });
}

/// Shows `message` in a native error dialog.
fn show_error(app: &AppHandle, message: String) {
    app.dialog()
        .message(message)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

/// Reopens the stored projects, or the welcome window when none can open.
pub fn restore(app: &AppHandle) {
    let store = app.state::<ProjectStore>();
    for path in store.paths() {
        match open_project_window(app, path.clone()) {
            Ok(()) => {}
            Err(OpenError::Server(message)) => show_error(app, message),
            Err(e) => {
                eprintln!("octodeck: {e}");
                store.remove(&path);
            }
        }
    }

    if app.state::<Windows>().is_empty() {
        open_welcome_window(app);
    }
}

/// Stops the closing window's server and forgets its project.
pub fn on_close_requested(window: &tauri::Window) {
    let app = window.app_handle();

    let windows = app.state::<Windows>();
    let Some(path) = windows.remove(window.label()) else {
        return;
    };

    app.state::<ProjectStore>().remove(&path);
    if windows.is_empty() {
        open_welcome_window(app);
    }
}

/// Builds the default menu with an Open Project… item in File.
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::default(app)?;
    let open = MenuItem::with_id(
        app,
        OPEN_PROJECT,
        "Open Project…",
        true,
        Some("CmdOrCtrl+O"),
    )?;

    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "File"
        {
            submenu.insert(&open, 0)?;
        }
    }

    Ok(menu)
}

#[tauri::command]
pub async fn open_project_folder(app: AppHandle) {
    pick_project(&app);
}

/// Lists the recent projects as `path`, `name` and `display` entries, most recent first.
#[tauri::command]
pub fn recent_factories(app: AppHandle) -> Vec<serde_json::Value> {
    let home = app.path().home_dir().ok();

    app.state::<RecentStore>()
        .paths()
        .iter()
        .map(|path| {
            let full = path.display().to_string();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| full.clone());
            let display = match &home {
                Some(home) => display_path(path, home),
                None => full.clone(),
            };

            serde_json::json!({ "path": full, "name": name, "display": display })
        })
        .collect()
}

/// Opens the recent project at `path`, and drops it from the list when it has no `.factory` folder.
#[tauri::command]
pub async fn open_recent_factory(app: AppHandle, path: String) {
    let path = PathBuf::from(&path);

    if let Err(e) = open_project_window(&app, path.clone()) {
        if drops_recent(&e) {
            app.state::<RecentStore>().remove(&path);
        }
        show_error(&app, e.to_string());
    }
}

/// Tells whether `e` removes the project from the recent list.
fn drops_recent(e: &OpenError) -> bool {
    matches!(e, OpenError::Factory(FactoryError::NoFactory(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_factory_drops_the_entry() {
        let e = OpenError::Factory(FactoryError::NoFactory(PathBuf::from("/src/a")));

        assert!(drops_recent(&e));
    }

    #[test]
    fn other_errors_keep_the_entry() {
        let errors = [
            OpenError::Factory(FactoryError::Config {
                path: PathBuf::from("/src/a/.factory/config.json"),
                reason: "bad port".into(),
            }),
            OpenError::Server("port in use".into()),
            OpenError::Window("no window".into()),
        ];

        for e in errors {
            assert!(!drops_recent(&e), "{e:?}");
        }
    }
}
