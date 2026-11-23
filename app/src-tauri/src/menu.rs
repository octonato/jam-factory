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

//! The app menu.

use tauri::menu::{Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

use crate::{github_view, windows};

/// An item of the Go menu, which runs a board shortcut in the page of the focused board window.
struct GoItem {
    id: &'static str,
    label: &'static str,
    accelerator: &'static str,
}

/// The items of the Go menu, in menu order.
const GO_ITEMS: [GoItem; 8] = [
    GoItem {
        id: "go-branches",
        label: "Branches",
        accelerator: "Shift+CmdOrCtrl+B",
    },
    GoItem {
        id: "go-worktrees",
        label: "Worktrees",
        accelerator: "Shift+CmdOrCtrl+W",
    },
    GoItem {
        id: "go-terminals",
        label: "Terminals",
        accelerator: "Shift+CmdOrCtrl+T",
    },
    GoItem {
        id: "go-github",
        label: "GitHub",
        accelerator: "Shift+CmdOrCtrl+G",
    },
    GoItem {
        id: "go-previous-bar",
        label: "Previous Bar Item",
        accelerator: "Alt+CmdOrCtrl+BracketLeft",
    },
    GoItem {
        id: "go-next-bar",
        label: "Next Bar Item",
        accelerator: "Alt+CmdOrCtrl+BracketRight",
    },
    GoItem {
        id: "go-detach",
        label: "Open in New Window",
        accelerator: "CmdOrCtrl+N",
    },
    GoItem {
        id: "go-board",
        label: "Jam Board Window",
        accelerator: "CmdOrCtrl+0",
    },
];

const QUIT: &str = "quit";

const RELOAD_GITHUB: &str = "reload-github";

const RELOAD_GITHUB_ACCELERATOR: &str = "CmdOrCtrl+R";

const BIGGER_TEXT: &str = "bigger-text";

const BIGGER_TEXT_ACCELERATOR: &str = "CmdOrCtrl+=";

const SMALLER_TEXT: &str = "smaller-text";

const SMALLER_TEXT_ACCELERATOR: &str = "CmdOrCtrl+-";

/// Returns the default menu with Bigger Text, Smaller Text and Reload GitHub items, a Quit item that closes the board windows, and the Go menu after the app menu.
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::default(app)?;
    let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() else {
        return Ok(menu);
    };

    if let Some(MenuItemKind::Predefined(quit)) = app_menu.items()?.last()
        && quit.text()?.starts_with("Quit")
    {
        app_menu.remove(quit)?;
    }

    let bigger = MenuItem::with_id(
        app,
        BIGGER_TEXT,
        "Bigger Text",
        true,
        Some(BIGGER_TEXT_ACCELERATOR),
    )?;
    app_menu.append(&bigger)?;

    let smaller = MenuItem::with_id(
        app,
        SMALLER_TEXT,
        "Smaller Text",
        true,
        Some(SMALLER_TEXT_ACCELERATOR),
    )?;
    app_menu.append(&smaller)?;

    let reload = MenuItem::with_id(
        app,
        RELOAD_GITHUB,
        "Reload GitHub",
        true,
        Some(RELOAD_GITHUB_ACCELERATOR),
    )?;
    app_menu.append(&reload)?;
    app_menu.append(&PredefinedMenuItem::separator(app)?)?;

    let label = format!("Quit {}", app.package_info().name);
    let quit = MenuItem::with_id(app, QUIT, label, true, Some("CmdOrCtrl+Q"))?;
    app_menu.append(&quit)?;

    menu.insert(&go_menu(app)?, 1)?;

    Ok(menu)
}

/// Returns the Go menu with one item per entry of `GO_ITEMS`.
fn go_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let go = Submenu::new(app, "Go", true)?;

    for item in &GO_ITEMS {
        let entry = MenuItem::with_id(app, item.id, item.label, true, Some(item.accelerator))?;
        go.append(&entry)?;
    }

    Ok(go)
}

/// Closes the board windows on Quit, steps the text size of the focused window on Bigger Text and Smaller Text, reloads its GitHub view on Reload GitHub, and runs a Go item in its page.
pub fn on_event(app: &AppHandle, event: MenuEvent) {
    if event.id() == QUIT {
        windows::close_board_windows(app);
    } else if event.id() == BIGGER_TEXT {
        windows::step_text_size(app, 1);
    } else if event.id() == SMALLER_TEXT {
        windows::step_text_size(app, -1);
    } else if event.id() == RELOAD_GITHUB {
        github_view::reload_focused(app);
    } else if let Some(item) = GO_ITEMS.iter().find(|item| event.id() == item.id) {
        windows::run_menu_shortcut(app, item.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_reloads_github_with_cmd_r() {
        assert_eq!(RELOAD_GITHUB, "reload-github");
        assert_eq!(RELOAD_GITHUB_ACCELERATOR, "CmdOrCtrl+R");
    }

    #[test]
    fn the_menu_steps_the_text_size_with_cmd_equal_and_cmd_minus() {
        assert_eq!(BIGGER_TEXT_ACCELERATOR, "CmdOrCtrl+=");
        assert_eq!(SMALLER_TEXT_ACCELERATOR, "CmdOrCtrl+-");
    }

    #[test]
    fn the_go_menu_holds_the_board_shortcuts() {
        let items: Vec<(&str, &str, &str)> = GO_ITEMS
            .iter()
            .map(|item| (item.id, item.label, item.accelerator))
            .collect();

        assert_eq!(
            items,
            [
                ("go-branches", "Branches", "Shift+CmdOrCtrl+B"),
                ("go-worktrees", "Worktrees", "Shift+CmdOrCtrl+W"),
                ("go-terminals", "Terminals", "Shift+CmdOrCtrl+T"),
                ("go-github", "GitHub", "Shift+CmdOrCtrl+G"),
                (
                    "go-previous-bar",
                    "Previous Bar Item",
                    "Alt+CmdOrCtrl+BracketLeft"
                ),
                ("go-next-bar", "Next Bar Item", "Alt+CmdOrCtrl+BracketRight"),
                ("go-detach", "Open in New Window", "CmdOrCtrl+N"),
                ("go-board", "Jam Board Window", "CmdOrCtrl+0"),
            ]
        );
    }
}
