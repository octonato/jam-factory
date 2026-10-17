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

//! Reads and parses the config file at `~/.config/odeck/config.toml`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// An editor the user can open a checkout in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Editor {
    pub name: String,
    pub command: String,
}

pub const DEFAULT_FONT_SIZE: u16 = 16;

/// The config.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub github_src: Option<PathBuf>,
    #[serde(rename = "editor")]
    pub editors: Vec<Editor>,
    pub font_size: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            github_src: None,
            editors: Vec::new(),
            font_size: DEFAULT_FONT_SIZE,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid config: {0}")]
    Parse(String),
}

/// Parses config file text into a `Config`.
pub fn parse(content: &str) -> Result<Config, ConfigError> {
    toml::from_str(content).map_err(|e| ConfigError::Parse(e.to_string()))
}

/// Reads the config file at `path`. Returns the default config if the file is missing.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(content) => parse(&content),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e.into()),
    }
}

/// The config file path under `home`.
pub fn default_path(home: &Path) -> PathBuf {
    home.join(".config").join("odeck").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
github_src = "/Users/me/Sources"

[[editor]]
name = "Zed"
command = "zed"

[[editor]]
name = "IntelliJ"
command = "idea"
"#;

    fn editor(name: &str, command: &str) -> Editor {
        Editor {
            name: name.to_string(),
            command: command.to_string(),
        }
    }

    #[test]
    fn parses_editors_and_github_src() {
        let config = parse(SAMPLE).unwrap();

        assert_eq!(config.github_src, Some(PathBuf::from("/Users/me/Sources")));
        assert_eq!(
            config.editors,
            vec![editor("Zed", "zed"), editor("IntelliJ", "idea")]
        );
    }

    #[test]
    fn parses_font_size() {
        let config = parse("font_size = 18\n").unwrap();

        assert_eq!(config.font_size, 18);
    }

    #[test]
    fn font_size_defaults_to_16() {
        assert_eq!(parse("").unwrap().font_size, 16);
        assert_eq!(Config::default().font_size, 16);
    }

    #[test]
    fn empty_file_is_default() {
        let config = parse("").unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();

        let config = load(&dir.path().join("config.toml")).unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn invalid_toml_is_error() {
        let result = parse("[[editor]\n");

        assert!(matches!(result, Err(ConfigError::Parse(_))));
    }

    #[test]
    fn default_path_is_under_dot_config() {
        let path = default_path(Path::new("/home/me"));

        assert_eq!(path, PathBuf::from("/home/me/.config/odeck/config.toml"));
    }
}
