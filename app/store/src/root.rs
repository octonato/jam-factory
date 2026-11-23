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

//! Finds the factory root, the server port, the repos, the GitHub user, the GitHub sign-in hosts and the GitHub names of the repos from `~/.jam-factory/config.json`.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
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
pub const PORT_VAR: &str = "JAM_FACTORY_PORT";

/// A config file that cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read the factory root from {}: {reason}", path.display())]
pub struct RootError {
    pub path: PathBuf,
    pub reason: String,
}

/// A server port that cannot be read from the config file or from `JAM_FACTORY_PORT`.
#[derive(Debug, Error)]
pub enum PortError {
    #[error("Cannot read the server port from {}: {reason}", path.display())]
    Config { path: PathBuf, reason: String },

    #[error("Cannot read the server port from {PORT_VAR}: {value:?} is not between 1 and 65535")]
    Var { value: OsString },
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

/// A config file whose GitHub user cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read the GitHub user from {}: {reason}", path.display())]
pub struct GithubUserError {
    pub path: PathBuf,
    pub reason: String,
}

/// A config file whose GitHub sign-in hosts cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read githubSignInHosts from {}: {reason}", path.display())]
pub struct SignInHostsError {
    pub path: PathBuf,
    pub reason: String,
}

/// A config file whose GitHub names of the repos cannot be read, with its path and the reason.
#[derive(Debug, Error)]
#[error("Cannot read github from {}: {reason}", path.display())]
pub struct GithubReposError {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Default, Deserialize)]
struct ReposConfig {
    repos: Option<Vec<String>>,
}

#[derive(Default, Deserialize)]
struct GithubUserConfig {
    #[serde(rename = "githubUser")]
    github_user: Option<String>,
}

#[derive(Default, Deserialize)]
struct SignInHostsConfig {
    #[serde(rename = "githubSignInHosts")]
    hosts: Option<Vec<String>>,
}

#[derive(Default, Deserialize)]
struct GithubReposConfig {
    github: Option<BTreeMap<String, String>>,
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

/// Returns the server port from `JAM_FACTORY_PORT`, else from the config for the home folder `home`.
pub fn resolve_port(home: &Path) -> Result<u16, PortError> {
    resolve_port_with(home, env::var_os(PORT_VAR))
}

/// Returns the server port from `var`, else from the config for the home folder `home`.
fn resolve_port_with(home: &Path, var: Option<OsString>) -> Result<u16, PortError> {
    if let Some(value) = var {
        return parse_port_var(value);
    }

    let path = home.join(CONFIG_DIR).join(CONFIG_FILE);

    let error = |reason: String| PortError::Config {
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

/// Parses a `JAM_FACTORY_PORT` value as a port from 1 to 65535.
fn parse_port_var(value: OsString) -> Result<u16, PortError> {
    let port = value.to_str().and_then(|text| text.parse::<u16>().ok());

    match port {
        Some(port) if port != 0 => Ok(port),
        _ => Err(PortError::Var { value }),
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

/// Returns the `githubUser` of the config for the home folder `home`, or `None` when it is missing or blank.
pub fn resolve_github_user(home: &Path) -> Result<Option<String>, GithubUserError> {
    let path = home.join(CONFIG_DIR).join(CONFIG_FILE);

    let config: GithubUserConfig = read_config(&path).map_err(|reason| GithubUserError {
        path: path.clone(),
        reason,
    })?;

    let user = config.github_user.map(|user| user.trim().to_string());

    Ok(user.filter(|user| !user.is_empty()))
}

/// Returns the `githubSignInHosts` patterns of the config for the home folder `home`, lowercased and trimmed.
pub fn resolve_sign_in_hosts(home: &Path) -> Result<Vec<String>, SignInHostsError> {
    let path = home.join(CONFIG_DIR).join(CONFIG_FILE);

    let error = |reason: String| SignInHostsError {
        path: path.clone(),
        reason,
    };

    let config: SignInHostsConfig = read_config(&path).map_err(error)?;

    let mut hosts = Vec::new();
    for host in config.hosts.unwrap_or_default() {
        let pattern = host.trim().to_lowercase();
        if !is_host_pattern(&pattern) {
            return Err(error(format!(
                "host {host:?} is not a host or *. plus a domain"
            )));
        }

        hosts.push(pattern);
    }

    Ok(hosts)
}

/// Tells whether `pattern` is a host, or `*.` plus a domain, made of letters, digits, `-` and dots.
fn is_host_pattern(pattern: &str) -> bool {
    let domain = pattern.strip_prefix("*.").unwrap_or(pattern);

    domain.split('.').all(|label| {
        !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// Returns the `github` map of the config for the home folder `home`, from each local `org/repo` to its GitHub `owner/name`.
pub fn resolve_github_repos(home: &Path) -> Result<BTreeMap<String, String>, GithubReposError> {
    let path = home.join(CONFIG_DIR).join(CONFIG_FILE);

    let error = |reason: String| GithubReposError {
        path: path.clone(),
        reason,
    };

    let config: GithubReposConfig = read_config(&path).map_err(error)?;

    let github = config.github.unwrap_or_default();
    for (local, remote) in &github {
        if !is_repo_name(local) {
            return Err(error(format!("repo {local:?} is not org/repo")));
        }

        if !is_repo_name(remote) {
            return Err(error(format!(
                "GitHub name {remote:?} of {local:?} is not owner/name"
            )));
        }
    }

    Ok(github)
}

/// Tells whether `text` is two non-empty parts split by one `/`.
fn is_repo_name(text: &str) -> bool {
    match text.split_once('/') {
        Some((owner, name)) => !owner.is_empty() && !name.is_empty() && !name.contains('/'),
        None => false,
    }
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

        let port = resolve_port_with(home.path(), None).unwrap();

        assert_eq!(port, 8731);
    }

    #[test]
    fn config_without_port_uses_default() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "root": "/x/y" }"#);

        let port = resolve_port_with(home.path(), None).unwrap();

        assert_eq!(port, 8731);
    }

    #[test]
    fn config_port_wins() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000 }"#);

        let port = resolve_port_with(home.path(), None).unwrap();

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

            let err = resolve_port_with(home.path(), None).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
        }
    }

    #[test]
    fn port_var_wins_over_config() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000 }"#);

        let port = resolve_port_with(home.path(), Some("8732".into())).unwrap();

        assert_eq!(port, 8732);
    }

    #[test]
    fn port_var_ignores_a_bad_config() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), "not json");

        let port = resolve_port_with(home.path(), Some("8732".into())).unwrap();

        assert_eq!(port, 8732);
    }

    #[test]
    fn unset_port_var_uses_config() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000 }"#);

        let port = resolve_port_with(home.path(), None).unwrap();

        assert_eq!(port, 9000);
    }

    #[test]
    fn bad_port_var_is_an_error() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000 }"#);

        for value in ["", "0", "65536", "-1", "abc", " 8732", "8732.0"] {
            let err = resolve_port_with(home.path(), Some(value.into())).unwrap_err();

            assert!(
                err.to_string().contains("JAM_FACTORY_PORT"),
                "{value}: {err}"
            );
            assert!(
                err.to_string().contains(&format!("{value:?}")),
                "{value}: {err}"
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

        let port = resolve_port_with(home.path(), None).unwrap();

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
    fn github_user_is_read_and_trimmed() {
        let home = tempfile::tempdir().unwrap();

        assert_eq!(resolve_github_user(home.path()).unwrap(), None);

        for (text, expected) in [
            (r#"{ "githubUser": "octonato" }"#, Some("octonato")),
            (r#"{ "githubUser": " octonato " }"#, Some("octonato")),
            (r#"{ "githubUser": "  " }"#, None),
            (r#"{ "root": "/x" }"#, None),
        ] {
            write_config(home.path(), text);

            let user = resolve_github_user(home.path()).unwrap();

            assert_eq!(user.as_deref(), expected, "{text}");
        }
    }

    #[test]
    fn bad_github_user_is_an_error() {
        let home = tempfile::tempdir().unwrap();

        for text in [r#"{ "githubUser": 3 }"#, "not json"] {
            let config = write_config(home.path(), text);

            let err = resolve_github_user(home.path()).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
        }
    }

    #[test]
    fn sign_in_hosts_default_to_empty() {
        let home = tempfile::tempdir().unwrap();

        let hosts = resolve_sign_in_hosts(home.path()).unwrap();
        assert_eq!(hosts, Vec::<String>::new());

        write_config(home.path(), r#"{ "root": "/x" }"#);
        let hosts = resolve_sign_in_hosts(home.path()).unwrap();
        assert_eq!(hosts, Vec::<String>::new());
    }

    #[test]
    fn sign_in_hosts_read_hosts_and_wildcards() {
        let home = tempfile::tempdir().unwrap();
        write_config(
            home.path(),
            r#"{ "githubSignInHosts": ["Akka.Okta.com", "*.okta.com"] }"#,
        );

        let hosts = resolve_sign_in_hosts(home.path()).unwrap();

        assert_eq!(hosts, vec!["akka.okta.com", "*.okta.com"]);
    }

    #[test]
    fn bad_sign_in_hosts_are_errors() {
        let home = tempfile::tempdir().unwrap();

        for (value, entry) in [
            (r#""*.okta.com""#, r#""*.okta.com""#),
            ("[3]", "`3`"),
            (r#"["https://a.okta.com"]"#, r#""https://a.okta.com""#),
            (r#"["a.okta.com/x"]"#, r#""a.okta.com/x""#),
            (r#"["a.okta.com:443"]"#, r#""a.okta.com:443""#),
            (r#"["a b.com"]"#, r#""a b.com""#),
            (r#"["a..com"]"#, r#""a..com""#),
            (r#"[""]"#, r#""""#),
            (r#"["a.*.com"]"#, r#""a.*.com""#),
            (r#"["*okta.com"]"#, r#""*okta.com""#),
            (r#"["*."]"#, r#""*.""#),
        ] {
            let text = format!(r#"{{ "githubSignInHosts": {value} }}"#);
            let config = write_config(home.path(), &text);

            let err = resolve_sign_in_hosts(home.path()).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
            assert!(err.reason.contains(entry), "{text}: {err}");
        }
    }

    #[test]
    fn bad_sign_in_hosts_do_not_break_port() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000, "githubSignInHosts": 3 }"#);

        let port = resolve_port_with(home.path(), None).unwrap();

        assert_eq!(port, 9000);
    }

    #[test]
    fn github_repos_default_to_empty() {
        let home = tempfile::tempdir().unwrap();

        let github = resolve_github_repos(home.path()).unwrap();
        assert_eq!(github, BTreeMap::new());

        write_config(home.path(), r#"{ "root": "/x" }"#);
        let github = resolve_github_repos(home.path()).unwrap();
        assert_eq!(github, BTreeMap::new());
    }

    #[test]
    fn github_repos_are_read() {
        let home = tempfile::tempdir().unwrap();
        write_config(
            home.path(),
            r#"{ "github": { "akka/runtime": "lightbend/akka-runtime" } }"#,
        );

        let github = resolve_github_repos(home.path()).unwrap();

        assert_eq!(
            github,
            BTreeMap::from([(
                "akka/runtime".to_string(),
                "lightbend/akka-runtime".to_string()
            )])
        );
    }

    #[test]
    fn bad_github_repos_are_errors() {
        let home = tempfile::tempdir().unwrap();

        for (value, entry) in [
            (r#""x""#, r#""x""#),
            (r#"{ "a": 3 }"#, "`3`"),
            (r#"{ "a": "b/c" }"#, r#""a""#),
            (r#"{ "a/b": "c" }"#, r#""c""#),
            (r#"{ "a/b/c": "d/e" }"#, r#""a/b/c""#),
            (r#"{ "a/b": "/c" }"#, r#""/c""#),
            (r#"{ "a/b": "c/" }"#, r#""c/""#),
        ] {
            let text = format!(r#"{{ "github": {value} }}"#);
            let config = write_config(home.path(), &text);

            let err = resolve_github_repos(home.path()).unwrap_err();

            assert!(
                err.to_string().contains(&config.display().to_string()),
                "{text}: {err}"
            );
            assert!(err.reason.contains(entry), "{text}: {err}");
        }
    }

    #[test]
    fn bad_github_repos_do_not_break_port() {
        let home = tempfile::tempdir().unwrap();
        write_config(home.path(), r#"{ "port": 9000, "github": 3 }"#);

        let port = resolve_port_with(home.path(), None).unwrap();

        assert_eq!(port, 9000);
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

        let port = resolve_port_with(home.path(), None).unwrap();
        assert_eq!(port, 9000);
    }
}
