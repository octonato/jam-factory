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

//! Finds the factory root from `~/.jam-factory/config.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

pub const CONFIG_DIR: &str = ".jam-factory";
pub const CONFIG_FILE: &str = "config.json";
pub const DEFAULT_ROOT: &str = "factory";

/// A config file that cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read the factory root from {}: {reason}", path.display())]
pub struct RootError {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Deserialize)]
struct Config {
    root: Option<String>,
}

/// Returns the factory root for the home folder `home`.
pub fn resolve(home: &Path) -> Result<PathBuf, RootError> {
    let dir = home.join(CONFIG_DIR);
    let path = dir.join(CONFIG_FILE);
    let default = dir.join(DEFAULT_ROOT);

    let error = |reason: String| RootError {
        path: path.clone(),
        reason,
    };

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(default),
        Err(e) => return Err(error(e.to_string())),
    };

    let config: Config = serde_json::from_str(&text).map_err(|e| error(e.to_string()))?;

    let Some(root) = config.root else {
        return Ok(default);
    };

    let expanded = expand(home, &root);
    if !expanded.is_absolute() {
        return Err(error(format!("root {root:?} is not an absolute path")));
    }

    Ok(expanded)
}

/// Replaces a leading `~` in `root` with `home`.
fn expand(home: &Path, root: &str) -> PathBuf {
    if root == "~" {
        return home.to_path_buf();
    }

    match root.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(home: &Path, text: &str) -> PathBuf {
        let dir = home.join(".jam-factory");
        fs::create_dir_all(&dir).unwrap();

        let config = dir.join("config.json");
        fs::write(&config, text).unwrap();
        config
    }

    #[test]
    fn default_root_without_config() {
        let home = tempfile::tempdir().unwrap();

        let root = resolve(home.path()).unwrap();

        assert_eq!(root, home.path().join(".jam-factory").join("factory"));
    }

    #[test]
    fn config_root_wins() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "root": "/x/y" }"#);

        let root = resolve(home.path()).unwrap();

        assert_eq!(root, PathBuf::from("/x/y"));
    }

    #[test]
    fn tilde_expands() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "root": "~/f" }"#);

        let root = resolve(home.path()).unwrap();

        assert_eq!(root, home.path().join("f"));
    }

    #[test]
    fn config_without_root_uses_default() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), "{}");

        let root = resolve(home.path()).unwrap();

        assert_eq!(root, home.path().join(".jam-factory").join("factory"));
    }

    #[test]
    fn bad_config_is_an_error() {
        let home = tempfile::tempdir().unwrap();

        for text in [
            "not json",
            r#"{ "root": 3 }"#,
            r#"{ "root": "factory" }"#,
            r#"{ "root": "" }"#,
            r#"{ "root": "~user/x" }"#,
            r#"{ "root": "$HOME/f" }"#,
        ] {
            let config = write_config(home.path(), text);

            let err = resolve(home.path()).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
        }
    }
}
