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

//! Opens a checkout in an editor.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use thiserror::Error;

use crate::config::Editor;
use crate::env::Env;

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("the editor command is empty")]
    EmptyCommand,
    #[error("cannot run {command}: {source}")]
    Spawn { command: String, source: io::Error },
}

/// Starts `editor` with `dir` as its last argument and returns without waiting.
pub fn open_in_editor(editor: &Editor, dir: &Path, env: &Env) -> Result<(), LaunchError> {
    let mut words = editor.command.split_whitespace();
    let program = words.next().ok_or(LaunchError::EmptyCommand)?;

    let mut child = Command::new(program)
        .args(words)
        .arg(dir)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| LaunchError::Spawn {
            command: editor.command.clone(),
            source,
        })?;

    thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::thread;
    use std::time::{Duration, Instant};

    fn editor(command: &str) -> Editor {
        Editor {
            name: "Test".to_string(),
            command: command.to_string(),
        }
    }

    #[test]
    fn passes_dir_as_last_arg() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("args.txt");
        let script = dir.path().join("rec.sh");
        fs::write(
            &script,
            format!("#!/bin/sh\necho \"$@\" > '{}'\n", out.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let env: Env = std::env::vars().collect();

        open_in_editor(
            &editor(&format!("{} -n", script.display())),
            Path::new("/x/y"),
            &env,
        )
        .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while !out.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        thread::sleep(Duration::from_millis(50));
        assert_eq!(fs::read_to_string(&out).unwrap().trim(), "-n /x/y");
    }

    #[test]
    fn finds_the_program_through_the_env_path() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("args.txt");
        let script = dir.path().join("fake-editor");
        fs::write(
            &script,
            format!("#!/bin/sh\necho \"$@\" > '{}'\n", out.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let env = Env::from([("PATH".to_string(), dir.path().display().to_string())]);

        open_in_editor(&editor("fake-editor"), Path::new("/x/y"), &env).unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while !out.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        thread::sleep(Duration::from_millis(50));
        assert_eq!(fs::read_to_string(&out).unwrap().trim(), "/x/y");
    }

    #[test]
    fn empty_command_is_error() {
        let result = open_in_editor(&editor("  "), Path::new("/x"), &Env::new());

        assert!(matches!(result, Err(LaunchError::EmptyCommand)));
    }

    #[test]
    fn missing_program_is_spawn_error() {
        let env: Env = std::env::vars().collect();

        let result = open_in_editor(&editor("no-such-editor-xyz"), Path::new("/x"), &env);

        match result {
            Err(LaunchError::Spawn { command, .. }) => assert_eq!(command, "no-such-editor-xyz"),
            other => panic!("expected a spawn error, got {other:?}"),
        }
    }
}
