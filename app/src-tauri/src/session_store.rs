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

//! Remembers which projects have an open window, in `open-projects.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The list of open project paths, saved to a file on every change.
pub struct ProjectStore {
    file: PathBuf,
    paths: Mutex<Vec<PathBuf>>,
    exiting: AtomicBool,
}

impl ProjectStore {
    /// Reads `file`. A missing or unreadable file gives an empty list.
    pub fn load(file: PathBuf) -> Self {
        let paths = fs::read_to_string(&file)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();

        Self {
            file,
            paths: Mutex::new(paths),
            exiting: AtomicBool::new(false),
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.paths.lock().unwrap().clone()
    }

    pub fn add(&self, path: &Path) {
        let mut paths = self.paths.lock().unwrap();
        if !paths.iter().any(|p| p == path) {
            paths.push(path.to_path_buf());
            self.save(&paths);
        }
    }

    /// Drops `path`, unless the app is exiting.
    pub fn remove(&self, path: &Path) {
        if self.exiting.load(Ordering::SeqCst) {
            return;
        }

        let mut paths = self.paths.lock().unwrap();
        paths.retain(|p| p != path);
        self.save(&paths);
    }

    pub fn mark_exiting(&self) {
        self.exiting.store(true, Ordering::SeqCst);
    }

    fn save(&self, paths: &[PathBuf]) {
        if let Some(parent) = self.file.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(paths) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.file, json) {
                    eprintln!("jam-board: cannot save {}: {e}", self.file.display());
                }
            }
            Err(e) => eprintln!("jam-board: cannot encode open projects: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> ProjectStore {
        ProjectStore::load(dir.path().join("open-projects.json"))
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();

        assert!(store_in(&dir).paths().is_empty());
    }

    #[test]
    fn added_paths_survive_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);

        store.add(Path::new("/src/akka"));
        store.add(Path::new("/src/octonato"));
        store.add(Path::new("/src/akka"));

        assert_eq!(
            store_in(&dir).paths(),
            vec![PathBuf::from("/src/akka"), PathBuf::from("/src/octonato")]
        );
    }

    #[test]
    fn removed_paths_are_gone_after_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.add(Path::new("/src/akka"));
        store.add(Path::new("/src/octonato"));

        store.remove(Path::new("/src/akka"));

        assert_eq!(store_in(&dir).paths(), vec![PathBuf::from("/src/octonato")]);
    }

    #[test]
    fn removals_stop_once_exiting() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.add(Path::new("/src/akka"));

        store.mark_exiting();
        store.remove(Path::new("/src/akka"));

        assert_eq!(store_in(&dir).paths(), vec![PathBuf::from("/src/akka")]);
    }

    #[test]
    fn corrupt_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("open-projects.json"), "not json").unwrap();

        assert!(store_in(&dir).paths().is_empty());
    }
}
