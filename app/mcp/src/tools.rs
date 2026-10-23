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

//! The MCP tools, each one store call.

use std::sync::Arc;

use jam_factory_store::{SessionFilter, SessionRef, Store, StoreError};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// A session named by its org, its repo and its id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionArgs {
    /// The org name.
    org: String,

    /// The repo name.
    repo: String,

    /// The session id.
    session: String,
}

/// The arguments of `list_sessions`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSessionsArgs {
    /// Lists only the sessions of this org.
    org: Option<String>,

    /// Lists only the sessions of this repo.
    repo: Option<String>,

    /// Lists the archived sessions too.
    #[serde(default)]
    include_archived: bool,
}

/// The arguments of `read_file`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,
}

/// The arguments of `write_file`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteFileArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,

    /// The whole new content of the file.
    content: String,

    /// The current version of the file. Leave it out to create a new file.
    expected_version: Option<String>,
}

/// The arguments of `edit_file`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditFileArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,

    /// The text to replace. It must occur exactly once in the file.
    old_str: String,

    /// The text that replaces `old_str`.
    new_str: String,
}

/// The arguments of `append_file`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendFileArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,

    /// The text to add at the end of the file.
    content: String,
}

/// The arguments of `move_file`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveFileArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file to move, relative to the session folder.
    from: String,

    /// The new path of the file, relative to the session folder.
    to: String,
}

/// The arguments of `delete_file`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteFileArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,

    /// The current version of the file.
    expected_version: String,
}

/// The arguments of `update_session`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSessionArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The fields to merge into `session.json`.
    fields: Map<String, Value>,
}

impl SessionArgs {
    fn session_ref(&self) -> Result<SessionRef, StoreError> {
        SessionRef::new(&self.org, &self.repo, &self.session)
    }
}

/// The MCP server of the factory folder.
pub struct FactoryServer {
    store: Arc<Store>,
    tool_router: ToolRouter<FactoryServer>,
}

impl FactoryServer {
    pub fn new(store: Store) -> FactoryServer {
        FactoryServer {
            store: Arc::new(store),
            tool_router: FactoryServer::tool_router(),
        }
    }

    /// Runs `call` on the store and returns its reply or error as a tool result.
    async fn run<F>(&self, call: F) -> CallToolResult
    where
        F: FnOnce(&Store) -> Result<Value, StoreError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);

        let result = tokio::task::spawn_blocking(move || call(&store))
            .await
            .unwrap_or_else(|e| Err(StoreError::Io(e.to_string())));

        match result {
            Ok(reply) => CallToolResult::success(vec![ContentBlock::text(reply.to_string())]),
            Err(error) => {
                CallToolResult::error(vec![ContentBlock::text(error_reply(&error).to_string())])
            }
        }
    }
}

#[tool_router]
impl FactoryServer {
    #[tool(
        description = "Lists sessions, each with `org`, `repo`, `session` and its parsed `session.json`."
    )]
    async fn list_sessions(
        &self,
        Parameters(args): Parameters<ListSessionsArgs>,
    ) -> CallToolResult {
        let filter = SessionFilter {
            org: args.org,
            repo: args.repo,
            include_archived: args.include_archived,
        };

        self.run(move |store| {
            let sessions = store.list_sessions(&filter)?;
            Ok(json!({ "sessions": sessions }))
        })
        .await
    }

    #[tool(description = "Lists every file in a session folder, recursively, as relative paths.")]
    async fn list_files(&self, Parameters(args): Parameters<SessionArgs>) -> CallToolResult {
        self.run(move |store| {
            let files = store.list_files(&args.session_ref()?)?;
            Ok(json!({ "files": files }))
        })
        .await
    }

    #[tool(
        description = "Returns the content and its version. The version is the lowercase hex SHA-256 of the file."
    )]
    async fn read_file(&self, Parameters(args): Parameters<ReadFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let read = store.read_file(&s, &args.path)?;
            Ok(json!({ "content": read.content, "version": read.version }))
        })
        .await
    }

    #[tool(description = "Writes the whole file.")]
    async fn write_file(&self, Parameters(args): Parameters<WriteFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let expected = args.expected_version.as_deref();
            let version = store.write_file(&s, &args.path, &args.content, expected)?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(description = "Replaces `old_str` with `new_str`.")]
    async fn edit_file(&self, Parameters(args): Parameters<EditFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let version = store.edit_file(&s, &args.path, &args.old_str, &args.new_str)?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(description = "Appends `content` to the end of the file. Creates the file if missing.")]
    async fn append_file(&self, Parameters(args): Parameters<AppendFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let version = store.append_file(&s, &args.path, &args.content)?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(description = "Renames a file inside one session folder.")]
    async fn move_file(&self, Parameters(args): Parameters<MoveFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let version = store.move_file(&s, &args.from, &args.to)?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(description = "Deletes a file whose current version matches `expected_version`.")]
    async fn delete_file(&self, Parameters(args): Parameters<DeleteFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            store.delete_file(&s, &args.path, &args.expected_version)?;
            Ok(json!({}))
        })
        .await
    }

    #[tool(
        description = "Merges fields into an existing `session.json`. Sets `updated` to the current UTC time."
    )]
    async fn update_session(
        &self,
        Parameters(args): Parameters<UpdateSessionArgs>,
    ) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let version = store.update_session(&s, &args.fields)?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(description = "Sets `archived: true` and `updated` in `session.json`.")]
    async fn archive_session(&self, Parameters(args): Parameters<SessionArgs>) -> CallToolResult {
        self.run(move |store| {
            let version = store.archive_session(&args.session_ref()?)?;
            Ok(json!({ "version": version }))
        })
        .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FactoryServer {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        let implementation = Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

        ServerConfig::new(capabilities).with_server_info(implementation)
    }
}

/// The JSON error reply of `error`.
fn error_reply(error: &StoreError) -> Value {
    let current_version = match error {
        StoreError::Conflict { current } => Some(current.as_str()),
        _ => None,
    };

    json!({
        "code": error.code(),
        "message": error.to_string(),
        "current_version": current_version,
    })
}
