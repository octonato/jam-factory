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

//! A store that calls the tools of a `jam-factory-mcp` child process.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use jam_factory_store::{FileRead, SessionEntry, SessionRef, StoreError};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use rmcp::service::{RunningService, ServiceError};
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tokio::io::{self, AsyncReadExt};
use tokio::process::{ChildStderr, Command};
use tokio::runtime::Runtime;
use tokio::time::timeout;

use crate::store::Store;

type Client = RunningService<RoleClient, ()>;

/// The longest wait for the stderr of a server child that failed to start.
const STDERR_TIMEOUT: Duration = Duration::from_secs(2);

/// A server child and the MCP client on its stdio.
struct Child {
    client: Arc<Client>,
    pid: Option<u32>,
}

/// A store backed by a `jam-factory-mcp` child, respawned once when a call finds it dead.
pub struct McpStore {
    program: PathBuf,
    home: Option<PathBuf>,
    runtime: Runtime,
    child: Mutex<Child>,
}

impl McpStore {
    /// Spawns `program` and connects to it.
    pub fn spawn(program: PathBuf) -> Result<McpStore, String> {
        McpStore::spawn_with_home(program, None)
    }

    /// Spawns `program`, with `HOME` set to `home` when given, and connects to it.
    fn spawn_with_home(program: PathBuf, home: Option<PathBuf>) -> Result<McpStore, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("jam-board-mcp")
            .enable_all()
            .build()
            .map_err(|e| format!("Cannot start the MCP client: {e}"))?;

        let child = runtime.block_on(connect(&program, home.as_deref()))?;

        Ok(McpStore {
            program,
            home,
            runtime,
            child: Mutex::new(child),
        })
    }

    /// The process id of the current server child.
    pub fn pid(&self) -> Option<u32> {
        self.child().pid
    }

    fn child(&self) -> MutexGuard<'_, Child> {
        self.child.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Calls the tool `name` with `arguments`, and retries once on a new child when the transport fails.
    fn call(&self, name: &'static str, arguments: Map<String, Value>) -> Result<Value, StoreError> {
        let client = Arc::clone(&self.child().client);

        let result = match self.call_on(&client, name, arguments.clone()) {
            Err(error) if transport_failed(&error) => {
                let client = self.respawn(&client)?;
                self.call_on(&client, name, arguments)
            }
            result => result,
        };

        let result = result.map_err(|e| StoreError::Io(format!("The {name} call failed: {e}")))?;

        reply(result)
    }

    fn call_on(
        &self,
        client: &Client,
        name: &'static str,
        arguments: Map<String, Value>,
    ) -> Result<CallToolResult, ServiceError> {
        let params = CallToolRequestParams::new(name).with_arguments(arguments);

        self.runtime.block_on(client.call_tool(params))
    }

    /// Replaces the child of the failed client `dead`, unless another call already did.
    fn respawn(&self, dead: &Arc<Client>) -> Result<Arc<Client>, StoreError> {
        let mut child = self.child();

        if Arc::ptr_eq(&child.client, dead) {
            let connected = connect(&self.program, self.home.as_deref());
            *child = self.runtime.block_on(connected).map_err(StoreError::Io)?;
        }

        Ok(Arc::clone(&child.client))
    }
}

impl Drop for McpStore {
    fn drop(&mut self) {
        let child = self.child.get_mut().unwrap_or_else(PoisonError::into_inner);

        if let Some(client) = Arc::get_mut(&mut child.client) {
            let _ = self.runtime.block_on(client.close());
        }
    }
}

impl Store for McpStore {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>, StoreError> {
        let mut reply = self.call("list_sessions", Map::new())?;

        field(&mut reply, "sessions")
    }

    fn list_files(&self, s: &SessionRef) -> Result<Vec<String>, StoreError> {
        let mut reply = self.call("list_files", session_args(s))?;

        field(&mut reply, "files")
    }

    fn read_file(&self, s: &SessionRef, path: &str) -> Result<FileRead, StoreError> {
        let mut arguments = session_args(s);
        arguments.insert("path".to_string(), Value::from(path));

        let mut reply = self.call("read_file", arguments)?;

        Ok(FileRead {
            content: field(&mut reply, "content")?,
            version: field(&mut reply, "version")?,
        })
    }

    fn archive_session(&self, s: &SessionRef) -> Result<(), StoreError> {
        self.call("archive_session", session_args(s))?;

        Ok(())
    }
}

/// Spawns `program`, with `HOME` set to `home` when given, and initializes an MCP client on its stdio.
async fn connect(program: &Path, home: Option<&Path>) -> Result<Child, String> {
    let mut command = Command::new(program);
    command.kill_on_drop(true);

    if let Some(home) = home {
        command.env("HOME", home);
    }

    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot start {}: {e}", program.display()))?;
    let pid = transport.id();

    let client = match ().serve(transport).await {
        Ok(client) => client,
        Err(e) => {
            let error = format!("Cannot connect to {}: {e}", program.display());
            let reason = read_stderr(stderr).await;

            return Err(if reason.is_empty() {
                error
            } else {
                format!("{error}\n\n{reason}")
            });
        }
    };

    if let Some(stderr) = stderr {
        tokio::spawn(forward_stderr(stderr));
    }

    Ok(Child {
        client: Arc::new(client),
        pid,
    })
}

/// The trimmed text a stopped server child wrote on `stderr`.
async fn read_stderr(stderr: Option<ChildStderr>) -> String {
    let mut bytes = Vec::new();

    if let Some(mut stderr) = stderr {
        let _ = timeout(STDERR_TIMEOUT, stderr.read_to_end(&mut bytes)).await;
    }

    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// Copies the stderr of a running server child to the board's stderr.
async fn forward_stderr(mut stderr: ChildStderr) {
    let _ = io::copy(&mut stderr, &mut io::stderr()).await;
}

/// Whether `error` means the child is gone.
fn transport_failed(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::TransportClosed | ServiceError::TransportSend(_)
    )
}

/// The `org`, `repo` and `session` arguments that name `s`.
fn session_args(s: &SessionRef) -> Map<String, Value> {
    [
        ("org", s.org()),
        ("repo", s.repo()),
        ("session", s.session()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), Value::from(value)))
    .collect()
}

/// The JSON reply of `result`, or the store error it holds.
fn reply(result: CallToolResult) -> Result<Value, StoreError> {
    let text = result
        .content
        .first()
        .and_then(ContentBlock::as_text)
        .map(|content| content.text.as_str())
        .unwrap_or_default();

    let failed = result.is_error == Some(true);

    match serde_json::from_str::<Value>(text) {
        Ok(value) if !failed => Ok(value),
        Ok(error) => Err(store_error(&error)),
        Err(_) if failed => Err(StoreError::Io(text.to_string())),
        Err(e) => Err(StoreError::Io(format!("The reply is not JSON: {e}"))),
    }
}

/// The store error that the JSON error reply `error` names.
fn store_error(error: &Value) -> StoreError {
    let code = error["code"].as_str().unwrap_or_default();
    let message = error["message"].as_str().unwrap_or_default();
    let current = error["current_version"].as_str().map(str::to_string);

    StoreError::from_code(code, message, current)
}

/// Takes the field `name` out of `reply` and decodes it.
fn field<T: DeserializeOwned>(reply: &mut Value, name: &str) -> Result<T, StoreError> {
    let value = reply.get_mut(name).map(Value::take).unwrap_or_default();

    serde_json::from_value(value)
        .map_err(|e| StoreError::Io(format!("The reply has no valid {name:?}: {e}")))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    use jam_factory_store::root;
    use serde_json::json;

    use super::*;

    const DEATH_TIMEOUT: Duration = Duration::from_secs(10);

    /// The `jam-factory-mcp` binary of the workspace debug build, if it is built.
    fn server_binary() -> Option<PathBuf> {
        let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug/jam-factory-mcp");

        binary.is_file().then_some(binary)
    }

    /// A temp folder holding `server`, a script that logs each start to the file `$starts` and runs `body`.
    struct FakeServer {
        dir: tempfile::TempDir,
    }

    impl FakeServer {
        fn new(body: &str) -> FakeServer {
            use std::os::unix::fs::PermissionsExt;

            let dir = tempfile::tempdir().unwrap();
            let starts = dir.path().join("starts");
            let script = format!(
                "#!/bin/sh\nstarts='{}'\necho start >> \"$starts\"\n{body}",
                starts.display()
            );

            let server = dir.path().join("server");
            fs::write(&server, script).unwrap();
            fs::set_permissions(&server, fs::Permissions::from_mode(0o755)).unwrap();

            FakeServer { dir }
        }

        fn program(&self) -> PathBuf {
            self.dir.path().join("server")
        }

        fn starts(&self) -> usize {
            fs::read_to_string(self.dir.path().join("starts"))
                .unwrap_or_default()
                .lines()
                .count()
        }

        fn spawn(&self) -> McpStore {
            let home = self.dir.path().join("home");
            McpStore::spawn_with_home(self.program(), Some(home)).unwrap()
        }
    }

    /// Kills the child of `store` and waits until its client sees it gone.
    fn kill_child(store: &McpStore) {
        let pid = store.pid().unwrap().to_string();
        let killed = Command::new("kill").args(["-9", &pid]).status().unwrap();
        assert!(killed.success());

        let client = Arc::clone(&store.child().client);
        let start = Instant::now();

        while !client.is_transport_closed() {
            assert!(start.elapsed() < DEATH_TIMEOUT, "the child did not die");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn text_result(text: &str, is_error: bool) -> CallToolResult {
        let content = vec![ContentBlock::text(text.to_string())];

        if is_error {
            CallToolResult::error(content)
        } else {
            CallToolResult::success(content)
        }
    }

    #[test]
    fn mcp_store_respawns_once() {
        let Some(binary) = server_binary() else {
            eprintln!("skipped: jam-factory-mcp is not built");
            return;
        };

        let server = FakeServer::new(&format!("exec '{}'\n", binary.display()));
        let store = server.spawn();
        let first = store.pid();

        kill_child(&store);

        assert_eq!(store.list_sessions(), Ok(Vec::new()));
        assert_ne!(store.pid(), first);
        assert_eq!(server.starts(), 2);
    }

    #[test]
    fn a_failed_respawn_is_an_error() {
        let Some(binary) = server_binary() else {
            eprintln!("skipped: jam-factory-mcp is not built");
            return;
        };

        let body = format!(
            "if [ \"$(wc -l < \"$starts\")\" -gt 1 ]; then exit 1; fi\nexec '{}'\n",
            binary.display()
        );
        let server = FakeServer::new(&body);
        let store = server.spawn();

        kill_child(&store);

        let error = store.list_sessions().unwrap_err();

        assert_eq!(error.code(), "io");
        assert_eq!(server.starts(), 2);
    }

    #[test]
    fn missing_program_fails_to_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("jam-factory-mcp");

        let error = McpStore::spawn(program.clone()).err().unwrap();

        assert!(error.contains(&program.display().to_string()), "{error}");
    }

    #[test]
    fn a_bad_config_error_holds_the_server_reason() {
        let Some(binary) = server_binary() else {
            eprintln!("skipped: jam-factory-mcp is not built");
            return;
        };

        let home = tempfile::tempdir().unwrap();
        let config_dir = home.path().join(root::CONFIG_DIR);
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join(root::CONFIG_FILE), r#"{ "root": 3 }"#).unwrap();

        let reason = root::resolve(home.path()).unwrap_err().to_string();
        let error = McpStore::spawn_with_home(binary, Some(home.path().to_path_buf()))
            .err()
            .unwrap();

        assert!(error.contains(&reason), "{error}");
    }

    #[test]
    fn tool_errors_map_to_their_store_error() {
        let conflict = json!({ "code": "conflict", "message": "stale", "current_version": "abc" });
        let not_found = json!({ "code": "not_found", "message": "gone", "current_version": null });

        assert_eq!(
            reply(text_result(&conflict.to_string(), true)),
            Err(StoreError::Conflict {
                current: "abc".to_string()
            })
        );
        assert_eq!(
            reply(text_result(&not_found.to_string(), true)),
            Err(StoreError::NotFound("gone".to_string()))
        );
        assert_eq!(
            reply(text_result("bad arguments", true)),
            Err(StoreError::Io("bad arguments".to_string()))
        );
        assert_eq!(
            reply(text_result(r#"{"files":[]}"#, false)),
            Ok(json!({ "files": [] }))
        );
    }
}
