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

//! Views an issue of a configured main checkout with `gh` and gives the worktree name for it.
//! Route: `GET /api/issues/view?path=<main checkout>&number=<N>`.

use std::path::Path;

use serde::Deserialize;
use serde_json::json;

use super::api::json_response;
use super::http::Response;
use super::prs::{Runner, SystemRunner, configured_main, pr_number};
use super::repos::RepoList;

/// The fields that `gh issue view` prints for an issue.
const VIEW_FIELDS: &str = "number,title,url";

/// The most title words in a worktree name.
const SLUG_WORDS: usize = 4;

const TEXT: &str = "text/plain; charset=utf-8";

/// An issue in the output of `gh issue view`.
#[derive(Debug, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub url: String,
}

/// Answers `GET /api/issues/view` with `repos`.
pub fn get(repos: &dyn RepoList, target: &str) -> Response {
    get_with(repos, target, &SystemRunner { bin: None })
}

/// Answers `GET /api/issues/view` with `repos`, running `gh` with `runner`.
pub fn get_with(repos: &dyn RepoList, target: &str, runner: &dyn Runner) -> Response {
    let Some(number) = pr_number(target) else {
        return Response::error(404);
    };

    let Some(main) = configured_main(repos, target) else {
        return Response::error(404);
    };

    match view_issue(Path::new(&main.path), number, runner) {
        Ok(issue) => json_response(&json!({
            "number": issue.number,
            "title": issue.title,
            "url": issue.url,
            "slug": slug(&issue.title, issue.number),
        })),
        Err(error) => text_response(500, &error),
    }
}

/// Reads the issue `number` of the checkout `dir` with `gh issue view`.
pub fn view_issue(dir: &Path, number: u64, runner: &dyn Runner) -> Result<Issue, String> {
    let number = number.to_string();
    let args = ["issue", "view", &number, "--json", VIEW_FIELDS];

    let json = runner.run(dir, "gh", &args)?;

    serde_json::from_str(&json)
        .map_err(|error| format!("Cannot parse the gh issue view output: {error}"))
}

/// The worktree name for the issue `number` titled `title`: its first 4 lowercase words joined by `-`, then `-{number}`.
/// A title without words gives `issue-{number}`.
pub fn slug(title: &str, number: u64) -> String {
    let title = title.to_lowercase();

    let words: Vec<&str> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(SLUG_WORDS)
        .collect();

    if words.is_empty() {
        return format!("issue-{number}");
    }

    format!("{}-{number}", words.join("-"))
}

/// A plain-text reply with `status` and the body `text`.
fn text_response(status: u16, text: &str) -> Response {
    Response::new(status, TEXT, text.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use serde_json::Value;

    use super::*;
    use crate::server::repos::FixedRepos;

    /// The `gh issue view` output of the issue `45` of `o/r`.
    const ISSUE: &str =
        r#"{"number":45,"title":"[SDK] Add rate-limit","url":"https://github.com/o/r/issues/45"}"#;

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

    /// The percent-encoded `path`.
    fn encoded(path: &Path) -> String {
        utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string()
    }

    /// The `/api/issues/view` target for the main checkout `path` and the issue `number`.
    fn view_target(path: &Path, number: &str) -> String {
        format!("/api/issues/view?path={}&number={number}", encoded(path))
    }

    fn body(response: &Response) -> Value {
        serde_json::from_slice(&response.body).unwrap()
    }

    fn text(response: &Response) -> String {
        String::from_utf8(response.body.clone()).unwrap()
    }

    #[test]
    fn slug_follows_the_rule() {
        assert_eq!(
            slug("[SDK] Add rate-limit: per tenant, configurable", 45),
            "sdk-add-rate-limit-45"
        );
        assert_eq!(slug("Fix bug", 7), "fix-bug-7");
        assert_eq!(slug("Élan: café crème", 3), "élan-café-crème-3");
        assert_eq!(slug("🚀 !!!", 9), "issue-9");
        assert_eq!(slug("", 1), "issue-1");
    }

    #[test]
    fn the_issue_view_gives_the_slug() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![Ok(ISSUE.to_string())]);

        let response = get_with(&repos, &view_target(&main, "45"), &runner);

        assert_eq!(response.status, 200);
        assert_eq!(
            body(&response),
            json!({
                "number": 45,
                "title": "[SDK] Add rate-limit",
                "url": "https://github.com/o/r/issues/45",
                "slug": "sdk-add-rate-limit-45",
            })
        );

        let view = strings(&["issue", "view", "45", "--json", "number,title,url"]);
        assert_eq!(runner.calls(), [(main, "gh".to_string(), view)]);
    }

    #[test]
    fn a_gh_failure_is_a_500_with_its_message() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let failure = "gh: not logged in";
        let runner = StubRunner::new(vec![Err(failure.to_string()), Ok("{}".to_string())]);

        let failed = get_with(&repos, &view_target(&main, "45"), &runner);

        assert_eq!(failed.status, 500);
        assert_eq!(text(&failed), failure);

        let wrong_shape = get_with(&repos, &view_target(&main, "45"), &runner);

        assert_eq!(wrong_shape.status, 500);
        assert!(
            text(&wrong_shape).starts_with("Cannot parse the gh issue view output"),
            "{}",
            text(&wrong_shape)
        );
    }

    #[test]
    fn an_unconfigured_path_or_a_bad_number_is_404() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![]);

        for target in [
            "/api/issues/view?number=45".to_string(),
            view_target(Path::new("/s/o/r/other"), "45"),
            view_target(&main, "0"),
            view_target(&main, "x"),
            format!("/api/issues/view?path={}", encoded(&main)),
        ] {
            let response = get_with(&repos, &target, &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls().is_empty());
    }
}
