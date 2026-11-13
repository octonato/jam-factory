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

//! Finds the factory root, the server port and the repos from `~/.jam-factory/config.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

pub const CONFIG_DIR: &str = ".jam-factory";
pub const CONFIG_FILE: &str = "config.json";
pub const DEFAULT_ROOT: &str = "factory";
pub const DEFAULT_PORT: u16 = 8731;

/// A config file that cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read the factory root from {}: {reason}", path.display())]
pub struct RootError {
    pub path: PathBuf,
    pub reason: String,
}

/// A config file whose server port cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read the server port from {}: {reason}", path.display())]
pub struct PortError {
    pub path: PathBuf,
    pub reason: String,
}

/// A config file whose repos cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read the repos from {}: {reason}", path.display())]
pub struct ReposError {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Default, Deserialize)]
struct RootConfig {
    root: Option<String>,
}

#[derive(Default, Deserialize)]
struct PortConfig {
    port: Option<u16>,
}

#[derive(Default, Deserialize)]
struct ReposConfig {
    repos: Option<Vec<String>>,
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

    let config: RootConfig = read_config(&path).map_err(error)?;

    let Some(root) = config.root else {
        return Ok(default);
    };

    let expanded = expand(home, &root);
    if !expanded.is_absolute() {
        return Err(error(format!("root {root:?} is not an absolute path")));
    }

    Ok(expanded)
}

/// Returns the server port for the home folder `home`.
pub fn resolve_port(home: &Path) -> Result<u16, PortError> {
    let path = home.join(CONFIG_DIR).join(CONFIG_FILE);

    let error = |reason: String| PortError {
        path: path.clone(),
        reason,
    };

    let config: PortConfig = read_config(&path).map_err(error)?;

    match config.port {
        None => Ok(DEFAULT_PORT),
        Some(0) => Err(error("port 0 is not between 1 and 65535".to_string())),
        Some(port) => Ok(port),
    }
}

/// Returns the main checkouts listed in the config for the home folder `home`.
pub fn resolve_repos(home: &Path) -> Result<Vec<PathBuf>, ReposError> {
    let path = home.join(CONFIG_DIR).join(CONFIG_FILE);

    let error = |reason: String| ReposError {
        path: path.clone(),
        reason,
    };

    let config: ReposConfig = read_config(&path).map_err(error)?;

    let mut repos = Vec::new();
    for repo in config.repos.unwrap_or_default() {
        let expanded = expand(home, &repo);
        if !expanded.is_absolute() {
            return Err(error(format!("repo {repo:?} is not an absolute path")));
        }

        repos.push(expanded);
    }

    Ok(repos)
}

/// Reads and parses the config file at `path`, or returns an empty config when it is missing.
fn read_config<T: Default + DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(e.to_string()),
    };

    serde_json::from_str(&text).map_err(|e| e.to_string())
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

    #[test]
    fn default_port_without_config() {
        let home = tempfile::tempdir().unwrap();

        let port = resolve_port(home.path()).unwrap();

        assert_eq!(port, 8731);
    }

    #[test]
    fn config_without_port_uses_default() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "root": "/x/y" }"#);

        let port = resolve_port(home.path()).unwrap();

        assert_eq!(port, 8731);
    }

    #[test]
    fn config_port_wins() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000 }"#);

        let port = resolve_port(home.path()).unwrap();

        assert_eq!(port, 9000);
    }

    #[test]
    fn bad_port_is_an_error() {
        let home = tempfile::tempdir().unwrap();

        for text in [
            r#"{ "port": 0 }"#,
            r#"{ "port": 65536 }"#,
            r#"{ "port": "8731" }"#,
            r#"{ "port": -1 }"#,
            "not json",
        ] {
            let config = write_config(home.path(), text);

            let err = resolve_port(home.path()).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
        }
    }

    #[test]
    fn bad_port_does_not_break_root() {
        let home = tempfile::tempdir().unwrap();

        write_config(home.path(), r#"{ "root": "/x/y", "port": 70000 }"#);
        let root = resolve(home.path()).unwrap();
        assert_eq!(root, PathBuf::from("/x/y"));

        write_config(home.path(), r#"{ "port": "9000" }"#);
        let root = resolve(home.path()).unwrap();
        assert_eq!(root, home.path().join(".jam-factory").join("factory"));
    }

    #[test]
    fn bad_root_does_not_break_port() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "root": 3, "port": 9000 }"#);

        let port = resolve_port(home.path()).unwrap();

        assert_eq!(port, 9000);
    }

    #[test]
    fn no_repos_without_config() {
        let home = tempfile::tempdir().unwrap();

        let repos = resolve_repos(home.path()).unwrap();

        assert_eq!(repos, Vec::<PathBuf>::new());
    }

    #[test]
    fn config_without_repos_is_empty() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "root": "/x" }"#);

        let repos = resolve_repos(home.path()).unwrap();

        assert_eq!(repos, Vec::<PathBuf>::new());
    }

    #[test]
    fn repos_expand_tilde() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "repos": ["~/a/b/main", "/x/y/main"] }"#);

        let repos = resolve_repos(home.path()).unwrap();

        assert_eq!(
            repos,
            vec![home.path().join("a/b/main"), PathBuf::from("/x/y/main")]
        );
    }

    #[test]
    fn bad_repos_is_an_error() {
        let home = tempfile::tempdir().unwrap();

        for text in [
            r#"{ "repos": "x" }"#,
            r#"{ "repos": [3] }"#,
            r#"{ "repos": ["rel/path"] }"#,
            r#"{ "repos": [""] }"#,
            "not json",
        ] {
            let config = write_config(home.path(), text);

            let err = resolve_repos(home.path()).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
        }
    }

    #[test]
    fn bad_repos_does_not_break_root_or_port() {
        let home = tempfile::tempdir().unwrap();
        write_config(
            home.path(),
            r#"{ "root": "/x/y", "port": 9000, "repos": 3 }"#,
        );

        let root = resolve(home.path()).unwrap();
        assert_eq!(root, PathBuf::from("/x/y"));

        let port = resolve_port(home.path()).unwrap();
        assert_eq!(port, 9000);
    }
}
