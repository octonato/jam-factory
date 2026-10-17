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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use odeck_core::env::Env;
use odeck_core::terminal::{AttachRequest, SessionHost, TerminalEvent, TerminalId, Tmux, TmuxHost};

const PROGRAM: &str = "echo odeck-ready; exec cat";

struct Fixture {
    host: TmuxHost,
    tmux: Tmux,
    _dir: tempfile::TempDir,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let mut cmd = Command::new("tmux");
        cmd.env_clear().envs(self.tmux.client_env());
        if let Some(socket) = &self.tmux.socket {
            cmd.args(["-L", socket]);
        }
        let _ = cmd.arg("kill-server").output();
    }
}

#[derive(Clone, Default)]
struct Recorder {
    output: Arc<Mutex<Vec<u8>>>,
    exited: Arc<Mutex<bool>>,
}

impl Recorder {
    fn sink(&self) -> Box<dyn Fn(TerminalEvent) + Send> {
        let me = self.clone();
        Box::new(move |event| match event {
            TerminalEvent::Output(bytes) => me.output.lock().unwrap().extend(bytes),
            TerminalEvent::Exited(_) => *me.exited.lock().unwrap() = true,
        })
    }

    fn output_contains(&self, text: &str) -> bool {
        String::from_utf8_lossy(&self.output.lock().unwrap()).contains(text)
    }

    fn exited(&self) -> bool {
        *self.exited.lock().unwrap()
    }
}

fn fixture(label: &str, extra_env: &[(&str, &str)]) -> Option<Fixture> {
    let dir = tempfile::tempdir().unwrap();
    let mut env: Env = std::env::vars().collect();
    env.insert("TMUX_TMPDIR".to_string(), dir.path().display().to_string());
    for (k, v) in extra_env {
        env.insert(k.to_string(), v.to_string());
    }

    let tmux = Tmux {
        socket: Some(format!("odeck-host-{}-{label}", std::process::id())),
        env,
    };
    if !tmux.is_available() {
        eprintln!("tmux not found, skipping");
        return None;
    }

    Some(Fixture {
        host: TmuxHost::new(tmux.clone()),
        tmux,
        _dir: dir,
    })
}

fn attach(f: &Fixture, name: &str, recorder: &Recorder) -> TerminalId {
    let request = AttachRequest {
        name: name.to_string(),
        dir: std::env::temp_dir(),
        program: Some(PROGRAM.to_string()),
        cols: 80,
        rows: 24,
    };
    f.host.attach(request, recorder.sink()).unwrap()
}

fn wait_for(condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn attach_streams_output() {
    let Some(f) = fixture("stream", &[]) else {
        return;
    };
    let recorder = Recorder::default();

    attach(&f, "odeck/a/b/c/shell", &recorder);

    assert!(wait_for(|| recorder.output_contains("odeck-ready")));
}

#[test]
fn write_reaches_session() {
    let Some(f) = fixture("write", &[]) else {
        return;
    };
    let recorder = Recorder::default();
    let id = attach(&f, "odeck/a/b/c/shell", &recorder);
    assert!(wait_for(|| recorder.output_contains("odeck-ready")));

    f.host.write(id, b"ping\r").unwrap();

    assert!(wait_for(|| recorder.output_contains("ping")));
}

#[test]
fn resize_succeeds() {
    let Some(f) = fixture("resize", &[]) else {
        return;
    };
    let recorder = Recorder::default();
    let id = attach(&f, "odeck/a/b/c/shell", &recorder);

    f.host.resize(id, 120, 40).unwrap();
}

#[test]
fn detach_keeps_session() {
    let Some(f) = fixture("detach", &[]) else {
        return;
    };
    let recorder = Recorder::default();
    let name = "odeck/a/b/c/shell";
    let id = attach(&f, name, &recorder);
    assert!(wait_for(|| recorder.output_contains("odeck-ready")));

    f.host.detach(id).unwrap();

    assert!(wait_for(|| recorder.exited()));
    assert!(f.host.live_sessions().unwrap().contains(&name.to_string()));
}

#[test]
fn kill_ends_session() {
    let Some(f) = fixture("kill", &[]) else {
        return;
    };
    let recorder = Recorder::default();
    let name = "odeck/a/b/c/shell";
    attach(&f, name, &recorder);
    assert!(wait_for(|| recorder.output_contains("odeck-ready")));

    f.host.kill(name).unwrap();

    assert!(!f.host.live_sessions().unwrap().contains(&name.to_string()));
}

#[test]
fn attach_works_inside_tmux() {
    let Some(f) = fixture("nested", &[("TMUX", "/tmp/fake,1,0")]) else {
        return;
    };
    let recorder = Recorder::default();

    attach(&f, "odeck/a/b/c/shell", &recorder);

    assert!(wait_for(|| recorder.output_contains("odeck-ready")));
}

#[test]
fn unknown_terminal_is_error() {
    let Some(f) = fixture("unknown", &[]) else {
        return;
    };

    assert!(f.host.write(999, b"x").is_err());
}
