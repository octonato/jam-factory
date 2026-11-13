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

//! Lists the worktrees of the configured main checkouts with `git worktree list --porcelain`.

use std::path::{Path, PathBuf};
use std::process::Command;

use jam_factory_store::root::resolve_repos;
use serde::Serialize;
use serde_json::{Value, json};

/// The length of a short sha.
const SHORT_SHA: usize = 7;

/// A source of the configured main checkout paths.
pub trait RepoList: Send + Sync {
    /// The paths of the main checkouts, or the reason they cannot be read.
    fn paths(&self) -> Result<Vec<PathBuf>, String>;
}

/// The repos of `~/.jam-factory/config.json` under `home`, read again on every call.
pub struct ConfigRepos {
    pub home: PathBuf,
}

impl RepoList for ConfigRepos {
    fn paths(&self) -> Result<Vec<PathBuf>, String> {
        resolve_repos(&self.home).map_err(|error| error.to_string())
    }
}

/// A fixed list of main checkout paths.
pub struct FixedRepos(pub Vec<PathBuf>);

impl RepoList for FixedRepos {
    fn paths(&self) -> Result<Vec<PathBuf>, String> {
        Ok(self.0.clone())
    }
}

/// A worktree with its branch, or with its short sha when its HEAD is detached.
#[derive(Debug, PartialEq, Serialize)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,
}

/// A main checkout with its linked worktrees, or with the reason they cannot be listed.
#[derive(Debug, PartialEq, Serialize)]
pub struct Repo {
    pub org: String,
    pub repo: String,
    pub path: String,
    pub error: Option<String>,
    pub worktrees: Vec<Worktree>,
}

/// The linked worktrees in the output of `git worktree list --porcelain`: every entry after the first.
pub fn parse_porcelain(text: &str) -> Vec<Worktree> {
    text.split("\n\n").skip(1).filter_map(parse_entry).collect()
}

/// The worktree of one porcelain entry, or `None` for a bare or empty entry.
fn parse_entry(entry: &str) -> Option<Worktree> {
    let mut path = None;
    let mut head = None;
    let mut branch = None;

    for line in entry.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));

        match key {
            "worktree" => path = Some(value),
            "HEAD" => head = Some(value),
            "branch" => branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value)),
            "bare" => return None,
            _ => {}
        }
    }

    let head = match branch {
        Some(_) => None,
        None => head.map(|head| head.chars().take(SHORT_SHA).collect()),
    };

    Some(Worktree {
        path: path?.to_string(),
        branch: branch.map(str::to_string),
        head,
    })
}

/// The main checkouts at `paths`, in order, each with its worktrees or the reason they cannot be listed.
pub fn list(paths: &[PathBuf]) -> Vec<Repo> {
    paths.iter().map(|path| repo(path)).collect()
}

/// The main checkout at `path`.
fn repo(path: &Path) -> Repo {
    let (org, repo) = names(path);

    let (error, worktrees) = match worktrees(path) {
        Ok(worktrees) => (None, worktrees),
        Err(reason) => (Some(reason), Vec::new()),
    };

    Repo {
        org,
        repo,
        path: path.display().to_string(),
        error,
        worktrees,
    }
}

/// The org and the repo of the main checkout at `path`: the names of its grandparent and parent folders.
fn names(path: &Path) -> (String, String) {
    let name = |folder: Option<&Path>| {
        folder
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    let parent = path.parent();

    (name(parent.and_then(Path::parent)), name(parent))
}

/// The linked worktrees that `git worktree list --porcelain` gives in the folder `path`.
fn worktrees(path: &Path) -> Result<Vec<Worktree>, String> {
    if !path.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|error| format!("Cannot run git: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();

        return Err(if stderr.is_empty() {
            format!("git worktree list failed: {}", output.status)
        } else {
            stderr.to_string()
        });
    }

    Ok(parse_porcelain(&String::from_utf8_lossy(&output.stdout)))
}

/// Whether `path` is a linked worktree path of one of the main checkouts of `repos`.
pub fn is_worktree(repos: &dyn RepoList, path: &Path) -> bool {
    let Ok(paths) = repos.paths() else {
        return false;
    };

    list(&paths)
        .iter()
        .flat_map(|repo| &repo.worktrees)
        .any(|worktree| Path::new(&worktree.path) == path)
}

/// The main checkouts of `repos`, as `{"error": null | "<config error>", "repos": [...]}`.
pub fn repos_json(repos: &dyn RepoList) -> Value {
    match repos.paths() {
        Ok(paths) => json!({ "error": null, "repos": list(&paths) }),
        Err(error) => json!({ "error": error, "repos": [] }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Stdio;

    use super::*;

    /// A repo list whose config cannot be read.
    struct BrokenRepos;

    impl RepoList for BrokenRepos {
        fn paths(&self) -> Result<Vec<PathBuf>, String> {
            Err("x".to_string())
        }
    }

    fn worktree(path: &str, branch: Option<&str>, head: Option<&str>) -> Worktree {
        Worktree {
            path: path.to_string(),
            branch: branch.map(str::to_string),
            head: head.map(str::to_string),
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

    /// The canonical main checkout `o/r/main` under `dir` and its linked worktree `o/r/feature x`, or `None` without git.
    fn repo_with_worktree(dir: &Path) -> Option<(PathBuf, PathBuf)> {
        if Command::new("git").arg("--version").output().is_err() {
            return None;
        }

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

        Some((main, linked))
    }

    #[test]
    fn parses_branches_and_detached_heads() {
        let text = "worktree /a\nHEAD 1234567890abcdef\nbranch refs/heads/main\n\n\
                    worktree /b\nHEAD 1234567890abcdef\ndetached\n\n\
                    worktree /c\nHEAD abcdef1234567890\nbranch refs/heads/old\n\
                    prunable gitdir file points to non-existent location\n";

        assert_eq!(
            parse_porcelain(text),
            vec![
                worktree("/b", None, Some("1234567")),
                worktree("/c", Some("old"), None),
            ]
        );
    }

    #[test]
    fn skips_the_main_entry_bare_or_not() {
        let bare = "worktree /bare\nbare\n\n\
                    worktree /a\nHEAD 1234567890abcdef\nbranch refs/heads/main\n\n";
        let main = "worktree /main\nHEAD 1234567890abcdef\nbranch refs/heads/main\n\n";

        assert_eq!(
            parse_porcelain(bare),
            vec![worktree("/a", Some("main"), None)]
        );
        assert_eq!(parse_porcelain(main), vec![]);
    }

    #[test]
    fn names_org_and_repo_from_the_path() {
        let (org, repo) = names(Path::new("/s/akka/sdk/main"));

        assert_eq!(org, "akka");
        assert_eq!(repo, "sdk");
    }

    #[test]
    fn lists_a_repo_with_a_linked_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };

        let repos = list(std::slice::from_ref(&main));

        assert_eq!(
            repos,
            vec![Repo {
                org: "o".to_string(),
                repo: "r".to_string(),
                path: main.display().to_string(),
                error: None,
                worktrees: vec![worktree(
                    &linked.display().to_string(),
                    Some("feature"),
                    None
                )],
            }]
        );
    }

    #[test]
    fn missing_or_non_git_path_is_an_error_entry() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        let empty = dir.path().join("empty");
        fs::create_dir(&empty).unwrap();

        let repos = list(&[missing, empty]);

        assert_eq!(repos.len(), 2);

        for repo in repos {
            assert!(repo.error.is_some(), "{}", repo.path);
            assert!(repo.worktrees.is_empty(), "{}", repo.path);
        }
    }

    #[test]
    fn is_worktree_accepts_only_listed_paths() {
        let dir = tempfile::tempdir().unwrap();
        let Some((main, linked)) = repo_with_worktree(dir.path()) else {
            eprintln!("skipped: git is missing");
            return;
        };

        let parent = main.parent().unwrap().to_path_buf();
        let sibling = parent.join("sibling");
        let sub = main.join("sub");
        fs::create_dir(&sibling).unwrap();
        fs::create_dir(&sub).unwrap();

        let repos = FixedRepos(vec![main.clone()]);

        assert!(is_worktree(&repos, &linked));

        for path in [main, parent, sibling, sub] {
            assert!(!is_worktree(&repos, &path), "{}", path.display());
        }
    }

    #[test]
    fn config_repos_re_reads_the_file() {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".jam-factory").join("config.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();

        let repos = ConfigRepos {
            home: home.path().to_path_buf(),
        };

        fs::write(&config, r#"{"repos": []}"#).unwrap();
        assert_eq!(repos.paths(), Ok(vec![]));

        fs::write(&config, r#"{"repos": ["/x/y/main"]}"#).unwrap();
        assert_eq!(repos.paths(), Ok(vec![PathBuf::from("/x/y/main")]));
    }

    #[test]
    fn bad_config_gives_a_top_level_error() {
        assert_eq!(
            repos_json(&BrokenRepos),
            json!({ "error": "x", "repos": [] })
        );
    }
}
