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

use tauri::menu::{Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem};
use tauri::{AppHandle, Wry};

use crate::{github_view, windows};

const QUIT: &str = "quit";

const RELOAD_GITHUB: &str = "reload-github";

const RELOAD_GITHUB_ACCELERATOR: &str = "CmdOrCtrl+R";

const BIGGER_TEXT: &str = "bigger-text";

const BIGGER_TEXT_ACCELERATOR: &str = "CmdOrCtrl+=";

const SMALLER_TEXT: &str = "smaller-text";

const SMALLER_TEXT_ACCELERATOR: &str = "CmdOrCtrl+-";

/// Returns the default menu with Bigger Text, Smaller Text and Reload GitHub items, and a Quit item that closes the board windows.
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

    Ok(menu)
}

/// Closes the board windows on Quit, steps the text size of the focused window on Bigger Text and Smaller Text, and reloads its GitHub view on Reload GitHub.
pub fn on_event(app: &AppHandle, event: MenuEvent) {
    if event.id() == QUIT {
        windows::close_board_windows(app);
    } else if event.id() == BIGGER_TEXT {
        windows::step_text_size(app, 1);
    } else if event.id() == SMALLER_TEXT {
        windows::step_text_size(app, -1);
    } else if event.id() == RELOAD_GITHUB {
        github_view::reload_focused(app);
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
}
