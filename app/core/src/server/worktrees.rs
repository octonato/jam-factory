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

//! Lists the titles and line counts of, creates and removes the linked worktrees of the configured main checkouts.
//! Routes: `GET /api/worktrees/stats?path=<main checkout>`, `POST /api/worktrees?path=<main checkout>&name=<name>`
//! and `POST /api/worktrees/remove?path=<worktree>`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;

use super::api::{json_response, query_field, same_origin, segments};
use super::http::{Request, Response};
use super::prs::{Runner, SystemRunner, configured_main, worktree_folder};
use super::repos::{Repo, RepoList, list};
use super::terminals::Terminals;

const TEXT: &str = "text/plain; charset=utf-8";

/// The title and the line counts of a linked worktree since it forked from the branch of its main checkout.
#[derive(Serialize)]
struct WorktreeStats {
    path: String,
    title: Option<String>,
    additions: Option<u64>,
    deletions: Option<u64>,
}

/// Answers `GET /api/worktrees/stats` with `repos`.
pub fn get(repos: &dyn RepoList, target: &str) -> Response {
    if segments(target)[..] != ["", "api", "worktrees", "stats"] {
        return Response::error(404);
    }

    let Some(main) = configured_main(repos, target) else {
        return Response::error(404);
    };

    json_response(&json!({ "worktrees": all_stats(&main, &SystemRunner { bin: None }) }))
}

/// The stats of each linked worktree of `main`.
fn all_stats(main: &Repo, runner: &dyn Runner) -> Vec<WorktreeStats> {
    let base = match &main.branch {
        Some(branch) => Some(branch.clone()),
        None => runner
            .run(Path::new(&main.path), "git", &["rev-parse", "HEAD"])
            .ok()
            .map(|sha| sha.trim().to_string()),
    };

    main.worktrees
        .iter()
        .map(|worktree| stats(&worktree.path, base.as_deref(), runner))
        .collect()
}

/// The stats of the worktree at `path` against its fork point from `base`, with no title and no counts when git fails.
fn stats(path: &str, base: Option<&str>, runner: &dyn Runner) -> WorktreeStats {
    let dir = Path::new(path);

    let fork = base.and_then(|base| {
        runner
            .run(dir, "git", &["merge-base", base, "HEAD"])
            .ok()
            .map(|sha| sha.trim().to_string())
    });

    let Some(fork) = fork else {
        return WorktreeStats {
            path: path.to_string(),
            title: None,
            additions: None,
            deletions: None,
        };
    };

    let range = format!("{fork}..HEAD");
    let title = runner
        .run(dir, "git", &["log", "--reverse", "--format=%s", &range])
        .ok()
        .and_then(|log| first_line(&log));

    let counts = runner
        .run(dir, "git", &["diff", "--numstat", &fork])
        .ok()
        .map(|numstat| line_counts(&numstat));

    WorktreeStats {
        path: path.to_string(),
        title,
        additions: counts.map(|(additions, _)| additions),
        deletions: counts.map(|(_, deletions)| deletions),
    }
}

/// The first non-empty line of `text`, trimmed.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// The added and deleted lines in the output of `git diff --numstat`. Binary files count zero lines.
fn line_counts(numstat: &str) -> (u64, u64) {
    numstat
        .lines()
        .fold((0, 0), |(additions, deletions), line| {
            let mut fields = line.split('\t');
            let mut count = || fields.next().and_then(|field| field.parse::<u64>().ok());

            let added = count().unwrap_or(0);
            let deleted = count().unwrap_or(0);

            (additions + added, deletions + deleted)
        })
}

/// Answers `POST /api/worktrees` and `POST /api/worktrees/remove` with `repos` and `terminals`.
pub fn post(repos: &dyn RepoList, terminals: &Terminals, request: &Request) -> Response {
    post_with(repos, terminals, request, &SystemRunner { bin: None })
}

/// Answers `POST /api/worktrees` and `POST /api/worktrees/remove` with `repos` and `terminals`, running `git` with `runner`.
pub fn post_with(
    repos: &dyn RepoList,
    terminals: &Terminals,
    request: &Request,
    runner: &dyn Runner,
) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    match segments(&request.target)[..] {
        ["", "api", "worktrees"] => add(repos, request, runner),
        ["", "api", "worktrees", "remove"] => remove(repos, terminals, request, runner),
        _ => Response::error(404),
    }
}

/// Creates the worktree named by the `name` query field of `request` for the main checkout of its `path` field.
fn add(repos: &dyn RepoList, request: &Request, runner: &dyn Runner) -> Response {
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

/// Updates the main checkout `main`, then creates the branch `branch` at its HEAD, in a new sibling worktree.
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

    if let Err(error) = refresh(main, runner) {
        return text_response(
            409,
            &format!("Could not update {}: {error}", main.display()),
        );
    }

    let folder_arg = folder.to_string_lossy();
    let add = ["worktree", "add", "-b", branch, &folder_arg, "HEAD"];

    if let Err(error) = runner.run(main, "git", &add) {
        return text_response(500, &error);
    }

    let listed = fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());

    json_response(&json!({ "path": listed.to_string_lossy() }))
}

/// Fast-forwards the checked-out branch of the main checkout `main` to its upstream, when it has one.
fn refresh(main: &Path, runner: &dyn Runner) -> Result<(), String> {
    let upstream = [
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ];

    if runner.run(main, "git", &upstream).is_err() {
        return Ok(());
    }

    runner.run(main, "git", &["pull", "--ff-only"]).map(|_| ())
}

/// Removes the linked worktree named by the `path` query field of `request`, deletes its branch when merged, and kills its terminals.
fn remove(
    repos: &dyn RepoList,
    terminals: &Terminals,
    request: &Request,
    runner: &dyn Runner,
) -> Response {
    let Some(path) = query_field(&request.target, "path") else {
        return Response::error(404);
    };

    let Some((main, branch)) = linked_worktree(repos, Path::new(&path)) else {
        return Response::error(404);
    };

    let remove = ["worktree", "remove", path.as_str()];

    if let Err(error) = runner.run(&main, "git", &remove) {
        return text_response(409, &error);
    }

    if let Some(branch) = branch {
        let _ = runner.run(&main, "git", &["branch", "-d", &branch]);
    }

    terminals.kill_in(Path::new(&path));

    Response::new(204, TEXT, Vec::new())
}

/// The main checkout of `repos` that lists the linked worktree at `path`, with the branch of that worktree.
fn linked_worktree(repos: &dyn RepoList, path: &Path) -> Option<(PathBuf, Option<String>)> {
    let paths = repos.paths().ok()?;

    list(&paths).into_iter().find_map(|repo| {
        let worktree = repo
            .worktrees
            .into_iter()
            .find(|worktree| Path::new(&worktree.path) == path)?;

        Some((PathBuf::from(repo.path), worktree.branch))
    })
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
    use crate::server::pty::Program;
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

        let response = post(&repos, &Terminals::default(), &request(&main, " fix-it "));

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

        let response = post(&repos, &Terminals::default(), &request(&main, "fix-it"));

        assert_eq!(response.status, 200, "{}", text(&response));

        let folder = main.parent().unwrap().join("fix-it");

        assert_eq!(branch_of(&repos, &main, &folder).as_deref(), Some("fix-it"));
    }

    /// Commits an empty commit with the message `message` in `dir`.
    fn commit(dir: &Path, message: &str) {
        git(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                message,
            ],
        );
    }

    /// The full sha of `revision` in `dir`.
    fn sha(dir: &Path, revision: &str) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", revision])
            .output()
            .unwrap();

        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// Makes `main` track the bare remote `remote.git` next to it, and returns a clone that pushes to that remote.
    fn track_remote(main: &Path) -> PathBuf {
        let dir = main.parent().unwrap().parent().unwrap();
        let remote = dir.join("remote.git");
        let pusher = dir.join("pusher");

        git(
            dir,
            &[
                "clone",
                "-q",
                "--bare",
                &main.to_string_lossy(),
                "remote.git",
            ],
        );
        git(
            main,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        git(main, &["fetch", "-q", "origin"]);
        git(main, &["branch", "-q", "-u", "origin/main"]);
        git(dir, &["clone", "-q", &remote.to_string_lossy(), "pusher"]);

        pusher
    }

    #[test]
    fn starts_the_worktree_at_the_remote_head() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let pusher = track_remote(&main);

        commit(&pusher, "remote work");
        git(&pusher, &["push", "-q", "origin", "main"]);

        let response = post(
            &repos(&main, None),
            &Terminals::default(),
            &request(&main, "fix"),
        );

        assert_eq!(response.status, 200, "{}", text(&response));

        let folder = main.parent().unwrap().join("fix");
        let remote_head = sha(&pusher, "HEAD");

        assert_eq!(sha(&main, "HEAD"), remote_head);
        assert_eq!(sha(&folder, "HEAD"), remote_head);
    }

    #[test]
    fn refuses_when_main_cannot_fast_forward() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let pusher = track_remote(&main);

        commit(&pusher, "remote work");
        git(&pusher, &["push", "-q", "origin", "main"]);
        commit(&main, "local work");

        let response = post(
            &repos(&main, None),
            &Terminals::default(),
            &request(&main, "fix"),
        );

        assert_eq!(response.status, 409, "{}", text(&response));
        assert!(
            text(&response).starts_with("Could not update"),
            "{}",
            text(&response)
        );
        assert!(!main.parent().unwrap().join("fix").exists());
        assert!(!has_branch(&main, "fix"));
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
            let response = post(&repos, &Terminals::default(), &request(&main, name));

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
        assert_eq!(
            post(&repos(&main, None), &Terminals::default(), &foreign).status,
            403
        );

        let mut other_route = request(&main, "x");
        other_route.target = other_route.target.replace("worktrees", "worktrees/x");
        assert_eq!(
            post(&repos(&main, None), &Terminals::default(), &other_route).status,
            404
        );

        let unlisted = repos(&main.parent().unwrap().join("other"), None);
        assert_eq!(
            post(&unlisted, &Terminals::default(), &request(&main, "x")).status,
            404
        );

        let broken = UserRepos {
            paths: vec![main.clone()],
            user: Err("bad config".to_string()),
        };
        let response = post(&broken, &Terminals::default(), &request(&main, "x"));
        assert_eq!(response.status, 500);
        assert_eq!(text(&response), "bad config");
    }

    /// A `POST` on `/api/worktrees/remove` for the worktree at `path`, from the board's host.
    fn remove_request(path: &Path) -> Request {
        let path = utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string();

        Request {
            method: "POST".to_string(),
            target: format!("/api/worktrees/remove?path={path}"),
            headers: vec![("Host".to_string(), "127.0.0.1:1".to_string())],
            body: Vec::new(),
        }
    }

    /// Adds the linked worktree `name` on the new branch `name` next to `main`, and returns its folder.
    fn add_worktree(main: &Path, name: &str) -> PathBuf {
        let folder = main.parent().unwrap().join(name);

        git(
            main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                name,
                &folder.to_string_lossy(),
            ],
        );

        folder
    }

    /// Writes `text` to `file` in `dir` and commits it with the message `message`.
    fn commit_file(dir: &Path, file: &str, text: &str, message: &str) {
        fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]);
        commit(dir, message);
    }

    /// The `/api/worktrees/stats` reply for the main checkout `main` of `repos`.
    fn stats_reply(repos: &UserRepos, main: &Path) -> Response {
        let path = utf8_percent_encode(&main.to_string_lossy(), NON_ALPHANUMERIC).to_string();

        get(repos, &format!("/api/worktrees/stats?path={path}"))
    }

    #[test]
    fn line_counts_sum_the_numstat_and_skip_binary_files() {
        assert_eq!(line_counts(""), (0, 0));
        assert_eq!(
            line_counts("3\t1\ta.txt\n-\t-\timage.png\n2\t0\tb.txt\n"),
            (5, 1)
        );
    }

    #[test]
    fn stats_give_the_first_commit_and_the_lines_since_the_fork_point() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = repos(&main, None);

        commit_file(&main, "a.txt", "one\ntwo\n", "base");
        let fix = add_worktree(&main, "fix");
        let empty = add_worktree(&main, "empty");

        commit_file(&fix, "a.txt", "one\n2\nthree\n", "feat: first");
        commit(&fix, "second");
        fs::write(fix.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
        commit(&main, "later main work");

        let response = stats_reply(&repos, &main);

        assert_eq!(response.status, 200, "{}", text(&response));

        let reply: Value = serde_json::from_slice(&response.body).unwrap();
        let of = |folder: &Path| {
            reply["worktrees"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["path"] == folder.to_string_lossy().as_ref())
                .cloned()
                .unwrap()
        };

        assert_eq!(reply["worktrees"].as_array().unwrap().len(), 2);
        assert_eq!(
            of(&fix),
            json!({ "path": fix, "title": "feat: first", "additions": 3, "deletions": 1 })
        );
        assert_eq!(
            of(&empty),
            json!({ "path": empty, "title": null, "additions": 0, "deletions": 0 })
        );
    }

    #[test]
    fn stats_answer_only_their_route_for_a_configured_main_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let linked = add_worktree(&main, "fix");

        assert_eq!(stats_reply(&repos(&main, None), &linked).status, 404);
        assert_eq!(get(&repos(&main, None), "/api/worktrees/stats").status, 404);
        assert_eq!(get(&repos(&main, None), "/api/worktrees/other").status, 404);
    }

    /// Whether the main checkout `main` holds the branch `branch`.
    fn has_branch(main: &Path, branch: &str) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(main)
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .stdout(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn removes_a_worktree_with_its_merged_branch_and_its_terminals() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = repos(&main, None);
        let folder = add_worktree(&main, "fix");
        let other = add_worktree(&main, "other");

        let terminals = Terminals::default();
        let program = Program {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), "exec sleep 60".to_string()],
        };
        let (gone, _) = terminals
            .open(&folder, "shell", &program, "shell 1", 80, 24)
            .unwrap();
        let (kept, _) = terminals
            .open(&other, "shell", &program, "shell 1", 80, 24)
            .unwrap();

        let response = post(&repos, &terminals, &remove_request(&folder));

        assert_eq!(response.status, 204, "{}", text(&response));
        assert!(!folder.exists());
        assert!(!has_branch(&main, "fix"));
        assert!(terminals.get(gone).is_none());
        assert!(terminals.get(kept).is_some());

        terminals.kill_all();
    }

    #[test]
    fn keeps_an_unmerged_branch() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let folder = add_worktree(&main, "fix");

        git(
            &folder,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "work",
            ],
        );

        let response = post(
            &repos(&main, None),
            &Terminals::default(),
            &remove_request(&folder),
        );

        assert_eq!(response.status, 204, "{}", text(&response));
        assert!(!folder.exists());
        assert!(has_branch(&main, "fix"));
    }

    #[test]
    fn refuses_a_dirty_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let folder = add_worktree(&main, "fix");

        fs::write(folder.join("draft.txt"), "draft").unwrap();

        let response = post(
            &repos(&main, None),
            &Terminals::default(),
            &remove_request(&folder),
        );

        assert_eq!(response.status, 409);
        assert!(text(&response).contains("untracked"), "{}", text(&response));
        assert!(folder.join("draft.txt").exists());
        assert!(has_branch(&main, "fix"));
    }

    #[test]
    fn refuses_the_main_checkout_and_unlisted_folders() {
        let dir = tempfile::tempdir().unwrap();
        let Some(main) = main_checkout(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };
        let repos = repos(&main, None);
        let terminals = Terminals::default();

        for path in [main.clone(), main.parent().unwrap().join("unknown")] {
            let response = post(&repos, &terminals, &remove_request(&path));

            assert_eq!(response.status, 404, "{}", path.display());
        }

        let mut no_path = remove_request(&main);
        no_path.target = "/api/worktrees/remove".to_string();
        assert_eq!(post(&repos, &terminals, &no_path).status, 404);

        let mut foreign = remove_request(&main);
        foreign
            .headers
            .push(("Origin".to_string(), "http://evil".to_string()));
        assert_eq!(post(&repos, &terminals, &foreign).status, 403);

        assert!(main.exists());
    }
}
