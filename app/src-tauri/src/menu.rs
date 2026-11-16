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

use tauri::menu::{Menu, MenuEvent, MenuItem, MenuItemKind};
use tauri::{AppHandle, Wry};

use crate::windows;

const QUIT: &str = "quit";

/// Returns the default menu with a Quit item that closes the board windows.
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

    let label = format!("Quit {}", app.package_info().name);
    let quit = MenuItem::with_id(app, QUIT, label, true, Some("CmdOrCtrl+Q"))?;
    app_menu.append(&quit)?;

    Ok(menu)
}

/// Closes the board windows on Quit.
pub fn on_event(app: &AppHandle, event: MenuEvent) {
    if event.id() == QUIT {
        windows::close_board_windows(app);
    }
}
