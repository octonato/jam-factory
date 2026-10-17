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

//! Runs tmux commands.

use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use thiserror::Error;

use crate::env::Env;

#[derive(Debug, Error)]
pub enum TmuxError {
    #[error("cannot run tmux: {0}")]
    Io(#[from] io::Error),
    #[error("tmux failed: {0}")]
    Failed(String),
}

/// Holds the tmux socket name and the env for every tmux call.
#[derive(Debug, Clone)]
pub struct Tmux {
    pub socket: Option<String>,
    pub env: Env,
}

impl Tmux {
    /// Returns true when `tmux -V` runs.
    pub fn is_available(&self) -> bool {
        self.command()
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// The argv that creates session `name` in `dir`, or attaches to it when it exists.
    pub fn attach_args(&self, name: &str, dir: &Path, program: Option<&str>) -> Vec<String> {
        let mut args = vec!["tmux".to_string()];
        if let Some(socket) = &self.socket {
            args.extend(["-L".to_string(), socket.clone()]);
        }

        args.extend(
            ["new-session", "-A", "-s", name, "-c"]
                .iter()
                .map(|s| s.to_string()),
        );
        args.push(dir.display().to_string());
        if let Some(program) = program {
            args.push(program.to_string());
        }

        args
    }

    /// The names of all sessions. No running server gives an empty list.
    pub fn list_sessions(&self) -> Result<Vec<String>, TmuxError> {
        let output = self
            .command()
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()?;

        if !output.status.success() {
            let stderr = stderr(&output);
            if is_no_server(&stderr) {
                return Ok(Vec::new());
            }
            return Err(TmuxError::Failed(stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    /// Ends session `name`.
    pub fn kill_session(&self, name: &str) -> Result<(), TmuxError> {
        let output = self
            .command()
            .args(["kill-session", "-t", &format!("={name}")])
            .output()?;

        if !output.status.success() {
            return Err(TmuxError::Failed(stderr(&output)));
        }

        Ok(())
    }

    /// The env a tmux client process runs with.
    pub fn client_env(&self) -> Env {
        let mut env = self.env.clone();
        env.remove("TMUX");
        env
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new("tmux");
        cmd.env_clear().envs(self.client_env());
        if let Some(socket) = &self.socket {
            cmd.args(["-L", socket]);
        }
        cmd
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

fn is_no_server(stderr: &str) -> bool {
    [
        "no server running",
        "error connecting",
        "server exited unexpectedly",
    ]
    .iter()
    .any(|m| stderr.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_no_server_messages() {
        assert!(is_no_server("no server running on /tmp/tmux-502/default"));
        assert!(is_no_server(
            "error connecting to /tmp/tmux-502/default (No such file or directory)"
        ));
        assert!(is_no_server("server exited unexpectedly"));
        assert!(!is_no_server("can't find session: x"));
    }

    #[test]
    fn looks_up_tmux_through_its_env_path() {
        let tmux = Tmux {
            socket: None,
            env: Env::from([("PATH".to_string(), "/nonexistent".to_string())]),
        };

        assert!(!tmux.is_available());
    }
}
