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

//! Gives the GitHub page of a configured checkout with `gh`: its pull request in any state, else its repository.
//! Route: `GET /api/github?path=<checkout>`.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;

use super::api::{configured_worktree, json_response};
use super::http::Response;
use super::prs::{Runner, SystemRunner, configured_main};
use super::repos::RepoList;

/// The fields of a pull request in the output of `gh pr view`.
#[derive(Deserialize)]
struct ViewedPr {
    url: String,
}

/// The fields of a repository in the output of `gh repo view`.
#[derive(Deserialize)]
struct ViewedRepo {
    url: String,
}

/// Answers `GET /api/github` with `repos`.
pub fn get(repos: &dyn RepoList, target: &str) -> Response {
    get_with(repos, target, &SystemRunner { bin: None })
}

/// Answers `GET /api/github` with `repos`, running `gh` with `runner`.
pub fn get_with(repos: &dyn RepoList, target: &str, runner: &dyn Runner) -> Response {
    let Some(checkout) = configured_checkout(repos, target) else {
        return Response::error(404);
    };

    let reply = match page_url(&checkout, runner) {
        Ok(url) => json!({ "url": url, "error": null }),
        Err(error) => json!({ "url": null, "error": error }),
    };

    json_response(&reply)
}

/// The existing folder of the configured checkout, main or linked, named by the `path` query field of `target`.
fn configured_checkout(repos: &dyn RepoList, target: &str) -> Option<PathBuf> {
    let main = configured_main(repos, target)
        .map(|main| PathBuf::from(main.path))
        .filter(|main| main.is_dir());

    main.or_else(|| configured_worktree(repos, target))
}

/// The URL of the pull request of the checkout `dir`, else the URL of its repository.
fn page_url(dir: &Path, runner: &dyn Runner) -> Result<String, String> {
    match pr_url(dir, runner) {
        Some(url) => Ok(url),
        None => repo_url(dir, runner),
    }
}

/// The URL of the pull request of the branch of `dir`: open, closed or merged.
fn pr_url(dir: &Path, runner: &dyn Runner) -> Option<String> {
    let json = runner
        .run(dir, "gh", &["pr", "view", "--json", "url"])
        .ok()?;

    let pr: ViewedPr = serde_json::from_str(&json).ok()?;

    Some(pr.url)
}

/// The URL of the repository of `dir`.
fn repo_url(dir: &Path, runner: &dyn Runner) -> Result<String, String> {
    let json = runner.run(dir, "gh", &["repo", "view", "--json", "url"])?;

    let repo: ViewedRepo = serde_json::from_str(&json)
        .map_err(|error| format!("Cannot parse the gh repo view output: {error}"))?;

    Ok(repo.url)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;

    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use serde_json::Value;

    use super::*;
    use crate::server::repos::FixedRepos;

    /// The `gh repo view` output of the repository `o/r`.
    const REPO: &str = r#"{"url":"https://github.com/o/r"}"#;

    /// A runner call: the folder, the program and its arguments.
    type Call = (PathBuf, String, Vec<String>);

    /// A runner that records each call and replies from a queue.
    struct StubRunner {
        calls: Mutex<Vec<Call>>,
        replies: Mutex<VecDeque<Result<String, String>>>,
    }

    impl StubRunner {
        fn new(replies: Vec<Result<String, String>>) -> StubRunner {
            StubRunner {
                calls: Mutex::new(Vec::new()),
                replies: Mutex::new(replies.into()),
            }
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Runner for StubRunner {
        fn run(&self, dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
            let call = (dir.to_path_buf(), program.to_string(), strings(args));
            self.calls.lock().unwrap().push(call);

            let reply = self.replies.lock().unwrap().pop_front();
            reply.unwrap_or_else(|| Err(format!("no reply for {program} {args:?}")))
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    /// The `gh pr view` output of the PR `7` of `o/r`.
    const PR: &str = r#"{"url":"https://github.com/o/r/pull/7"}"#;

    /// The `gh pr view --json url` call in `dir`.
    fn pr_view(dir: &Path) -> Call {
        let args = ["pr", "view", "--json", "url"];
        (dir.to_path_buf(), "gh".to_string(), strings(&args))
    }

    /// The `gh repo view --json url` call in `dir`.
    fn repo_view(dir: &Path) -> Call {
        let args = ["repo", "view", "--json", "url"];
        (dir.to_path_buf(), "gh".to_string(), strings(&args))
    }

    /// The `/api/github` target for the checkout `path`.
    fn github_target(path: &Path) -> String {
        let path = utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string();

        format!("/api/github?path={path}")
    }

    fn body(response: &Response) -> Value {
        serde_json::from_slice(&response.body).unwrap()
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

    /// The canonical main checkout `o/r/main` under `dir` and its linked worktree `o/r/feature x+y`, or `None` without git.
    fn repo_with_worktree(dir: &Path) -> Option<(PathBuf, PathBuf)> {
        if Command::new("git").arg("--version").output().is_err() {
            return None;
        }

        let dir = fs::canonicalize(dir).unwrap();
        let main = dir.join("o/r/main");
        let linked = dir.join("o/r/feature x+y");

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
        git(
            &main,
            &["worktree", "add", "-q", "-b", "feature", "../feature x+y"],
        );

        Some((main, linked))
    }

    #[test]
    fn a_pr_gives_its_url() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = FixedRepos(vec![main]);
        let runner = StubRunner::new(vec![Ok(PR.to_string())]);

        let response = get_with(&repos, &github_target(&linked), &runner);

        assert_eq!(response.status, 200);
        assert_eq!(
            body(&response),
            json!({ "url": "https://github.com/o/r/pull/7", "error": null })
        );
        assert_eq!(runner.calls(), [pr_view(&linked)]);
    }

    #[test]
    fn no_pr_gives_the_repo_url() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = FixedRepos(vec![main]);
        let no_pr = "no pull requests found for branch \"feature\"";
        let runner = StubRunner::new(vec![Err(no_pr.to_string()), Ok(REPO.to_string())]);

        let response = get_with(&repos, &github_target(&linked), &runner);

        assert_eq!(response.status, 200);
        assert_eq!(
            body(&response),
            json!({ "url": "https://github.com/o/r", "error": null })
        );
        assert_eq!(runner.calls(), [pr_view(&linked), repo_view(&linked)]);
    }

    #[test]
    fn a_gh_failure_is_an_error_reply() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = FixedRepos(vec![main]);
        let failure = "gh: not logged in";
        let runner = StubRunner::new(vec![Err(failure.to_string()), Err(failure.to_string())]);

        let response = get_with(&repos, &github_target(&linked), &runner);

        assert_eq!(response.status, 200);
        assert_eq!(body(&response), json!({ "url": null, "error": failure }));

        let wrong_shape = StubRunner::new(vec![Err(failure.to_string()), Ok("{}".to_string())]);
        let reply = body(&get_with(&repos, &github_target(&linked), &wrong_shape));

        assert_eq!(reply["url"], Value::Null);
        assert!(reply["error"].is_string(), "{reply}");
    }

    #[test]
    fn the_main_checkout_is_served() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, _)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![Ok(PR.to_string())]);

        let response = get_with(&repos, &github_target(&main), &runner);

        assert_eq!(response.status, 200);
        assert_eq!(
            body(&response),
            json!({ "url": "https://github.com/o/r/pull/7", "error": null })
        );
        assert_eq!(runner.calls(), [pr_view(&main)]);
    }

    #[test]
    fn an_unconfigured_path_is_404() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let parent = main.parent().unwrap().to_path_buf();
        let gone = parent.join("gone");
        let repos = FixedRepos(vec![main, gone.clone()]);
        let runner = StubRunner::new(vec![]);

        for target in [
            "/api/github".to_string(),
            "/api/github?path=".to_string(),
            github_target(&parent),
            github_target(&linked.join("missing")),
            github_target(&gone),
        ] {
            let response = get_with(&repos, &target, &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls().is_empty());
    }
}
