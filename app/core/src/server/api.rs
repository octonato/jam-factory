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

//! Answers the `/api/` routes with store calls.

use std::path::{Path, PathBuf};

use jam_factory_store::{SessionRef, StoreError};
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};

use super::github;
use super::http::{Request, Response};
use super::issues;
use super::open;
use super::prs;
use super::repos::{RepoList, is_worktree, repos_json};
use super::terminals::{self, Terminals};
use super::worktrees;
use crate::store::Store;
use crate::version::BUILD_VERSION;

const HTML: &str = "text/html; charset=utf-8";
const JSON: &str = "application/json";
const TEXT: &str = "text/plain; charset=utf-8";

/// Answers `GET target` with `store`, `repos` and `terminals`.
pub fn get(
    store: &dyn Store,
    repos: &dyn RepoList,
    terminals: &Terminals,
    target: &str,
) -> Response {
    let segments = segments(target);

    let result = match segments.as_slice() {
        ["", "api", "sessions"] => sessions(store),
        ["", "api", "repos"] => Ok(json_response(&repos_json(repos))),
        ["", "api", "terminals"] => Ok(terminals::get(terminals)),
        ["", "api", "version"] => Ok(json_response(&json!({ "version": BUILD_VERSION }))),
        ["", "api", "prs", ..] => return prs::get(repos, target),
        ["", "api", "github"] => return github::get(repos, target),
        ["", "api", "issues", "view"] => return issues::get(repos, target),
        ["", "api", "worktrees", "stats"] => return worktrees::get(repos, target),
        ["", "api", "files", org, repo, branch, id] => files(store, [org, repo, branch, id]),
        ["", "api", "file", org, repo, branch, id, path @ ..] if !path.is_empty() => {
            file(store, [org, repo, branch, id], path)
        }
        _ => return Response::error(404),
    };

    result.unwrap_or_else(|error| error_response(&error))
}

/// Answers `POST` with `store`, `repos` and `terminals`.
pub fn post(
    store: &dyn Store,
    repos: &dyn RepoList,
    terminals: &Terminals,
    request: &Request,
) -> Response {
    let segments = segments(&request.target);

    match segments.as_slice() {
        ["", "api", "archive", org, repo, branch, id] => {
            archive(store, request, [org, repo, branch, id])
        }
        ["", "api", "open", ..] => open::post(store, repos, request),
        ["", "api", "prs", ..] => prs::post(repos, request),
        ["", "api", "terminals", ..] => terminals::post(terminals, request),
        ["", "api", "worktrees", ..] => worktrees::post(repos, terminals, request),
        _ => Response::error(404),
    }
}

/// Answers `PUT` with `store`.
pub fn put(store: &dyn Store, request: &Request) -> Response {
    let segments = segments(&request.target);

    match segments.as_slice() {
        ["", "api", "file", org, repo, branch, id, path @ ..] if !path.is_empty() => {
            write(store, request, [org, repo, branch, id], path)
        }
        _ => Response::error(404),
    }
}

/// The sessions that are not archived, as `{"sessions": [...]}`.
fn sessions(store: &dyn Store) -> Result<Response, StoreError> {
    let sessions = store.list_sessions()?;

    Ok(json_response(&json!({ "sessions": sessions })))
}

/// The file paths of the session `org/repo/branch/id`, as `{"files": [...]}`.
fn files(store: &dyn Store, session: Segments) -> Result<Response, StoreError> {
    let files = store.list_files(&session_ref(session)?)?;

    Ok(json_response(&json!({ "files": files })))
}

/// Serves the file at `path` (percent-encoded segments) in session `org/repo/branch/id`.
/// The response sandboxes the file with an opaque origin.
fn file(store: &dyn Store, session: Segments, path: &[&str]) -> Result<Response, StoreError> {
    let session = session_ref(session)?;
    let path = file_path(path)?;

    let file = store.read_file(&session, &path)?;

    let response = Response::new(
        200,
        content_type(Path::new(&path)),
        file.content.into_bytes(),
    );

    Ok(response
        .with_header("ETag", &etag(&file.version))
        .with_header("Content-Security-Policy", "sandbox"))
}

/// Overwrites the `.md` file at `path` in session `org/repo/branch/id` with the body of `request`.
fn write(store: &dyn Store, request: &Request, session: Segments, path: &[&str]) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let target = session_ref(session).and_then(|s| Ok((s, file_path(path)?)));

    let (session, path) = match target {
        Ok(target) => target,
        Err(error) => return error_response(&error),
    };

    if !path.ends_with(".md") {
        return Response::error(403);
    }

    let Some(expected) = request.header("If-Match") else {
        return Response::error(428);
    };

    let Ok(content) = str::from_utf8(&request.body) else {
        return Response::error(400);
    };

    match store.write_file(&session, &path, content, unquote(expected)) {
        Ok(version) => Response::new(200, TEXT, Vec::new()).with_header("ETag", &etag(&version)),
        Err(StoreError::Conflict { current }) => {
            Response::error(409).with_header("ETag", &etag(&current))
        }
        Err(error) => error_response(&error),
    }
}

/// Archives the session `org/repo/branch/id` when `request` comes from the same origin.
fn archive(store: &dyn Store, request: &Request, session: Segments) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let archived = session_ref(session).and_then(|s| store.archive_session(&s));

    match archived {
        Ok(()) => Response::new(204, TEXT, Vec::new()),
        Err(error) => error_response(&error),
    }
}

/// Whether the request has no `Origin`, or one equal to `http://<Host>`.
pub(super) fn same_origin(request: &Request) -> bool {
    let Some(origin) = request.header("Origin") else {
        return true;
    };

    request
        .header("Host")
        .is_some_and(|host| origin == format!("http://{host}"))
}

/// The `/`-separated segments of the path in `target`, still percent-encoded.
pub(super) fn segments(target: &str) -> Vec<&str> {
    let path = target.split(['?', '#']).next().unwrap_or_default();

    path.split('/').collect()
}

/// The percent-decoded `segment`.
pub(super) fn name(segment: &str) -> Result<String, StoreError> {
    percent_decode_str(segment)
        .decode_utf8()
        .map(|name| name.into_owned())
        .map_err(|_| StoreError::BadName(format!("{segment:?} is not UTF-8")))
}

/// The percent-encoded segments `org`, `repo`, `branch` and `id` that name a session.
pub(super) type Segments<'a> = [&'a str; 4];

/// The session named by the percent-encoded segments `[org, repo, branch, id]`.
pub(super) fn session_ref([org, repo, branch, id]: Segments) -> Result<SessionRef, StoreError> {
    SessionRef::new(&name(org)?, &name(repo)?, &name(branch)?, &name(id)?)
}

/// The `worktree` folder named in the session JSON `text`, if it is a string naming an existing folder.
pub(super) fn worktree(text: &str) -> Option<PathBuf> {
    let session: Value = serde_json::from_str(text).ok()?;

    let worktree = PathBuf::from(session.get("worktree")?.as_str()?);
    worktree.is_dir().then_some(worktree)
}

/// The folder in the `path` field of the query in `target`, if it exists and is a worktree of `repos`.
pub(super) fn configured_worktree(repos: &dyn RepoList, target: &str) -> Option<PathBuf> {
    query_field(target, "path")
        .map(PathBuf::from)
        .filter(|path| path.is_dir() && is_worktree(repos, path))
}

/// The percent-decoded value of the field `name` in the query of `target`.
pub(super) fn query_field(target: &str, name: &str) -> Option<String> {
    let query = target.split_once('?').map_or("", |(_, query)| query);

    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| percent_decode_str(value).decode_utf8().ok())
        .map(|value| value.into_owned())
}

/// The file path named by the percent-encoded segments `path`.
fn file_path(path: &[&str]) -> Result<String, StoreError> {
    let names = path
        .iter()
        .map(|segment| name(segment))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(names.join("/"))
}

/// The `ETag` value of the store version `version`.
fn etag(version: &str) -> String {
    format!("\"{version}\"")
}

/// The version in the `If-Match` value `value`, without its surrounding quotes.
fn unquote(value: &str) -> &str {
    let value = value.trim();

    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

/// `404` for a missing or misnamed session or file, else `500` with the message of `error`.
pub(super) fn error_response(error: &StoreError) -> Response {
    match error {
        StoreError::NotFound(_) | StoreError::BadName(_) => Response::error(404),
        _ => Response::new(500, TEXT, error.to_string().into_bytes()),
    }
}

pub(super) fn json_response(value: &Value) -> Response {
    Response::new(200, JSON, value.to_string().into_bytes())
}

fn content_type(file: &Path) -> &'static str {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();

    match extension.to_ascii_lowercase().as_str() {
        "html" => HTML,
        "js" => "text/javascript; charset=utf-8",
        "json" => JSON,
        "jsonl" => "application/jsonl",
        "md" => "text/markdown; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "txt" => TEXT,
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_follow_the_extension() {
        for (name, expected) in [
            ("a.html", "text/html; charset=utf-8"),
            ("a.js", "text/javascript; charset=utf-8"),
            ("a.json", "application/json"),
            ("runs.jsonl", "application/jsonl"),
            ("a.md", "text/markdown; charset=utf-8"),
            ("a.css", "text/css; charset=utf-8"),
            ("a.txt", "text/plain; charset=utf-8"),
            ("a.png", "application/octet-stream"),
            ("a", "application/octet-stream"),
        ] {
            assert_eq!(content_type(Path::new(name)), expected, "{name}");
        }
    }

    #[test]
    fn names_are_percent_decoded() {
        assert_eq!(name("a%20b%25c"), Ok("a b%c".to_string()));
        assert_eq!(name("caf%C3%A9"), Ok("caf\u{e9}".to_string()));
        assert_eq!(name("%FF").unwrap_err().code(), "bad_name");
    }

    #[test]
    fn store_errors_map_to_statuses() {
        let cases = [
            (StoreError::NotFound("gone".to_string()), 404),
            (StoreError::BadName("bad".to_string()), 404),
            (StoreError::BadJson("broken".to_string()), 500),
            (StoreError::Io("disk".to_string()), 500),
            (StoreError::NoMatch, 500),
        ];

        for (error, status) in cases {
            let response = error_response(&error);

            assert_eq!(response.status, status, "{error:?}");

            if status == 500 {
                assert_eq!(response.body, error.to_string().into_bytes(), "{error:?}");
            }
        }
    }
}
