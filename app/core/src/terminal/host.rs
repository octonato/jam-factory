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

//! Attaches PTYs to tmux sessions and streams their output.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use thiserror::Error;

use super::tmux::{Tmux, TmuxError};

pub type TerminalId = u64;

/// An event from an attached terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    Output(Vec<u8>),
    Exited(Option<u32>),
}

pub type Sink = Box<dyn Fn(TerminalEvent) + Send + 'static>;

/// What to attach to, and the starting terminal size.
#[derive(Debug, Clone)]
pub struct AttachRequest {
    pub name: String,
    pub dir: PathBuf,
    pub program: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("pty error: {0}")]
    Pty(String),
    #[error("no terminal with id {0}")]
    Unknown(TerminalId),
    #[error(transparent)]
    Tmux(#[from] TmuxError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Creates, controls, and kills tmux sessions and the terminals attached to them.
pub trait SessionHost: Send + Sync {
    fn attach(&self, req: AttachRequest, sink: Sink) -> Result<TerminalId, TerminalError>;
    fn write(&self, id: TerminalId, bytes: &[u8]) -> Result<(), TerminalError>;
    fn resize(&self, id: TerminalId, cols: u16, rows: u16) -> Result<(), TerminalError>;
    fn detach(&self, id: TerminalId) -> Result<(), TerminalError>;
    fn kill(&self, name: &str) -> Result<(), TerminalError>;
    fn live_sessions(&self) -> Result<Vec<String>, TerminalError>;
}

type SharedChild = Arc<Mutex<Box<dyn Child + Send + Sync>>>;

struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: SharedChild,
}

type Terminals = Arc<Mutex<HashMap<TerminalId, PtyHandle>>>;

/// A `SessionHost` that runs one `tmux new-session -A` client per terminal.
pub struct TmuxHost {
    tmux: Tmux,
    next_id: AtomicU64,
    terminals: Terminals,
}

impl TmuxHost {
    pub fn new(tmux: Tmux) -> Self {
        Self {
            tmux,
            next_id: AtomicU64::new(1),
            terminals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn with_handle<T>(
        &self,
        id: TerminalId,
        f: impl FnOnce(&mut PtyHandle) -> Result<T, TerminalError>,
    ) -> Result<T, TerminalError> {
        let mut terminals = self.terminals.lock().unwrap();
        let handle = terminals.get_mut(&id).ok_or(TerminalError::Unknown(id))?;
        f(handle)
    }
}

impl SessionHost for TmuxHost {
    fn attach(&self, req: AttachRequest, sink: Sink) -> Result<TerminalId, TerminalError> {
        let pair = native_pty_system()
            .openpty(size(req.cols, req.rows))
            .map_err(pty_error)?;

        let argv = self
            .tmux
            .attach_args(&req.name, &req.dir, req.program.as_deref());
        let mut cmd = CommandBuilder::from_argv(argv.into_iter().map(Into::into).collect());
        cmd.env_clear();
        for (key, value) in self.tmux.client_env() {
            cmd.env(key, value);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.cwd(&req.dir);

        let child = pair.slave.spawn_command(cmd).map_err(pty_error)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().map_err(pty_error)?;
        let writer = pair.master.take_writer().map_err(pty_error)?;
        let child: SharedChild = Arc::new(Mutex::new(child));

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.terminals.lock().unwrap().insert(
            id,
            PtyHandle {
                master: pair.master,
                writer,
                child: child.clone(),
            },
        );

        let terminals = self.terminals.clone();
        thread::spawn(move || pump(id, reader, child, terminals, sink));

        Ok(id)
    }

    fn write(&self, id: TerminalId, bytes: &[u8]) -> Result<(), TerminalError> {
        self.with_handle(id, |h| {
            h.writer.write_all(bytes)?;
            h.writer.flush()?;
            Ok(())
        })
    }

    fn resize(&self, id: TerminalId, cols: u16, rows: u16) -> Result<(), TerminalError> {
        self.with_handle(id, |h| h.master.resize(size(cols, rows)).map_err(pty_error))
    }

    fn detach(&self, id: TerminalId) -> Result<(), TerminalError> {
        let handle = self
            .terminals
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or(TerminalError::Unknown(id))?;

        handle.child.lock().unwrap().kill()?;
        Ok(())
    }

    fn kill(&self, name: &str) -> Result<(), TerminalError> {
        Ok(self.tmux.kill_session(name)?)
    }

    fn live_sessions(&self) -> Result<Vec<String>, TerminalError> {
        Ok(self.tmux.list_sessions()?)
    }
}

fn pump(
    id: TerminalId,
    mut reader: Box<dyn Read + Send>,
    child: SharedChild,
    terminals: Terminals,
    sink: Sink,
) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => sink(TerminalEvent::Output(buf[..n].to_vec())),
        }
    }

    terminals.lock().unwrap().remove(&id);
    sink(TerminalEvent::Exited(exit_code(&child)));
}

fn exit_code(child: &SharedChild) -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.lock().unwrap().try_wait() {
            return Some(status.exit_code());
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn pty_error(e: impl std::fmt::Display) -> TerminalError {
    TerminalError::Pty(e.to_string())
}
