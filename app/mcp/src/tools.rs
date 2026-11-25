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

//! The MCP tools, each one store call or one worktree excerpt appended to a session file.

use std::path::PathBuf;
use std::sync::Arc;

use jam_factory_store::sessions::SESSION_FILE;
use jam_factory_store::{Category, SessionFilter, SessionRef, Store, StoreError};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::excerpt::{self, ExcerptError};

/// A session named by its org, its repo, its branch and its id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionArgs {
    /// The org name.
    org: String,

    /// The repo name.
    repo: String,

    /// The last segment of the branch name.
    branch: String,

    /// The session id, such as `walk-20261104-0912-3f00486`.
    session: String,
}

/// The arguments of `list_sessions`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSessionsArgs {
    /// Lists only the sessions of this org.
    org: Option<String>,

    /// Lists only the sessions of this repo.
    repo: Option<String>,

    /// Lists only the sessions of this branch (last segment of its name).
    branch: Option<String>,

    /// Lists only the sessions of this category: `session`, `review`, `walk` or `lesson`.
    category: Option<String>,

    /// Lists the archived sessions too.
    #[serde(default)]
    include_archived: bool,
}

/// The arguments of `start_session`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartSessionArgs {
    /// The org name.
    org: String,

    /// The repo name.
    repo: String,

    /// The last segment of the branch name.
    branch: String,

    /// The category of the session: `session`, `review`, `walk` or `lesson`.
    category: String,

    /// The short sha of HEAD.
    head: String,

    /// The PR number of a review session. Only a review takes it.
    #[serde(default)]
    pr: Option<u64>,

    /// The first fields of `session.json`.
    fields: Map<String, Value>,
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

/// The arguments of `append_diff`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendDiffArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,

    /// The commit the diff starts from.
    base: String,

    /// The commit the diff ends at.
    head: String,

    /// The files to diff, relative to the worktree.
    files: Vec<String>,

    /// The `[start, end]` line ranges on the new side that a kept hunk must meet.
    lines: Option<Vec<[u32; 2]>>,
}

/// The arguments of `append_code`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendCodeArgs {
    #[serde(flatten)]
    session_args: SessionArgs,

    /// The path of the file, relative to the session folder.
    path: String,

    /// The file to read, relative to the worktree.
    file: String,

    /// The first line to read, from 1.
    start: u32,

    /// The last line to read, inclusive.
    end: u32,

    /// The git ref to read the file at. Leave it out to read the worktree.
    #[serde(rename = "ref")]
    git_ref: Option<String>,
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
        SessionRef::new(&self.org, &self.repo, &self.branch, &self.session)
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
        F: FnOnce(&Store) -> Result<Value, ExcerptError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);

        let result = tokio::task::spawn_blocking(move || call(&store))
            .await
            .unwrap_or_else(|e| Err(StoreError::Io(e.to_string()).into()));

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
        description = "Lists sessions, each with `org`, `repo`, `branch`, `session`, `category` and its parsed `session.json`."
    )]
    async fn list_sessions(
        &self,
        Parameters(args): Parameters<ListSessionsArgs>,
    ) -> CallToolResult {
        self.run(move |store| {
            let filter = SessionFilter {
                org: args.org,
                repo: args.repo,
                branch: args.branch,
                category: args.category.as_deref().map(str::parse).transpose()?,
                include_archived: args.include_archived,
            };

            let sessions = store.list_sessions(&filter)?;
            Ok(json!({ "sessions": sessions }))
        })
        .await
    }

    #[tool(
        description = "Creates a session folder `{category}-{YYYYMMDD-HHMM}-{head}` on the branch, or `review-pr{pr}-{YYYYMMDD-HHMM}-{head}` for a review with `pr`, with `fields` in its `session.json`. A `lesson` also gets `assets/course.css` and `assets/quiz.js`. Returns `org`, `repo`, `branch`, `session` and the version. A branch holds at most one live `session` and one live `review`. Fails with `exists` when the branch already holds a live one of the category."
    )]
    async fn start_session(
        &self,
        Parameters(args): Parameters<StartSessionArgs>,
    ) -> CallToolResult {
        self.run(move |store| {
            let category: Category = args.category.parse()?;

            let started = store.start_session(
                &args.org,
                &args.repo,
                &args.branch,
                category,
                &args.head,
                args.pr,
                &args.fields,
            )?;

            let s = started.session;

            Ok(json!({
                "org": s.org(),
                "repo": s.repo(),
                "branch": s.branch(),
                "session": s.session(),
                "version": started.version,
            }))
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

    #[tool(description = "Writes the whole file. The session must exist.")]
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

    #[tool(
        description = "Appends `content` to the end of the file. Creates the file if missing. The session must exist."
    )]
    async fn append_file(&self, Parameters(args): Parameters<AppendFileArgs>) -> CallToolResult {
        self.run(move |store| {
            let s = args.session_args.session_ref()?;
            let version = store.append_file(&s, &args.path, &args.content)?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(
        description = "Appends the git diff of `files` from `base` to `head` in the session's worktree as a fenced diff block, keeping only the hunks that meet `lines` when given."
    )]
    async fn append_diff(&self, Parameters(args): Parameters<AppendDiffArgs>) -> CallToolResult {
        self.run(move |store| {
            if args.files.is_empty() {
                return Err(ExcerptError::BadPath("files is empty".to_string()));
            }

            for file in &args.files {
                excerpt::check_repo_path(file)?;
            }

            let s = args.session_args.session_ref()?;
            let worktree = worktree(store, &s)?;
            let diff = excerpt::git_diff(&worktree, &args.base, &args.head, &args.files)?;

            let body = match &args.lines {
                None => diff,
                Some(lines) => kept_hunks(&diff, lines)?,
            };

            let version = store.append_file(&s, &args.path, &excerpt::fence("diff", &body))?;
            Ok(json!({ "version": version }))
        })
        .await
    }

    #[tool(
        description = "Appends lines `start` to `end` of a worktree file, or of the file at `ref`, as a fenced code block."
    )]
    async fn append_code(&self, Parameters(args): Parameters<AppendCodeArgs>) -> CallToolResult {
        self.run(move |store| {
            excerpt::check_repo_path(&args.file)?;

            let s = args.session_args.session_ref()?;
            let worktree = worktree(store, &s)?;
            let git_ref = args.git_ref.as_deref();
            let body = excerpt::code_lines(&worktree, &args.file, args.start, args.end, git_ref)?;

            let info = format!(
                "{} path={} lines={}-{}",
                excerpt::language(&args.file),
                args.file,
                args.start,
                args.end
            );

            let version = store.append_file(&s, &args.path, &excerpt::fence(&info, &body))?;
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

/// Returns the string `worktree` of the `session.json` of `s`.
fn worktree(store: &Store, s: &SessionRef) -> Result<PathBuf, ExcerptError> {
    let session = store.read_file(s, SESSION_FILE)?;

    let data: Value = serde_json::from_str(&session.content)
        .map_err(|e| StoreError::BadJson(format!("{SESSION_FILE} does not parse: {e}")))?;

    match data.get("worktree").and_then(Value::as_str) {
        Some(worktree) => Ok(PathBuf::from(worktree)),
        None => Err(StoreError::BadJson(format!("{SESSION_FILE} has no string worktree")).into()),
    }
}

/// Returns the hunks of `diff` that meet `lines`, or `empty_diff` when none does.
fn kept_hunks(diff: &str, lines: &[[u32; 2]]) -> Result<String, ExcerptError> {
    let ranges: Vec<(u32, u32)> = lines.iter().map(|&[start, end]| (start, end)).collect();
    let kept = excerpt::filter_hunks(diff, &ranges);

    if kept.is_empty() {
        return Err(ExcerptError::EmptyDiff(format!(
            "No hunk of the diff meets the lines {lines:?}"
        )));
    }

    Ok(kept)
}

/// The JSON error reply of `error`.
fn error_reply(error: &ExcerptError) -> Value {
    let current_version = match error {
        ExcerptError::Store(StoreError::Conflict { current }) => Some(current.as_str()),
        _ => None,
    };

    json!({
        "code": error.code(),
        "message": error.to_string(),
        "current_version": current_version,
    })
}
