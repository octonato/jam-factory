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

//! Remembers the recently opened projects, in `recent-factories.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The most paths the list keeps.
pub const MAX_RECENT: usize = 10;

/// The recently opened project paths, most recent first, saved to a file on every change.
pub struct RecentStore {
    file: PathBuf,
    paths: Mutex<Vec<PathBuf>>,
}

impl RecentStore {
    /// Reads `file`. A missing, unreadable or invalid file gives an empty list.
    pub fn load(file: PathBuf) -> Self {
        let mut paths: Vec<PathBuf> = fs::read_to_string(&file)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        paths.truncate(MAX_RECENT);

        Self {
            file,
            paths: Mutex::new(paths),
        }
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.paths.lock().unwrap().clone()
    }

    /// Moves `path` to the top of the list.
    pub fn touch(&self, path: &Path) {
        let mut paths = self.paths.lock().unwrap();
        paths.retain(|p| p != path);
        paths.insert(0, path.to_path_buf());
        paths.truncate(MAX_RECENT);
        self.save(&paths);
    }

    pub fn remove(&self, path: &Path) {
        let mut paths = self.paths.lock().unwrap();
        paths.retain(|p| p != path);
        self.save(&paths);
    }

    fn save(&self, paths: &[PathBuf]) {
        if let Some(parent) = self.file.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(paths) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.file, json) {
                    eprintln!("octodeck: cannot save {}: {e}", self.file.display());
                }
            }
            Err(e) => eprintln!("octodeck: cannot encode recent projects: {e}"),
        }
    }
}

/// Shows `path` with a leading `home` folder written as `~`.
pub fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> RecentStore {
        RecentStore::load(recent_file(dir))
    }

    fn recent_file(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("recent").join("recent-factories.json")
    }

    fn write_file(dir: &tempfile::TempDir, content: &str) {
        let file = recent_file(dir);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();

        assert!(store_in(&dir).paths().is_empty());
    }

    #[test]
    fn corrupt_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, "not json");

        assert!(store_in(&dir).paths().is_empty());
    }

    #[test]
    fn non_array_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, r#"{"a":1}"#);

        assert!(store_in(&dir).paths().is_empty());
    }

    #[test]
    fn touched_paths_survive_a_reload_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);

        store.touch(Path::new("/src/a"));
        store.touch(Path::new("/src/b"));

        assert_eq!(
            store_in(&dir).paths(),
            vec![PathBuf::from("/src/b"), PathBuf::from("/src/a")]
        );
    }

    #[test]
    fn touching_a_known_path_moves_it_to_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);

        store.touch(Path::new("/src/a"));
        store.touch(Path::new("/src/b"));
        store.touch(Path::new("/src/a"));

        assert_eq!(
            store_in(&dir).paths(),
            vec![PathBuf::from("/src/a"), PathBuf::from("/src/b")]
        );
    }

    #[test]
    fn touch_drops_duplicates_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, r#"["/src/a","/src/b","/src/a"]"#);

        store_in(&dir).touch(Path::new("/src/a"));

        assert_eq!(
            store_in(&dir).paths(),
            vec![PathBuf::from("/src/a"), PathBuf::from("/src/b")]
        );
    }

    #[test]
    fn keeps_at_most_ten_paths() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);

        for i in 0..12 {
            store.touch(&PathBuf::from(format!("/src/{i}")));
        }

        let paths = store_in(&dir).paths();
        assert_eq!(paths.len(), MAX_RECENT);
        assert_eq!(paths.first(), Some(&PathBuf::from("/src/11")));
        assert_eq!(paths.last(), Some(&PathBuf::from("/src/2")));
    }

    #[test]
    fn removed_paths_are_gone_after_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.touch(Path::new("/src/a"));
        store.touch(Path::new("/src/b"));

        store.remove(Path::new("/src/a"));

        assert_eq!(store_in(&dir).paths(), vec![PathBuf::from("/src/b")]);
    }

    #[test]
    fn display_path_replaces_home_with_tilde() {
        assert_eq!(
            display_path(Path::new("/Users/me/src/a"), Path::new("/Users/me")),
            "~/src/a"
        );
    }

    #[test]
    fn display_path_shows_home_as_tilde() {
        assert_eq!(
            display_path(Path::new("/Users/me"), Path::new("/Users/me")),
            "~"
        );
    }

    #[test]
    fn display_path_keeps_paths_outside_home() {
        assert_eq!(
            display_path(Path::new("/opt/a"), Path::new("/Users/me")),
            "/opt/a"
        );
    }

    #[test]
    fn display_path_needs_a_folder_boundary() {
        assert_eq!(
            display_path(Path::new("/Users/me2/a"), Path::new("/Users/me")),
            "/Users/me2/a"
        );
    }
}
