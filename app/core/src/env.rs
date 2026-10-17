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

//! Reads the environment of the user's login shell.

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::config::Config;

pub type Env = HashMap<String, String>;

const MARKER: &[u8] = b"__ODECK_ENV__";

const DRAIN_IDLE: Duration = Duration::from_millis(200);

#[derive(Debug, Error)]
pub enum EnvError {
    #[error("cannot run {shell}: {source}")]
    Spawn { shell: String, source: io::Error },
    #[error("{0} did not finish in time")]
    Timeout(String),
    #[error("{0} printed no environment")]
    Empty(String),
}

/// Reads the `\0`-separated `KEY=value` pairs printed after the marker.
pub fn parse_env_dump(output: &[u8]) -> Env {
    let Some(start) = find(output, MARKER) else {
        return Env::new();
    };

    output[start + MARKER.len()..]
        .split(|b| *b == 0)
        .filter_map(|entry| {
            let entry = String::from_utf8_lossy(entry);
            let (key, value) = entry.split_once('=')?;
            (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}

/// Runs `shell` as an interactive login shell and returns its environment.
pub fn login_env(shell: &Path, timeout: Duration) -> Result<Env, EnvError> {
    let name = shell.display().to_string();

    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", "printf __ODECK_ENV__; env -0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| EnvError::Spawn {
            shell: name.clone(),
            source,
        })?;

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (chunks, received) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || chunks.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EnvError::Timeout(name));
        }
        thread::sleep(Duration::from_millis(20));
    }

    let mut output = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match received.recv_timeout(left.min(DRAIN_IDLE)) {
            Ok(chunk) => output.extend(chunk),
            Err(_) => break,
        }
    }

    let env = parse_env_dump(&output);
    if env.is_empty() {
        return Err(EnvError::Empty(name));
    }

    Ok(env)
}

/// Returns `GITHUB_SRC` from `env`, or else `github_src` from `config`.
pub fn resolve_github_src(env: &Env, config: &Config) -> Option<PathBuf> {
    env.get("GITHUB_SRC")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| config.github_src.clone())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn ignores_output_before_marker() {
        let env = parse_env_dump(b"motd junk\n__ODECK_ENV__PATH=/a:/b\0HOME=/h\0");

        assert_eq!(env.get("PATH").map(String::as_str), Some("/a:/b"));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/h"));
        assert!(env.keys().all(|k| !k.contains("motd")));
    }

    #[test]
    fn keeps_values_with_equals_and_newlines() {
        let env = parse_env_dump(b"__ODECK_ENV__X=a=b\nc\0");

        assert_eq!(env.get("X").map(String::as_str), Some("a=b\nc"));
    }

    #[test]
    fn missing_marker_gives_empty_env() {
        let env = parse_env_dump(b"PATH=/a\0");

        assert!(env.is_empty());
    }

    #[test]
    fn github_src_prefers_env_over_config() {
        let env = Env::from([("GITHUB_SRC".to_string(), "/from/env".to_string())]);
        let config = Config {
            github_src: Some(PathBuf::from("/from/config")),
            ..Config::default()
        };

        assert_eq!(
            resolve_github_src(&env, &config),
            Some(PathBuf::from("/from/env"))
        );
    }

    #[test]
    fn github_src_falls_back_to_config() {
        let config = Config {
            github_src: Some(PathBuf::from("/from/config")),
            ..Config::default()
        };

        assert_eq!(
            resolve_github_src(&Env::new(), &config),
            Some(PathBuf::from("/from/config"))
        );
    }

    #[test]
    fn github_src_none_when_neither() {
        assert_eq!(resolve_github_src(&Env::new(), &Config::default()), None);
    }

    #[test]
    fn login_env_returns_when_a_background_child_keeps_stdout_open() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("fake-shell");
        std::fs::write(
            &shell,
            "#!/bin/sh\nprintf '__ODECK_ENV__PATH=/x\\0'\nsleep 8 &\n",
        )
        .unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();

        let started = Instant::now();
        let env = login_env(&shell, Duration::from_secs(2)).unwrap();

        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(env.get("PATH").map(String::as_str), Some("/x"));
    }

    #[test]
    fn login_env_reads_real_shell() {
        let env = login_env(Path::new("/bin/sh"), Duration::from_secs(5)).unwrap();

        assert!(env.contains_key("PATH"));
    }
}
