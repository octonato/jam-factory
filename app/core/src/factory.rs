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

//! Finds a project's factory root and reads its port from `config.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

pub const FACTORY_DIR: &str = ".factory";
pub const CONFIG_FILE: &str = "config.json";
pub const DEFAULT_PORT: u16 = 8731;

/// An opened project: its folder, its factory root and the port to serve on.
#[derive(Debug, Clone)]
pub struct Factory {
    pub project: PathBuf,
    pub root: PathBuf,
    pub port: u16,
}

impl Factory {
    /// The project folder name.
    pub fn name(&self) -> String {
        self.project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.project.display().to_string())
    }

    /// The `config.json` path in the factory root.
    pub fn config_file(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }
}

#[derive(Debug, Error)]
pub enum FactoryError {
    #[error("No .factory folder in {}", .0.display())]
    NoFactory(PathBuf),
    #[error("Cannot read the port from {}: {reason}", path.display())]
    Config { path: PathBuf, reason: String },
}

/// Opens the project at `picked`, or the parent project of a picked `.factory` folder.
pub fn open(picked: &Path) -> Result<Factory, FactoryError> {
    let canonical =
        fs::canonicalize(picked).map_err(|_| FactoryError::NoFactory(picked.to_path_buf()))?;

    let project = if canonical.ends_with(FACTORY_DIR) {
        canonical.parent().unwrap_or(&canonical).to_path_buf()
    } else {
        canonical
    };

    let root = project.join(FACTORY_DIR);
    if !root.is_dir() {
        return Err(FactoryError::NoFactory(project));
    }

    let port = read_port(&root.join(CONFIG_FILE))?;

    Ok(Factory {
        project,
        root,
        port,
    })
}

/// Reads the port from the config file at `path`, creating the file with the default port if missing.
fn read_port(path: &Path) -> Result<u16, FactoryError> {
    let error = |reason: String| FactoryError::Config {
        path: path.to_path_buf(),
        reason,
    };

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let default = format!("{{ \"port\": {DEFAULT_PORT} }}\n");
            fs::write(path, default).map_err(|e| error(e.to_string()))?;
            return Ok(DEFAULT_PORT);
        }
        Err(e) => return Err(error(e.to_string())),
    };

    let config: Value = serde_json::from_str(&text).map_err(|e| error(e.to_string()))?;

    config
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| error("\"port\" must be an integer from 1 to 65535".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(dir: &Path) -> PathBuf {
        let proj = dir.join("proj");
        fs::create_dir_all(proj.join(FACTORY_DIR)).unwrap();
        proj
    }

    fn write_config(proj: &Path, text: &str) {
        fs::write(proj.join(FACTORY_DIR).join(CONFIG_FILE), text).unwrap();
    }

    #[test]
    fn opens_project_with_factory() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path());
        write_config(&proj, r#"{ "port": 9000 }"#);

        let factory = open(&proj).unwrap();

        let canonical = fs::canonicalize(&proj).unwrap();
        assert_eq!(factory.project, canonical);
        assert_eq!(factory.root, canonical.join(".factory"));
        assert_eq!(factory.port, 9000);
        assert_eq!(factory.name(), "proj");
        assert_eq!(
            factory.config_file(),
            canonical.join(".factory").join("config.json")
        );
    }

    #[test]
    fn open_accepts_factory_folder() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path());
        write_config(&proj, r#"{ "port": 9000 }"#);

        let factory = open(&proj.join(".factory")).unwrap();

        let canonical = fs::canonicalize(&proj).unwrap();
        assert_eq!(factory.project, canonical);
        assert_eq!(factory.root, canonical.join(".factory"));
    }

    #[test]
    fn open_canonicalizes_project() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path());
        write_config(&proj, r#"{ "port": 9000 }"#);
        let picked = PathBuf::from(format!("{}/../proj/", proj.display()));

        let factory = open(&picked).unwrap();

        assert_eq!(factory.project, fs::canonicalize(&proj).unwrap());
    }

    #[test]
    fn refuses_folder_without_factory() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty");
        fs::create_dir(&empty).unwrap();

        let err = open(&empty).unwrap_err();

        assert!(matches!(err, FactoryError::NoFactory(_)));
        assert_eq!(
            err.to_string(),
            format!(
                "No .factory folder in {}",
                fs::canonicalize(&empty).unwrap().display()
            )
        );
    }

    #[test]
    fn creates_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path());

        let factory = open(&proj).unwrap();

        assert_eq!(factory.port, 8731);
        assert_eq!(
            fs::read_to_string(factory.config_file()).unwrap(),
            "{ \"port\": 8731 }\n"
        );
    }

    #[test]
    fn rejects_bad_config() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path());
        let config = fs::canonicalize(&proj)
            .unwrap()
            .join(".factory")
            .join("config.json");
        let prefix = format!("Cannot read the port from {}: ", config.display());

        for text in [
            "not json",
            "{}",
            r#"{ "port": "8731" }"#,
            r#"{ "port": 0 }"#,
            r#"{ "port": 70000 }"#,
        ] {
            write_config(&proj, text);

            let err = open(&proj).unwrap_err();

            assert!(matches!(err, FactoryError::Config { .. }), "{text}");
            assert!(err.to_string().starts_with(&prefix), "{text}: {err}");
        }
    }
}
