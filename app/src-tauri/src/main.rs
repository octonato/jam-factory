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

mod navigation;
mod recent_store;
mod session_store;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};

use recent_store::RecentStore;
use session_store::ProjectStore;

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(windows::Windows::default())
        .setup(|app| {
            let store_file = app.path().app_data_dir()?.join("open-projects.json");
            app.manage(ProjectStore::load(store_file));

            let recent_file = app
                .path()
                .home_dir()?
                .join(".jam-factory")
                .join("recent-factories.json");
            app.manage(RecentStore::load(recent_file));

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
            windows::open_project_folder,
            windows::recent_factories,
            windows::open_recent_factory
        ])
        .build(tauri::generate_context!())
        .expect("error while building Jam Board");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { .. } = event {
            app.state::<ProjectStore>().mark_exiting();
        }
    });
}
