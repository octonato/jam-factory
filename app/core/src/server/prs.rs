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

//! Lists, caches, refreshes, checks out and opens the pull requests of a configured main checkout with `gh` and `git`.
//! Routes: `GET /api/prs?path=<main checkout>`, `GET /api/prs/cached?path=<main checkout>`,
//! `GET /api/prs/view?path=<main checkout>&number=<N>`, `POST /api/prs/checkout?path=<main checkout>&number=<N>`
//! and `POST /api/prs/open?path=<main checkout>&number=<N>`.

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::api::{json_response, query_field, same_origin, segments};
use super::http::{Request, Response};
use super::repos::{Repo, RepoList, find_main};

/// The fields that `gh pr list` prints for each pull request.
pub const LIST_FIELDS: &str = "number,title,author,headRefName,headRepositoryOwner,isCrossRepository,isDraft,additions,deletions,url,updatedAt,statusCheckRollup,reviewDecision,latestReviews,reviewRequests";

/// The fields that `gh pr view` prints for the pull request to check out.
const HEAD_FIELDS: &str = "headRefName,headRepositoryOwner,isCrossRepository";

/// The Homebrew folders at the end of the `PATH` of every program.
const BREW_FOLDERS: &str = "/opt/homebrew/bin:/usr/local/bin";

const TEXT: &str = "text/plain; charset=utf-8";

/// The login of a deleted account.
const GHOST: &str = "ghost";

/// The pull requests the server listed since it started.
static CACHE: LazyLock<PrCache> = LazyLock::new(PrCache::default);

/// Runs a program in a folder and waits for it to exit.
pub trait Runner: Send + Sync {
    /// Runs `program` with `args` in `dir`, giving its stdout, or its trimmed stderr, exit status or start error on failure.
    fn run(&self, dir: &Path, program: &str, args: &[&str]) -> Result<String, String>;
}

/// Runs programs with `std::process::Command`, with `bin` first and the Homebrew folders last on `PATH`.
pub struct SystemRunner {
    pub bin: Option<PathBuf>,
}

impl SystemRunner {
    /// The `PATH` of the programs: `bin`, the current `PATH`, then the Homebrew folders.
    fn path(&self) -> OsString {
        let mut path = OsString::new();

        if let Some(bin) = &self.bin {
            path.push(bin);
            path.push(":");
        }

        path.push(env::var_os("PATH").unwrap_or_default());
        path.push(":");
        path.push(BREW_FOLDERS);

        path
    }
}

impl Runner for SystemRunner {
    fn run(&self, dir: &Path, program: &str, args: &[&str]) -> Result<String, String> {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .env("PATH", self.path())
            .output()
            .map_err(|error| error.to_string())?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(output.status.to_string())
        } else {
            Err(stderr)
        }
    }
}

/// An open pull request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub branch: String,
    pub draft: bool,
    pub additions: u64,
    pub deletions: u64,
    pub url: String,
    pub ci: Ci,
    pub review: Review,
    pub reviewers: Vec<Reviewer>,
    pub review_requested: bool,
}

/// The combined state of the checks of a pull request, from the least to the most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ci {
    None,
    Passing,
    Pending,
    Failing,
}

/// The review state of a pull request, from the least to the most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Review {
    None,
    Commented,
    ReviewRequired,
    Approved,
    ChangesRequested,
}

/// The latest submitted review of one reviewer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reviewer {
    pub login: String,
    pub state: String,
}

/// A pull request in the output of `gh pr list`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedPr {
    number: u64,
    title: String,
    #[serde(default)]
    author: Option<Author>,
    #[serde(flatten)]
    head: Head,
    is_draft: bool,
    additions: u64,
    deletions: u64,
    url: String,
    updated_at: String,
    #[serde(default)]
    status_check_rollup: Vec<Check>,
    #[serde(default)]
    review_decision: Option<String>,
    #[serde(default)]
    latest_reviews: Vec<ListedReview>,
    #[serde(default)]
    review_requests: Vec<IgnoredAny>,
    #[serde(default)]
    state: Option<String>,
}

/// A check run or a commit status in the `statusCheckRollup` of `gh pr list`.
#[derive(Deserialize)]
struct Check {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

impl Check {
    /// The state of the check: a commit status by its `state`, a check run by its `status` and `conclusion`.
    fn ci(&self) -> Ci {
        if let Some(state) = &self.state {
            return match state.as_str() {
                "SUCCESS" => Ci::Passing,
                "FAILURE" | "ERROR" => Ci::Failing,
                _ => Ci::Pending,
            };
        }

        if self.status.as_deref() != Some("COMPLETED") {
            return Ci::Pending;
        }

        match self.conclusion.as_deref() {
            Some("FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE") => {
                Ci::Failing
            }
            _ => Ci::Passing,
        }
    }
}

/// A review in the `latestReviews` of `gh pr list`.
#[derive(Deserialize)]
struct ListedReview {
    #[serde(default)]
    author: Option<Author>,
    state: String,
}

impl ListedReview {
    /// The review state this review gives the pull request.
    fn review(&self) -> Review {
        match self.state.as_str() {
            "APPROVED" => Review::Approved,
            "CHANGES_REQUESTED" => Review::ChangesRequested,
            _ => Review::Commented,
        }
    }
}

/// The most severe state of `checks`, or `Ci::None` without checks.
fn ci_state(checks: &[Check]) -> Ci {
    checks.iter().map(Check::ci).max().unwrap_or(Ci::None)
}

/// The review state of a pull request: its `decision` when GitHub gives one, else the most severe of `reviews`.
fn review_state(decision: Option<&str>, reviews: &[ListedReview]) -> Review {
    match decision {
        Some("APPROVED") => Review::Approved,
        Some("CHANGES_REQUESTED") => Review::ChangesRequested,
        Some("REVIEW_REQUIRED") => Review::ReviewRequired,
        _ => reviews
            .iter()
            .map(ListedReview::review)
            .max()
            .unwrap_or(Review::None),
    }
}

/// The author of a pull request in the output of `gh pr list`.
#[derive(Deserialize)]
struct Author {
    login: String,
    #[serde(default)]
    is_bot: bool,
}

/// The head branch of a pull request in the output of `gh pr list` and `gh pr view`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Head {
    head_ref_name: String,
    #[serde(default)]
    head_repository_owner: Option<Owner>,
    is_cross_repository: bool,
}

/// The owner of the head repository of a pull request.
#[derive(Deserialize)]
struct Owner {
    login: String,
}

impl Head {
    /// The local branch of the pull request: `{owner}/{headRefName}` for a cross-repo PR, else `headRefName`.
    fn local_branch(&self) -> String {
        if !self.is_cross_repository {
            return self.head_ref_name.clone();
        }

        let owner = self
            .head_repository_owner
            .as_ref()
            .map(|owner| owner.login.as_str())
            .filter(|login| !login.is_empty())
            .unwrap_or(GHOST);

        format!("{owner}/{}", self.head_ref_name)
    }
}

impl From<ListedPr> for Pr {
    fn from(mut pr: ListedPr) -> Pr {
        pr.latest_reviews.retain(|review| review.state != "PENDING");

        let ci = ci_state(&pr.status_check_rollup);
        let review = review_state(pr.review_decision.as_deref(), &pr.latest_reviews);
        let review_requested = !pr.review_requests.is_empty();

        let reviewers = pr
            .latest_reviews
            .into_iter()
            .map(|listed| Reviewer {
                login: listed
                    .author
                    .map_or_else(|| GHOST.to_string(), |author| author.login),
                state: listed.state.to_lowercase().replace('_', " "),
            })
            .collect();

        Pr {
            number: pr.number,
            title: pr.title,
            author: pr
                .author
                .map_or_else(|| GHOST.to_string(), |author| author.login),
            branch: pr.head.local_branch(),
            draft: pr.is_draft,
            additions: pr.additions,
            deletions: pr.deletions,
            url: pr.url,
            ci,
            review,
            reviewers,
            review_requested,
        }
    }
}

/// The pull requests in the `gh pr list` output `json` without bot authors, the last updated first.
pub fn parse_prs(json: &str) -> Result<Vec<Pr>, String> {
    let mut listed: Vec<ListedPr> = serde_json::from_str(json)
        .map_err(|error| format!("Cannot parse the gh pr list output: {error}"))?;

    listed.retain(|pr| !pr.author.as_ref().is_some_and(|author| author.is_bot));
    listed.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(listed.into_iter().map(Pr::from).collect())
}

/// The pull request in the `gh pr view` output `json`, or `None` when it is not open or has a bot author.
fn parse_open_pr(json: &str) -> Result<Option<Pr>, String> {
    let pr: ListedPr = serde_json::from_str(json)
        .map_err(|error| format!("Cannot parse the gh pr view output: {error}"))?;

    let open = pr.state.as_deref() == Some("OPEN");
    let bot = pr.author.as_ref().is_some_and(|author| author.is_bot);

    Ok((open && !bot).then(|| Pr::from(pr)))
}

/// The head branch in the `gh pr view` output `json`.
fn parse_head(json: &str) -> Result<Head, String> {
    serde_json::from_str(json)
        .map_err(|error| format!("Cannot parse the gh pr view output: {error}"))
}

/// The configured main checkout of `repos` named by the `path` query field of `target`.
pub fn configured_main(repos: &dyn RepoList, target: &str) -> Option<Repo> {
    let path = query_field(target, "path")?;

    find_main(repos, Path::new(&path))
}

/// The last listed pull requests of each main checkout, by its path.
#[derive(Default)]
pub struct PrCache {
    lists: Mutex<HashMap<String, Vec<Pr>>>,
}

impl PrCache {
    /// The cached pull requests of the main checkout `main`, or `None` before its first listing.
    pub fn list(&self, main: &str) -> Option<Vec<Pr>> {
        self.lists().get(main).cloned()
    }

    /// Caches `prs` as the pull requests of the main checkout `main`.
    pub fn store(&self, main: &str, prs: &[Pr]) {
        self.lists().insert(main.to_string(), prs.to_vec());
    }

    /// Replaces the cached PR `number` of `main` with `pr`, or drops it when `pr` is `None`.
    /// A PR missing from the list goes first. A main checkout never listed stays uncached.
    pub fn update(&self, main: &str, number: u64, pr: Option<&Pr>) {
        let mut lists = self.lists();
        let Some(prs) = lists.get_mut(main) else {
            return;
        };

        let index = prs.iter().position(|cached| cached.number == number);

        match (index, pr) {
            (Some(index), Some(pr)) => prs[index] = pr.clone(),
            (Some(index), None) => {
                prs.remove(index);
            }
            (None, Some(pr)) => prs.insert(0, pr.clone()),
            (None, None) => {}
        }
    }

    fn lists(&self) -> MutexGuard<'_, HashMap<String, Vec<Pr>>> {
        self.lists.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Answers `GET /api/prs`, `GET /api/prs/cached` and `GET /api/prs/view` with `repos`.
pub fn get(repos: &dyn RepoList, target: &str) -> Response {
    get_with(repos, target, &SystemRunner { bin: None }, &CACHE)
}

/// Answers `GET /api/prs`, `GET /api/prs/cached` and `GET /api/prs/view` with `repos`, running `gh` with `runner` and caching in `cache`.
pub fn get_with(
    repos: &dyn RepoList,
    target: &str,
    runner: &dyn Runner,
    cache: &PrCache,
) -> Response {
    let route = segments(target);
    let number = pr_number(target);

    let Some(main) = configured_main(repos, target) else {
        return Response::error(404);
    };

    match (&route[..], number) {
        (["", "api", "prs"], _) => list(&main, runner, cache),
        (["", "api", "prs", "cached"], _) => cached(&main, cache),
        (["", "api", "prs", "view"], Some(number)) => view(&main, number, runner, cache),
        _ => Response::error(404),
    }
}

/// Lists the open pull requests of `main` with `gh pr list` and caches them on success.
fn list(main: &Repo, runner: &dyn Runner, cache: &PrCache) -> Response {
    let args = [
        "pr",
        "list",
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        LIST_FIELDS,
    ];

    let prs = runner
        .run(Path::new(&main.path), "gh", &args)
        .and_then(|json| parse_prs(&json));

    let reply = match prs {
        Ok(prs) => {
            cache.store(&main.path, &prs);
            json!({ "error": null, "prs": prs })
        }
        Err(error) => json!({ "error": error, "prs": [] }),
    };

    json_response(&reply)
}

/// The cached pull requests of `main`, with `prs: null` before its first listing.
fn cached(main: &Repo, cache: &PrCache) -> Response {
    json_response(&json!({ "error": null, "prs": cache.list(&main.path) }))
}

/// Reads the PR `number` of `main` with `gh pr view` and updates the cache, with `pr: null` when it is no longer open.
fn view(main: &Repo, number: u64, runner: &dyn Runner, cache: &PrCache) -> Response {
    let number_arg = number.to_string();
    let fields = format!("{LIST_FIELDS},state");
    let args = ["pr", "view", &number_arg, "--json", &fields];

    let pr = runner
        .run(Path::new(&main.path), "gh", &args)
        .and_then(|json| parse_open_pr(&json));

    let reply = match pr {
        Ok(pr) => {
            cache.update(&main.path, number, pr.as_ref());
            json!({ "error": null, "pr": pr })
        }
        Err(error) => json!({ "error": error, "pr": null }),
    };

    json_response(&reply)
}

/// Answers `POST /api/prs/checkout` and `POST /api/prs/open` with `repos`.
pub fn post(repos: &dyn RepoList, request: &Request) -> Response {
    post_with(repos, request, &SystemRunner { bin: None })
}

/// Answers `POST /api/prs/checkout` and `POST /api/prs/open` with `repos`, running `git` and `gh` with `runner`.
pub fn post_with(repos: &dyn RepoList, request: &Request, runner: &dyn Runner) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let action: fn(&Repo, u64, &dyn Runner) -> Response = match segments(&request.target)[..] {
        ["", "api", "prs", "checkout"] => checkout,
        ["", "api", "prs", "open"] => open,
        _ => return Response::error(404),
    };

    let Some(number) = pr_number(&request.target) else {
        return Response::error(404);
    };

    let Some(main) = configured_main(repos, &request.target) else {
        return Response::error(404);
    };

    action(&main, number, runner)
}

/// The folder of a new worktree for `branch`: the sibling of `main` named after the last segment of `branch`.
pub fn worktree_folder(main: &Path, branch: &str) -> PathBuf {
    let name = branch.rsplit_once('/').map_or(branch, |(_, name)| name);

    main.parent().unwrap_or(main).join(name)
}

/// The positive integer in the `number` query field of `target`.
pub(super) fn pr_number(target: &str) -> Option<u64> {
    query_field(target, "number")?
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
}

/// Checks out the PR `number` of `main` into a new sibling worktree, or finds the linked worktree on its local branch.
fn checkout(main: &Repo, number: u64, runner: &dyn Runner) -> Response {
    let dir = Path::new(&main.path);
    let number = number.to_string();

    let view = ["pr", "view", &number, "--json", HEAD_FIELDS];
    let head = match runner
        .run(dir, "gh", &view)
        .and_then(|json| parse_head(&json))
    {
        Ok(head) => head,
        Err(error) => return text_response(500, &error),
    };
    let branch = head.local_branch();

    let linked = main
        .worktrees
        .iter()
        .find(|worktree| worktree.branch.as_deref() == Some(branch.as_str()));

    if let Some(worktree) = linked {
        return path_response(&worktree.path);
    }

    let reference = format!("refs/heads/{branch}");
    let verify = ["rev-parse", "--verify", "--quiet", &reference];

    if runner.run(dir, "git", &verify).is_ok() {
        let message = format!("Branch {branch} exists locally without a worktree");
        return text_response(409, &message);
    }

    let folder = worktree_folder(dir, &branch);

    if folder.exists() {
        let message = format!("{} already exists", folder.display());
        return text_response(409, &message);
    }

    let folder_arg = folder.to_string_lossy();
    let add = ["worktree", "add", "--detach", &folder_arg, "HEAD"];

    if let Err(error) = runner.run(dir, "git", &add) {
        return text_response(500, &error);
    }

    let mut gh_checkout = vec!["pr", "checkout", number.as_str()];

    if head.is_cross_repository {
        gh_checkout.extend(["--branch", branch.as_str()]);
    }

    if let Err(error) = runner.run(&folder, "gh", &gh_checkout) {
        let remove = ["worktree", "remove", "--force", &folder_arg];
        let _ = runner.run(dir, "git", &remove);

        return text_response(500, &error);
    }

    let listed = fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());

    path_response(&listed.to_string_lossy())
}

/// Opens the PR `number` of `main` on GitHub.
fn open(main: &Repo, number: u64, runner: &dyn Runner) -> Response {
    let number = number.to_string();
    let view = ["pr", "view", &number, "--web"];

    match runner.run(Path::new(&main.path), "gh", &view) {
        Ok(_) => Response::new(204, TEXT, Vec::new()),
        Err(error) => text_response(500, &error),
    }
}

/// The JSON reply `{"path": path}`.
fn path_response(path: &str) -> Response {
    json_response(&json!({ "path": path }))
}

/// A plain-text reply with `status` and the body `text`.
fn text_response(status: u16, text: &str) -> Response {
    Response::new(status, TEXT, text.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::process::Stdio;
    use std::sync::Mutex;

    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use serde_json::Value;

    use super::*;
    use crate::server::repos::{FixedRepos, Worktree, is_worktree};

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

    /// The `gh pr list` entry of the same-repo PR `number` by `author`, updated on day `day` of November 2026.
    fn listed(number: u64, author: Value, day: u32) -> Value {
        json!({
            "number": number,
            "title": format!("PR {number}"),
            "author": author,
            "headRefName": format!("alice/pr-{number}"),
            "headRepositoryOwner": { "id": "U_o", "login": "o" },
            "isCrossRepository": false,
            "isDraft": number == 3,
            "additions": number * 10,
            "deletions": number,
            "url": format!("https://github.com/o/r/pull/{number}"),
            "updatedAt": format!("2026-11-{day:02}T10:00:00Z"),
            "statusCheckRollup": [],
            "reviewDecision": "",
            "latestReviews": [],
            "reviewRequests": [],
        })
    }

    /// A check run in `status` with `conclusion`.
    fn check_run(status: &str, conclusion: &str) -> Value {
        json!({ "__typename": "CheckRun", "name": "build", "status": status, "conclusion": conclusion })
    }

    /// A commit status in `state`.
    fn status_context(state: &str) -> Value {
        json!({ "__typename": "StatusContext", "context": "ci/legacy", "state": state })
    }

    /// A latest review by `login` in `state`.
    fn review(login: &str, state: &str) -> Value {
        json!({ "author": { "login": login }, "state": state, "body": "" })
    }

    /// The CI state of the PR whose `statusCheckRollup` is `checks`.
    fn ci_of(checks: Value) -> Ci {
        let pr = with_fields(
            listed(1, json!({ "login": "alice" }), 1),
            json!({ "statusCheckRollup": checks }),
        );

        parse_prs(&json!([pr]).to_string()).unwrap().remove(0).ci
    }

    /// The `gh pr view` head of a PR on `branch` in the repository of `owner`, cross-repo when `cross`.
    fn head(branch: &str, owner: Value, cross: bool) -> Value {
        json!({
            "headRefName": branch,
            "headRepositoryOwner": owner,
            "isCrossRepository": cross,
        })
    }

    /// The `gh pr list` entry `pr` with the fields of `fields`.
    fn with_fields(mut pr: Value, fields: Value) -> Value {
        for (name, value) in fields.as_object().unwrap() {
            pr[name] = value.clone();
        }

        pr
    }

    /// The owner `login` of a head repository.
    fn owner(login: &str) -> Value {
        json!({ "id": format!("U_{login}"), "login": login })
    }

    fn numbers(prs: &[Pr]) -> Vec<u64> {
        prs.iter().map(|pr| pr.number).collect()
    }

    /// The percent-encoded `path`.
    fn encoded(path: &Path) -> String {
        utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string()
    }

    /// The `/api/prs` target for the main checkout `path`.
    fn prs_target(path: &Path) -> String {
        format!("/api/prs?path={}", encoded(path))
    }

    /// The `/api/prs/cached` target for the main checkout `path`.
    fn cached_target(path: &Path) -> String {
        format!("/api/prs/cached?path={}", encoded(path))
    }

    /// The `/api/prs/view` target for the main checkout `path` and the PR `number`.
    fn view_target(path: &Path, number: u64) -> String {
        post_target("view", path, &number.to_string())
    }

    /// The `/api/prs/{route}` target for the main checkout `path` and the PR `number`.
    fn post_target(route: &str, path: &Path, number: &str) -> String {
        format!("/api/prs/{route}?path={}&number={number}", encoded(path))
    }

    /// A `POST` on `target` from the board's host, with the extra `headers`.
    fn request(target: &str, headers: &[(&str, &str)]) -> Request {
        let mut all = vec![("Host".to_string(), "127.0.0.1:1".to_string())];
        for (name, value) in headers {
            all.push((name.to_string(), value.to_string()));
        }

        Request {
            method: "POST".to_string(),
            target: target.to_string(),
            headers: all,
            body: Vec::new(),
        }
    }

    fn body(response: &Response) -> Value {
        serde_json::from_slice(&response.body).unwrap()
    }

    fn text(response: &Response) -> String {
        String::from_utf8(response.body.clone()).unwrap()
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

    /// The canonical main checkout `o/r/{name}` under `dir` on branch `main` with one commit, or `None` without git.
    fn main_checkout(dir: &Path, name: &str) -> Option<PathBuf> {
        main_checkout_at(&fs::canonicalize(dir).unwrap(), name)
    }

    /// The main checkout `o/r/{name}` under `dir` as given, on branch `main` with one commit, or `None` without git.
    fn main_checkout_at(dir: &Path, name: &str) -> Option<PathBuf> {
        if Command::new("git").arg("--version").output().is_err() {
            return None;
        }

        let main = dir.join("o/r").join(name);

        git(dir, &["init", "-q", "-b", "main", &format!("o/r/{name}")]);
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

    /// The canonical main checkout `o/r/{name}` under `dir` and its linked worktree `o/r/feature`, or `None` without git.
    fn repo_with_worktree(dir: &Path, name: &str) -> Option<(PathBuf, PathBuf)> {
        let main = main_checkout(dir, name)?;
        let linked = main.parent().unwrap().join("feature");

        git(
            &main,
            &["worktree", "add", "-q", "-b", "feature", "../feature"],
        );

        Some((main, linked))
    }

    /// A stub `gh` that prints the PR head in `head` next to it, records `pr view N --web` in `viewed`,
    /// or records `pr checkout` in `checked-out` and checks out the `--branch` value or the branch in `branch`.
    const GH_STUB: &str = r#"bin=$(dirname "$0")
case "$1 $2" in
    "pr view")
        if [ "$4" = --web ]; then
            echo "$@" > "$bin/viewed"
        elif [ "$4 $5" = "--json headRefName,headRepositoryOwner,isCrossRepository" ]; then
            cat "$bin/head"
        else
            exit 1
        fi
        ;;
    "pr checkout")
        echo "$@" > "$bin/checked-out"
        if [ -e "$bin/fail" ]; then
            echo boom >&2
            exit 1
        fi
        if [ "$4" = --branch ]; then
            git checkout -q -b "$5"
        else
            git checkout -q -b "$(cat "$bin/branch")"
        fi
        ;;
    *)
        exit 1
        ;;
esac
"#;

    /// Writes the executable shell script `body` to `file`.
    fn write_program(file: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(file, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A runner with `dir/bin` first on `PATH`, where the stub `gh` gives the same-repo PR branch `branch`.
    fn stub_gh(dir: &Path, branch: &str) -> SystemRunner {
        stub_gh_head(dir, &head(branch, owner("o"), false))
    }

    /// A runner with `dir/bin` first on `PATH`, where the stub `gh` gives the PR head `head`.
    fn stub_gh_head(dir: &Path, head: &Value) -> SystemRunner {
        let bin = dir.join("bin");
        fs::create_dir(&bin).unwrap();
        set_head(&bin, head);
        write_program(&bin.join("gh"), GH_STUB);

        SystemRunner { bin: Some(bin) }
    }

    /// Makes the stub `gh` in `bin` give the PR head `head`.
    fn set_head(bin: &Path, head: &Value) {
        let branch = head["headRefName"].as_str().unwrap();

        fs::write(bin.join("head"), head.to_string()).unwrap();
        fs::write(bin.join("branch"), format!("{branch}\n")).unwrap();
    }

    /// The arguments of the last `gh pr checkout` of the stub `gh` in `dir/bin`.
    fn checked_out(dir: &Path) -> String {
        fs::read_to_string(dir.join("bin/checked-out")).unwrap()
    }

    /// The linked worktrees that `git worktree list` gives in the main checkout `main`.
    fn linked_worktrees(main: &Path) -> Vec<Worktree> {
        find_main(&FixedRepos(vec![main.to_path_buf()]), main)
            .unwrap()
            .worktrees
    }

    /// The names in the folder `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();

        names.sort();
        names
    }

    #[test]
    fn parses_and_sorts_prs() {
        let alice = json!({ "login": "alice" });
        let json = json!([
            listed(1, alice.clone(), 1),
            listed(3, alice.clone(), 3),
            listed(2, alice, 2),
        ]);

        let prs = parse_prs(&json.to_string()).unwrap();

        assert_eq!(numbers(&prs), [3, 2, 1]);
        assert_eq!(
            prs[0],
            Pr {
                number: 3,
                title: "PR 3".to_string(),
                author: "alice".to_string(),
                branch: "alice/pr-3".to_string(),
                draft: true,
                additions: 30,
                deletions: 3,
                url: "https://github.com/o/r/pull/3".to_string(),
                ci: Ci::None,
                review: Review::None,
                reviewers: vec![],
                review_requested: false,
            }
        );
        assert!(!prs[1].draft);
    }

    #[test]
    fn ci_is_the_most_severe_check() {
        let passed = check_run("COMPLETED", "SUCCESS");
        let skipped = check_run("COMPLETED", "SKIPPED");
        let running = check_run("IN_PROGRESS", "");
        let failed = check_run("COMPLETED", "FAILURE");

        assert_eq!(ci_of(json!([])), Ci::None);
        assert_eq!(ci_of(json!([passed, skipped])), Ci::Passing);
        assert_eq!(ci_of(json!([passed, running])), Ci::Pending);
        assert_eq!(ci_of(json!([running, failed, passed])), Ci::Failing);
        assert_eq!(
            ci_of(json!([check_run("COMPLETED", "CANCELLED")])),
            Ci::Failing
        );
        assert_eq!(ci_of(json!([check_run("QUEUED", "")])), Ci::Pending);
    }

    #[test]
    fn ci_reads_commit_statuses() {
        assert_eq!(ci_of(json!([status_context("SUCCESS")])), Ci::Passing);
        assert_eq!(ci_of(json!([status_context("PENDING")])), Ci::Pending);
        assert_eq!(ci_of(json!([status_context("ERROR")])), Ci::Failing);
        assert_eq!(ci_of(json!([status_context("FAILURE")])), Ci::Failing);
    }

    #[test]
    fn review_takes_the_decision_or_the_latest_reviews() {
        let alice = json!({ "login": "alice" });
        let entry = |number, decision: &str, reviews: Value| {
            with_fields(
                listed(number, alice.clone(), number as u32),
                json!({ "reviewDecision": decision, "latestReviews": reviews }),
            )
        };
        let json = json!([
            entry(1, "", json!([])),
            entry(2, "", json!([review("bob", "COMMENTED")])),
            entry(
                3,
                "",
                json!([review("bob", "APPROVED"), review("carol", "COMMENTED")])
            ),
            entry(
                4,
                "",
                json!([
                    review("bob", "APPROVED"),
                    review("carol", "CHANGES_REQUESTED")
                ])
            ),
            entry(5, "REVIEW_REQUIRED", json!([review("bob", "COMMENTED")])),
            entry(6, "APPROVED", json!([])),
            entry(7, "CHANGES_REQUESTED", json!([])),
        ]);

        let prs = parse_prs(&json.to_string()).unwrap();
        let reviews: Vec<(u64, Review)> = prs.iter().map(|pr| (pr.number, pr.review)).collect();

        assert_eq!(
            reviews,
            [
                (7, Review::ChangesRequested),
                (6, Review::Approved),
                (5, Review::ReviewRequired),
                (4, Review::ChangesRequested),
                (3, Review::Approved),
                (2, Review::Commented),
                (1, Review::None),
            ]
        );
    }

    #[test]
    fn reviewers_skip_pending_reviews() {
        let pr = with_fields(
            listed(1, json!({ "login": "alice" }), 1),
            json!({
                "latestReviews": [
                    review("bob", "CHANGES_REQUESTED"),
                    review("carol", "PENDING"),
                    { "author": null, "state": "COMMENTED" },
                ],
                "reviewRequests": [{ "__typename": "Team", "name": "core", "slug": "core" }],
            }),
        );

        let prs = parse_prs(&json!([pr]).to_string()).unwrap();

        assert_eq!(
            prs[0].reviewers,
            [
                Reviewer {
                    login: "bob".to_string(),
                    state: "changes requested".to_string()
                },
                Reviewer {
                    login: "ghost".to_string(),
                    state: "commented".to_string()
                },
            ]
        );
        assert!(prs[0].review_requested);
    }

    #[test]
    fn prs_serialize_in_camel_case() {
        let pr = with_fields(
            listed(1, json!({ "login": "alice" }), 1),
            json!({ "reviewDecision": "CHANGES_REQUESTED", "statusCheckRollup": [check_run("IN_PROGRESS", "")] }),
        );

        let prs = parse_prs(&json!([pr]).to_string()).unwrap();
        let value = serde_json::to_value(&prs[0]).unwrap();

        assert_eq!(value["ci"], "pending");
        assert_eq!(value["review"], "changes-requested");
        assert_eq!(value["reviewRequested"], false);
        assert_eq!(value["reviewers"], json!([]));
    }

    #[test]
    fn drops_bot_authors() {
        let json = json!([
            listed(1, json!({ "login": "dependabot", "is_bot": true }), 1),
            listed(2, json!({ "login": "alice" }), 2),
            listed(3, json!({ "login": "bob", "is_bot": false }), 3),
        ]);

        let prs = parse_prs(&json.to_string()).unwrap();

        assert_eq!(numbers(&prs), [3, 2]);
    }

    #[test]
    fn null_author_is_ghost() {
        let json = json!([listed(1, Value::Null, 1)]);

        let prs = parse_prs(&json.to_string()).unwrap();

        assert_eq!(prs[0].author, "ghost");
    }

    #[test]
    fn a_cross_repo_pr_branch_has_the_owner() {
        let alice = json!({ "login": "alice" });
        let json = json!([
            with_fields(
                listed(1, alice.clone(), 4),
                head("patch-1", owner("bob"), true)
            ),
            with_fields(
                listed(2, alice.clone(), 3),
                head("patch-1", owner("o"), false)
            ),
            with_fields(
                listed(3, alice.clone(), 2),
                head("patch-1", owner(""), true)
            ),
            with_fields(listed(4, alice, 1), head("patch-1", Value::Null, true)),
        ]);

        let prs = parse_prs(&json.to_string()).unwrap();
        let branches: Vec<&str> = prs.iter().map(|pr| pr.branch.as_str()).collect();

        assert_eq!(
            branches,
            ["bob/patch-1", "patch-1", "ghost/patch-1", "ghost/patch-1"]
        );
    }

    #[test]
    fn wrong_shape_is_an_error() {
        for json in ["{}", r#"[{"number": 1}]"#, "not json"] {
            assert!(parse_prs(json).is_err(), "{json}");
        }
    }

    #[test]
    fn get_runs_gh_pr_list_in_the_main_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, _)) = repo_with_worktree(dir.path(), "work tree's") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = FixedRepos(vec![main.clone()]);
        let listed = json!([listed(12, json!({ "login": "alice" }), 1)]);
        let runner = StubRunner::new(vec![Ok(listed.to_string())]);

        let response = get_with(&repos, &prs_target(&main), &runner, &PrCache::default());

        assert_eq!(response.status, 200);
        assert_eq!(
            body(&response),
            json!({
                "error": null,
                "prs": [{
                    "number": 12,
                    "title": "PR 12",
                    "author": "alice",
                    "branch": "alice/pr-12",
                    "draft": false,
                    "additions": 120,
                    "deletions": 12,
                    "url": "https://github.com/o/r/pull/12",
                    "ci": "none",
                    "review": "none",
                    "reviewers": [],
                    "reviewRequested": false,
                }],
            })
        );

        let args = [
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            LIST_FIELDS,
        ];
        assert_eq!(runner.calls(), [(main, "gh".to_string(), strings(&args))]);
    }

    #[test]
    fn gh_failure_is_an_error_reply() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, _)) = repo_with_worktree(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![Err("not logged in".to_string()), Ok("{}".to_string())]);

        let cache = PrCache::default();

        let failed = get_with(&repos, &prs_target(&main), &runner, &cache);

        assert_eq!(failed.status, 200);
        assert_eq!(
            body(&failed),
            json!({ "error": "not logged in", "prs": [] })
        );

        let wrong_shape = body(&get_with(&repos, &prs_target(&main), &runner, &cache));

        assert!(wrong_shape["error"].is_string(), "{wrong_shape}");
        assert_eq!(wrong_shape["prs"], json!([]));
        assert_eq!(cache.list(&main.to_string_lossy()), None);
    }

    #[test]
    fn unconfigured_path_is_404() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path(), "work tree's") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let parent = main.parent().unwrap().to_path_buf();
        let repos = FixedRepos(vec![main]);
        let runner = StubRunner::new(vec![]);

        for target in [
            "/api/prs".to_string(),
            "/api/prs?path=".to_string(),
            prs_target(&linked),
            prs_target(&parent),
            cached_target(&linked),
            view_target(&linked, 7),
        ] {
            let response = get_with(&repos, &target, &runner, &PrCache::default());

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls().is_empty());
    }

    #[test]
    fn the_cached_route_serves_the_last_listing() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let alice = json!({ "login": "alice" });
        let first = json!([listed(12, alice.clone(), 2), listed(7, alice.clone(), 1)]);
        let second = json!([listed(7, alice, 1)]);
        let runner = StubRunner::new(vec![
            Ok(first.to_string()),
            Err("offline".to_string()),
            Ok(second.to_string()),
        ]);
        let cache = PrCache::default();
        let cached_numbers = || {
            let reply = body(&get_with(&repos, &cached_target(&main), &runner, &cache));
            assert_eq!(reply["error"], Value::Null);
            reply["prs"].as_array().map(|prs| {
                prs.iter()
                    .map(|pr| pr["number"].as_u64().unwrap())
                    .collect::<Vec<_>>()
            })
        };

        assert_eq!(cached_numbers(), None);

        get_with(&repos, &prs_target(&main), &runner, &cache);
        assert_eq!(cached_numbers(), Some(vec![12, 7]));

        get_with(&repos, &prs_target(&main), &runner, &cache);
        assert_eq!(
            cached_numbers(),
            Some(vec![12, 7]),
            "a failed listing keeps the cache"
        );

        get_with(&repos, &prs_target(&main), &runner, &cache);
        assert_eq!(cached_numbers(), Some(vec![7]));

        assert_eq!(runner.calls().len(), 3, "the cached route never runs gh");
    }

    #[test]
    fn the_view_route_refreshes_one_pr_and_its_cache_entry() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let alice = json!({ "login": "alice" });
        let open = |pr: Value| with_fields(pr, json!({ "state": "OPEN" }));

        let listing = json!([listed(12, alice.clone(), 2), listed(7, alice.clone(), 1)]);
        let renamed = open(with_fields(
            listed(12, alice.clone(), 3),
            json!({ "title": "Renamed" }),
        ));
        let merged = with_fields(listed(7, alice.clone(), 1), json!({ "state": "MERGED" }));
        let new = open(listed(30, alice, 4));
        let runner = StubRunner::new(vec![
            Ok(listing.to_string()),
            Ok(renamed.to_string()),
            Ok(merged.to_string()),
            Ok(new.to_string()),
        ]);
        let cache = PrCache::default();
        let path = main.to_string_lossy();
        let titles = || {
            let prs = cache.list(&path).unwrap();
            prs.into_iter()
                .map(|pr| format!("#{} {}", pr.number, pr.title))
                .collect::<Vec<_>>()
        };

        get_with(&repos, &prs_target(&main), &runner, &cache);

        let reply = body(&get_with(&repos, &view_target(&main, 12), &runner, &cache));
        assert_eq!(reply["error"], Value::Null);
        assert_eq!(reply["pr"]["number"], 12);
        assert_eq!(reply["pr"]["title"], "Renamed");
        assert_eq!(titles(), ["#12 Renamed", "#7 PR 7"]);

        let reply = body(&get_with(&repos, &view_target(&main, 7), &runner, &cache));
        assert_eq!(reply, json!({ "error": null, "pr": null }));
        assert_eq!(titles(), ["#12 Renamed"]);

        get_with(&repos, &view_target(&main, 30), &runner, &cache);
        assert_eq!(titles(), ["#30 PR 30", "#12 Renamed"]);

        let fields = format!("{LIST_FIELDS},state");
        let view = strings(&["pr", "view", "12", "--json", &fields]);
        assert_eq!(runner.calls()[1], (main, "gh".to_string(), view));
    }

    #[test]
    fn a_failed_view_keeps_the_cache() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let listing = json!([listed(12, json!({ "login": "alice" }), 2)]);
        let runner = StubRunner::new(vec![
            Ok(listing.to_string()),
            Err("not logged in".to_string()),
            Ok("not json".to_string()),
        ]);
        let cache = PrCache::default();

        get_with(&repos, &prs_target(&main), &runner, &cache);

        let failed = body(&get_with(&repos, &view_target(&main, 12), &runner, &cache));
        assert_eq!(failed, json!({ "error": "not logged in", "pr": null }));

        let wrong_shape = body(&get_with(&repos, &view_target(&main, 12), &runner, &cache));
        assert!(wrong_shape["error"].is_string(), "{wrong_shape}");
        assert_eq!(wrong_shape["pr"], Value::Null);

        assert_eq!(numbers(&cache.list(&main.to_string_lossy()).unwrap()), [12]);
    }

    #[test]
    fn a_bot_pr_view_is_dropped() {
        let bot = json!({ "login": "dependabot", "is_bot": true });
        let json = with_fields(listed(5, bot, 1), json!({ "state": "OPEN" }));

        assert_eq!(parse_open_pr(&json.to_string()), Ok(None));
    }

    #[test]
    fn a_view_without_a_listing_caches_nothing() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let pr = with_fields(
            listed(12, json!({ "login": "alice" }), 2),
            json!({ "state": "OPEN" }),
        );
        let runner = StubRunner::new(vec![Ok(pr.to_string())]);
        let cache = PrCache::default();

        let reply = body(&get_with(&repos, &view_target(&main, 12), &runner, &cache));

        assert_eq!(reply["pr"]["number"], 12);
        assert_eq!(cache.list(&main.to_string_lossy()), None);
    }

    #[test]
    fn get_routes_check_their_input() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![]);

        for target in [
            format!("/api/prs/view?path={}", encoded(&main)),
            post_target("view", &main, "0"),
            post_target("view", &main, "x"),
            format!("/api/prs/other?path={}", encoded(&main)),
        ] {
            let response = get_with(&repos, &target, &runner, &PrCache::default());

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls().is_empty());
    }

    #[test]
    fn system_runner_appends_the_brew_folders() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let print = ["-c", "printf %s \"$PATH\""];

        let path = SystemRunner { bin: None }
            .run(dir.path(), "sh", &print)
            .unwrap();

        assert!(
            path.ends_with(":/opt/homebrew/bin:/usr/local/bin"),
            "{path}"
        );

        let path = SystemRunner {
            bin: Some(bin.clone()),
        }
        .run(dir.path(), "sh", &print)
        .unwrap();

        assert!(path.starts_with(&format!("{}:", bin.display())), "{path}");
    }

    #[test]
    fn system_runner_gives_stdout_or_the_failure() {
        let dir = tempfile::tempdir().unwrap();
        let folder = fs::canonicalize(dir.path()).unwrap();
        let runner = SystemRunner { bin: None };
        let run = |script: &str| runner.run(&folder, "sh", &["-c", script]);

        assert_eq!(run("pwd -P"), Ok(format!("{}\n", folder.display())));
        assert_eq!(run("echo ' oops ' >&2; exit 1"), Err("oops".to_string()));
        assert_eq!(run("exit 3"), Err("exit status: 3".to_string()));
        assert!(runner.run(&folder, "no-such-program", &[]).is_err());
    }

    #[test]
    fn worktree_folder_takes_the_last_segment() {
        let main = Path::new("/s/o/r/main");

        assert_eq!(
            worktree_folder(main, "alice/fix-login"),
            Path::new("/s/o/r/fix-login")
        );
        assert_eq!(worktree_folder(main, "fix"), Path::new("/s/o/r/fix"));
    }

    #[test]
    fn checkout_creates_a_sibling_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);
        let folder = main.parent().unwrap().join("fix-login");

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));
        assert_eq!(body(&response), json!({ "path": folder }));
        assert_eq!(checked_out(dir.path()), "pr checkout 12\n");
        assert_eq!(
            linked_worktrees(&main),
            [Worktree {
                path: folder.display().to_string(),
                branch: Some("alice/fix-login".to_string()),
                head: None,
            }]
        );
    }

    #[test]
    fn checkout_under_a_symlink_returns_the_listed_worktree_path() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let Some(main) = main_checkout_at(&link, "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);

        assert_ne!(fs::canonicalize(&main).unwrap(), main);

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));

        let path = body(&response)["path"].as_str().unwrap().to_string();

        assert!(is_worktree(&repos, Path::new(&path)), "{path}");
    }

    #[test]
    fn checkout_of_a_cross_repo_pr_passes_its_local_branch() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh_head(dir.path(), &head("patch-1", owner("bob"), true));
        let repos = FixedRepos(vec![main.clone()]);
        let folder = main.parent().unwrap().join("patch-1");

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));
        assert_eq!(body(&response), json!({ "path": folder }));
        assert_eq!(
            checked_out(dir.path()),
            "pr checkout 12 --branch bob/patch-1\n"
        );
        assert_eq!(
            linked_worktrees(&main),
            [Worktree {
                path: folder.display().to_string(),
                branch: Some("bob/patch-1".to_string()),
                head: None,
            }]
        );
    }

    #[test]
    fn checkout_skips_the_worktree_of_another_cross_repo_pr_on_the_same_branch() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh_head(dir.path(), &head("patch-1", owner("carol"), true));
        let repos = FixedRepos(vec![main.clone()]);
        let parent = main.parent().unwrap().to_path_buf();
        let bob = parent.join("bob-patch-1");
        let carol = parent.join("patch-1");

        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "bob/patch-1",
                "../bob-patch-1",
            ],
        );

        let target = post_target("checkout", &main, "15");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));
        assert_eq!(body(&response), json!({ "path": carol }));
        assert_eq!(
            checked_out(dir.path()),
            "pr checkout 15 --branch carol/patch-1\n"
        );

        set_head(
            &dir.path().join("bin"),
            &head("patch-1", owner("bob"), true),
        );

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));
        assert_eq!(body(&response), json!({ "path": bob }));
        assert_eq!(names(&parent), ["bob-patch-1", "main", "patch-1"]);
    }

    #[test]
    fn checkout_of_a_checked_out_pr_returns_its_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);
        let parent = main.parent().unwrap().to_path_buf();
        let login = parent.join("login");

        git(
            &main,
            &["worktree", "add", "-q", "-b", "alice/fix-login", "../login"],
        );

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));
        assert_eq!(body(&response), json!({ "path": login }));
        assert_eq!(names(&parent), ["login", "main"]);
    }

    #[test]
    fn checkout_with_a_local_branch_without_worktree_is_409() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);

        git(&main, &["branch", "alice/fix-login"]);

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 409);
        assert_eq!(
            text(&response),
            "Branch alice/fix-login exists locally without a worktree"
        );
        assert_eq!(names(main.parent().unwrap()), ["main"]);
    }

    #[test]
    fn checkout_of_the_main_branch_is_409() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "main");
        let repos = FixedRepos(vec![main.clone()]);

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 409);
        assert_eq!(
            text(&response),
            "Branch main exists locally without a worktree"
        );
        assert_eq!(names(main.parent().unwrap()), ["main"]);
        assert!(linked_worktrees(&main).is_empty());
    }

    #[test]
    fn checkout_into_an_existing_folder_is_409() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);
        let folder = main.parent().unwrap().join("fix-login");
        fs::create_dir(&folder).unwrap();

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 409);
        assert_eq!(
            text(&response),
            format!("{} already exists", folder.display())
        );
        assert!(linked_worktrees(&main).is_empty());
    }

    #[test]
    fn gh_checkout_failure_removes_the_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "main") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);
        fs::write(dir.path().join("bin/fail"), "").unwrap();

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 500);
        assert_eq!(text(&response), "boom");
        assert_eq!(names(main.parent().unwrap()), ["main"]);
        assert!(linked_worktrees(&main).is_empty());
    }

    #[test]
    fn checkout_handles_spaces_and_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "work tree's") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);
        let folder = main.parent().unwrap().join("fix-login");

        let target = post_target("checkout", &main, "12");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 200, "{}", text(&response));
        assert_eq!(body(&response), json!({ "path": folder }));
        assert_eq!(
            linked_worktrees(&main),
            [Worktree {
                path: folder.display().to_string(),
                branch: Some("alice/fix-login".to_string()),
                head: None,
            }]
        );
    }

    #[test]
    fn open_runs_gh_pr_view_web() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path(), "work tree's") else {
            eprintln!("skipped: git is missing");
            return;
        };
        let runner = stub_gh(dir.path(), "alice/fix-login");
        let repos = FixedRepos(vec![main.clone()]);

        let target = post_target("open", &main, "7");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 204, "{}", text(&response));
        assert_eq!(
            fs::read_to_string(dir.path().join("bin/viewed")).unwrap(),
            "pr view 7 --web\n"
        );
    }

    #[test]
    fn open_failure_is_500_with_the_error() {
        let main = PathBuf::from("/s/o/r/main");
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![Err("not logged in".to_string())]);

        let target = post_target("open", &main, "7");
        let response = post_with(&repos, &request(&target, &[]), &runner);

        assert_eq!(response.status, 500);
        assert_eq!(text(&response), "not logged in");

        let view = strings(&["pr", "view", "7", "--web"]);
        assert_eq!(runner.calls(), [(main, "gh".to_string(), view)]);
    }

    #[test]
    fn post_routes_check_origin_and_input() {
        let main = PathBuf::from("/s/o/r/main");
        let other = Path::new("/s/o/r/other");
        let repos = FixedRepos(vec![main.clone()]);
        let runner = StubRunner::new(vec![]);

        for route in ["checkout", "open"] {
            let foreign = request(
                &post_target(route, &main, "7"),
                &[("Origin", "http://evil")],
            );

            assert_eq!(post_with(&repos, &foreign, &runner).status, 403, "{route}");

            for target in [
                post_target(route, &main, "0"),
                post_target(route, &main, "-1"),
                post_target(route, &main, "x"),
                post_target(route, &main, ""),
                format!("/api/prs/{route}?path={}", encoded(&main)),
                post_target(route, other, "7"),
            ] {
                let response = post_with(&repos, &request(&target, &[]), &runner);

                assert_eq!(response.status, 404, "{target}");
            }
        }

        for target in [
            post_target("other", &main, "7"),
            format!("/api/prs?path={}&number=7", encoded(&main)),
        ] {
            let response = post_with(&repos, &request(&target, &[]), &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls().is_empty());
    }
}
