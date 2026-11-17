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

//! Creates a worktree on a new branch next to a configured main checkout.
//! Route: `POST /api/worktrees?path=<main checkout>&name=<name>`.

use std::fs;
use std::path::Path;

use serde_json::json;

use super::api::{json_response, query_field, same_origin, segments};
use super::http::{Request, Response};
use super::prs::{Runner, SystemRunner, configured_main, worktree_folder};
use super::repos::RepoList;

const TEXT: &str = "text/plain; charset=utf-8";

/// Answers `POST /api/worktrees` with `repos`.
pub fn post(repos: &dyn RepoList, request: &Request) -> Response {
    post_with(repos, request, &SystemRunner { bin: None })
}

/// Answers `POST /api/worktrees` with `repos`, running `git` with `runner`.
pub fn post_with(repos: &dyn RepoList, request: &Request, runner: &dyn Runner) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    if segments(&request.target)[..] != ["", "api", "worktrees"] {
        return Response::error(404);
    }

    let Some(main) = configured_main(repos, &request.target) else {
        return Response::error(404);
    };

    let name = query_field(&request.target, "name").unwrap_or_default();
    let name = name.trim();

    if name.is_empty() {
        return text_response(400, "The worktree name is empty");
    }

    if name.contains('/') {
        return text_response(400, &format!("The worktree name {name} holds a /"));
    }

    let user = match repos.github_user() {
        Ok(user) => user,
        Err(error) => return text_response(500, &error),
    };

    let branch = branch_name(user.as_deref(), name);

    create(Path::new(&main.path), &branch, runner)
}

/// The branch of the worktree `name`: `{user}/{name}` with a user, else `name`.
pub fn branch_name(user: Option<&str>, name: &str) -> String {
    match user {
        Some(user) => format!("{user}/{name}"),
        None => name.to_string(),
    }
}

/// Creates the branch `branch` at the HEAD of the main checkout `main`, in a new sibling worktree.
fn create(main: &Path, branch: &str, runner: &dyn Runner) -> Response {
    let check = ["check-ref-format", "--branch", branch];

    if runner.run(main, "git", &check).is_err() {
        return text_response(400, &format!("{branch} is not a valid branch name"));
    }

    let reference = format!("refs/heads/{branch}");
    let verify = ["rev-parse", "--verify", "--quiet", &reference];

    if runner.run(main, "git", &verify).is_ok() {
        return text_response(409, &format!("Branch {branch} already exists"));
    }

    let folder = worktree_folder(main, branch);

    if folder.exists() {
        return text_response(409, &format!("{} already exists", folder.display()));
    }

    let folder_arg = folder.to_string_lossy();
    let add = ["worktree", "add", "-b", branch, &folder_arg, "HEAD"];

    if let Err(error) = runner.run(main, "git", &add) {
        return text_response(500, &error);
    }

    let listed = fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());

    json_response(&json!({ "path": listed.to_string_lossy() }))
}

/// A plain-text reply with `status` and the body `text`.
fn text_response(status: u16, text: &str) -> Response {
    Response::new(status, TEXT, text.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use serde_json::Value;

    use super::*;
    use crate::server::repos::{find_main, is_worktree};

    /// The main checkouts `paths` with the GitHub user `user`.
    struct UserRepos {
        paths: Vec<PathBuf>,
        user: Result<Option<String>, String>,
    }

    impl RepoList for UserRepos {
        fn paths(&self) -> Result<Vec<PathBuf>, String> {
            Ok(self.paths.clone())
        }

        fn github_user(&self) -> Result<Option<String>, String> {
            self.user.clone()
        }
    }

    fn repos(main: &Path, user: Option<&str>) -> UserRepos {
        UserRepos {
            paths: vec![main.to_path_buf()],
            user: Ok(user.map(str::to_string)),
        }
    }

    /// Runs `git args` in `dir`, without the user and system git config.
    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();

        assert!(status.success(), "git {args:?}");
    }

    /// The canonical main checkout `o/r/main` under `dir` with one commit, or `None` without git.
    fn main_checkout(dir: &Path) -> Option<PathBuf> {
        if Command::new("git").arg("--version").output().is_err() {
            return None;
        }

        let dir = fs::canonicalize(dir).unwrap();
        let main = dir.join("o/r/main");

        git(&dir, &["init", "-q", "-b", "main", "o/r/main"]);
        git(
            &main,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
        );

        Some(main)
    }

    /// A `POST` on `/api/worktrees` for the main checkout `main` and the worktree `name`, from the board's host.
    fn request(main: &Path, name: &str) -> Request {
        let encode = |text: &str| utf8_percent_encode(text, NON_ALPHANUMERIC).to_string();
        let target = format!(
            "/api/worktrees?path={}&name={}",
            encode(&main.to_string_lossy()),
            encode(name)
        );

        Request {
            method: "POST".to_string(),
            target,
            headers: vec![("Host".to_string(), "127.0.0.1:1".to_string())],
            body: Vec::new(),
        }
    }

    fn text(response: &Response) -> String {
        String::from_utf8(response.body.clone()).unwrap()
    }

    /// The branch of the linked worktree at `path` of the main checkout `main`.
    fn branch_of(repos: &UserRepos, main: &Path, path: &Path) -> Option<String> {
        find_main(repos, main)?
            .worktrees
            .into_iter()
            .find(|worktree| Path::new(&worktree.path) == path)?
            .branch
    }

    #[test]
    fn branch_names_take_the_user_prefix() {
        assert_eq!(branch_name(Some("octonato"), "fix"), "octonato/fix");
        assert_eq!(branch_name(None, "fix"), "fix");
    }

    #[test]
    fn creates_a_prefixed_branch_in_a_sibling_folder() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = repos(&main, Some("octonato"));

        let response = post(&repos, &request(&main, " fix-it "));

        assert_eq!(response.status, 200, "{}", text(&response));

        let reply: Value = serde_json::from_slice(&response.body).unwrap();
        let folder = main.parent().unwrap().join("fix-it");

        assert_eq!(reply["path"], folder.to_string_lossy().as_ref());
        assert!(is_worktree(&repos, &folder));
        assert_eq!(
            branch_of(&repos, &main, &folder).as_deref(),
            Some("octonato/fix-it")
        );
    }

    #[test]
    fn creates_a_bare_branch_without_a_user() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = repos(&main, None);

        let response = post(&repos, &request(&main, "fix-it"));

        assert_eq!(response.status, 200, "{}", text(&response));

        let folder = main.parent().unwrap().join("fix-it");

        assert_eq!(branch_of(&repos, &main, &folder).as_deref(), Some("fix-it"));
    }

    #[test]
    fn refuses_bad_names_and_taken_branches_or_folders() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = repos(&main, Some("octonato"));

        git(&main, &["branch", "octonato/taken"]);
        fs::create_dir(main.parent().unwrap().join("busy")).unwrap();

        for (name, status) in [
            ("", 400),
            ("  ", 400),
            ("a/b", 400),
            ("bad..name", 400),
            ("taken", 409),
            ("busy", 409),
        ] {
            let response = post(&repos, &request(&main, name));

            assert_eq!(response.status, status, "{name:?}: {}", text(&response));
        }
    }

    #[test]
    fn checks_origin_route_checkout_and_config() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };

        let mut foreign = request(&main, "x");
        foreign
            .headers
            .push(("Origin".to_string(), "http://evil".to_string()));
        assert_eq!(post(&repos(&main, None), &foreign).status, 403);

        let mut other_route = request(&main, "x");
        other_route.target = other_route.target.replace("worktrees", "worktrees/x");
        assert_eq!(post(&repos(&main, None), &other_route).status, 404);

        let unlisted = repos(&main.parent().unwrap().join("other"), None);
        assert_eq!(post(&unlisted, &request(&main, "x")).status, 404);

        let broken = UserRepos {
            paths: vec![main.clone()],
            user: Err("bad config".to_string()),
        };
        let response = post(&broken, &request(&main, "x"));
        assert_eq!(response.status, 500);
        assert_eq!(text(&response), "bad config");
    }
}
