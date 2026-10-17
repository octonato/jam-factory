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

//! Tauri entry point for the octodeck desktop app.

mod commands;
mod dto;
mod owners;
mod session_store;
mod state;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};

use session_store::ProjectStore;

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::load())
        .manage(windows::Windows::default())
        .manage(owners::TerminalOwners::default())
        .setup(|app| {
            let store_file = app.path().app_data_dir()?.join("open-projects.json");
            app.manage(ProjectStore::load(store_file));

            app.set_menu(windows::build_menu(app.handle())?)?;
            app.on_menu_event(|app, event| {
                if event.id() == windows::OPEN_PROJECT {
                    windows::pick_project(app);
                }
            });

            windows::restore(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                windows::on_close_requested(window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::open_project,
            commands::list_editors,
            commands::open_in_editor,
            commands::open_terminal,
            commands::write_terminal,
            commands::resize_terminal,
            commands::close_terminal,
            commands::kill_session,
            commands::list_live_sessions,
            windows::current_project,
            windows::open_project_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building octodeck");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { .. } = event {
            app.state::<ProjectStore>().mark_exiting();
        }
    });
}
