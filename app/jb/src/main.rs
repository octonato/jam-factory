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

//! Opens a worktree of the configured repos in its own Jam Board window.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use jam_board_core::server::repos::{FixedRepos, RepoList, is_worktree};
use jam_factory_store::root::resolve_repos;
use url::Url;

const USAGE: &str = "Usage: jb [dir]\n\nOpens the worktree that holds dir, or the current folder, in its own Jam Board window.";

const LINK: &str = "jam-board://open";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if matches!(args.as_slice(), [flag] if flag == "-h" || flag == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("jb: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Opens the worktree that holds the folder in `args`, or the current folder.
fn run(args: &[String]) -> Result<(), String> {
    let dir = match args {
        [] => env::current_dir().map_err(|e| format!("Cannot read the current folder: {e}"))?,
        [dir] if !dir.starts_with('-') => PathBuf::from(dir),
        _ => return Err(USAGE.to_string()),
    };

    let home = env::home_dir().ok_or("Cannot find the home folder")?;
    let paths = resolve_repos(&home).map_err(|e| e.to_string())?;

    let worktree = worktree_of(&FixedRepos(paths), &dir)?;

    open(&link(&worktree)?)
}

/// Returns the top folder of the git worktree that holds `dir`, when it is a linked worktree of `repos`.
fn worktree_of(repos: &dyn RepoList, dir: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("Cannot run git: {e}"))?;

    if !output.status.success() {
        return Err(format!("{} is not in a git worktree", dir.display()));
    }

    let top = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());

    if !is_worktree(repos, &top) {
        return Err(format!(
            "{} is not a linked worktree of a repo in ~/.jam-factory/config.json",
            top.display()
        ));
    }

    Ok(top)
}

/// Returns the `jam-board://open?path=…` link to the worktree at `path`.
fn link(path: &Path) -> Result<Url, String> {
    let path = path
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", path.display()))?;

    let mut url = Url::parse(LINK).map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("path", path);

    Ok(url)
}

/// Opens `url` with the macOS `open` command.
fn open(url: &Url) -> Result<(), String> {
    let status = Command::new("open")
        .arg(url.as_str())
        .status()
        .map_err(|e| format!("Cannot run open: {e}"))?;

    if !status.success() {
        return Err(format!(
            "open {url} failed: {status}. Is Jam Board in /Applications?"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Stdio;

    use super::*;

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

    /// The canonical main checkout `o/r/main` under `dir` and its linked worktree `o/r/feature x`.
    fn repo_with_worktree(dir: &Path) -> (PathBuf, PathBuf) {
        let dir = fs::canonicalize(dir).unwrap();
        let main = dir.join("o/r/main");
        let linked = dir.join("o/r/feature x");

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
            &["worktree", "add", "-q", "-b", "feature", "../feature x"],
        );

        (main, linked)
    }

    #[test]
    fn finds_the_linked_worktree_from_its_top_or_a_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        let (main, linked) = repo_with_worktree(dir.path());
        let sub = linked.join("a/b");
        fs::create_dir_all(&sub).unwrap();

        let repos = FixedRepos(vec![main]);

        assert_eq!(worktree_of(&repos, &linked), Ok(linked.clone()));
        assert_eq!(worktree_of(&repos, &sub), Ok(linked));
    }

    #[test]
    fn rejects_a_main_checkout_an_unlisted_repo_and_a_plain_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (main, linked) = repo_with_worktree(dir.path());
        let plain = tempfile::tempdir().unwrap();

        let listed = FixedRepos(vec![main.clone()]);
        let unlisted = FixedRepos(vec![]);

        assert!(worktree_of(&listed, &main).is_err());
        assert!(worktree_of(&unlisted, &linked).is_err());
        assert!(worktree_of(&listed, plain.path()).is_err());
    }

    #[test]
    fn the_link_carries_the_encoded_path() {
        let url = link(Path::new("/a/feature x+y")).unwrap();

        assert_eq!(url.as_str(), "jam-board://open?path=%2Fa%2Ffeature+x%2By");
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "path").unwrap().1,
            "/a/feature x+y"
        );
    }
}
