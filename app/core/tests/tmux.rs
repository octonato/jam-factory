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

use std::process::Command;

use odeck_core::env::Env;
use odeck_core::terminal::Tmux;

struct TestTmux {
    tmux: Tmux,
    _dir: tempfile::TempDir,
}

impl Drop for TestTmux {
    fn drop(&mut self) {
        let _ = raw(&self.tmux).arg("kill-server").output();
    }
}

fn test_tmux(label: &str) -> Option<TestTmux> {
    let dir = tempfile::tempdir().unwrap();
    let mut env: Env = std::env::vars().collect();
    env.insert("TMUX_TMPDIR".to_string(), dir.path().display().to_string());

    let tmux = Tmux {
        socket: Some(format!("odeck-test-{}-{label}", std::process::id())),
        env,
    };
    if !tmux.is_available() {
        eprintln!("tmux not found, skipping");
        return None;
    }

    Some(TestTmux { tmux, _dir: dir })
}

fn raw(tmux: &Tmux) -> Command {
    let mut cmd = Command::new("tmux");
    cmd.env_clear().envs(&tmux.env).env_remove("TMUX");
    if let Some(socket) = &tmux.socket {
        cmd.args(["-L", socket]);
    }
    cmd
}

#[test]
fn create_list_kill() {
    let Some(t) = test_tmux("cl") else { return };
    let name = "odeck/a/b/c_d/shell";
    let created = raw(&t.tmux)
        .args(["new-session", "-d", "-s", name])
        .status()
        .unwrap();
    assert!(created.success());

    assert!(t.tmux.list_sessions().unwrap().contains(&name.to_string()));

    t.tmux.kill_session(name).unwrap();

    assert!(!t.tmux.list_sessions().unwrap().contains(&name.to_string()));
}

#[test]
fn list_without_server_is_empty() {
    let Some(t) = test_tmux("empty") else { return };

    assert!(t.tmux.list_sessions().unwrap().is_empty());
}

#[test]
fn attach_args_build_new_session_command() {
    let tmux = Tmux {
        socket: Some("sock".to_string()),
        env: Env::new(),
    };

    let args = tmux.attach_args(
        "odeck/a/b/c/claude",
        std::path::Path::new("/x"),
        Some("claude"),
    );

    assert_eq!(
        args,
        vec![
            "tmux",
            "-L",
            "sock",
            "new-session",
            "-A",
            "-s",
            "odeck/a/b/c/claude",
            "-c",
            "/x",
            "claude"
        ]
    );
}
