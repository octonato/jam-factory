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

//! Tauri entry point for the Jam Board desktop app.

mod github_view;
mod menu;
mod navigation;
mod probe;
mod windows;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .menu(menu::build)
        .on_menu_event(menu::on_event)
        .invoke_handler(tauri::generate_handler![
            windows::open_window,
            windows::open_main_window,
            github_view::github_view_show,
            github_view::github_view_hide,
            github_view::github_view_navigate
        ])
        .setup(|app| {
            windows::open_board_window(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Jam Board")
        .run(|app, event| {
            if let tauri::RunEvent::Opened { urls } = event {
                windows::open_links(app, &urls);
            }
        });
}
