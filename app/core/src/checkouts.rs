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

//! Lists the checkouts of a repo.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

use crate::env::Env;

/// A clone or worktree of a repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub path: PathBuf,
    pub dir_name: String,
    pub branch: Option<String>,
}

/// The checkouts of a repo, or `NotCloned` when its directory is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoCheckouts {
    NotCloned,
    Checkouts(Vec<Checkout>),
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("git failed in {path}: {stderr}")]
    Failed { path: PathBuf, stderr: String },
}

/// Runs the git commands used to list checkouts.
pub trait GitRunner {
    fn worktree_list(&self, clone: &Path) -> Result<String, GitError>;
}

/// Runs the `git` binary with a fixed environment.
pub struct SystemGit {
    pub env: Env,
}

impl GitRunner for SystemGit {
    fn worktree_list(&self, clone: &Path) -> Result<String, GitError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(clone)
            .args(["worktree", "list", "--porcelain"])
            .env_clear()
            .envs(&self.env)
            .output()?;

        if !output.status.success() {
            return Err(GitError::Failed {
                path: clone.to_path_buf(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Parses the output of `git worktree list --porcelain`.
pub fn parse_worktree_porcelain(output: &str) -> Vec<Checkout> {
    output.split("\n\n").filter_map(parse_block).collect()
}

/// Lists the checkouts of each clone in `repo_dir`, merged and sorted by directory name.
pub fn list(repo_dir: &Path, git: &dyn GitRunner) -> Result<RepoCheckouts, GitError> {
    if !repo_dir.is_dir() {
        return Ok(RepoCheckouts::NotCloned);
    }

    let mut seen = HashSet::new();
    let mut checkouts = Vec::new();

    for clone in primary_clones(repo_dir)? {
        for checkout in parse_worktree_porcelain(&git.worktree_list(&clone)?) {
            if seen.insert(checkout.path.clone()) {
                checkouts.push(checkout);
            }
        }
    }

    checkouts.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
    Ok(RepoCheckouts::Checkouts(checkouts))
}

fn primary_clones(repo_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut clones = Vec::new();
    for entry in fs::read_dir(repo_dir)? {
        let path = entry?.path();
        if path.join(".git").is_dir() {
            clones.push(path);
        }
    }

    clones.sort();
    Ok(clones)
}

fn parse_block(block: &str) -> Option<Checkout> {
    let mut path = None;
    let mut branch = None;

    for line in block.lines() {
        if line == "bare" {
            return None;
        }
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(p));
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        }
    }

    let path = path?;
    let dir_name = path.file_name()?.to_string_lossy().into_owned();
    Some(Checkout {
        path,
        dir_name,
        branch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fs;

    const PORCELAIN: &str = "\
worktree /src/akka/sdk/main
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /src/akka/sdk/feat-x
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feat/x

worktree /src/akka/sdk/detached
HEAD 3333333333333333333333333333333333333333
detached
";

    struct FakeGit {
        outputs: HashMap<PathBuf, String>,
        calls: RefCell<Vec<PathBuf>>,
    }

    impl GitRunner for FakeGit {
        fn worktree_list(&self, clone: &Path) -> Result<String, GitError> {
            self.calls.borrow_mut().push(clone.to_path_buf());
            Ok(self.outputs.get(clone).cloned().unwrap_or_default())
        }
    }

    fn entry(path: &Path, branch: &str) -> String {
        format!(
            "worktree {}\nHEAD 0\nbranch refs/heads/{branch}\n\n",
            path.display()
        )
    }

    #[test]
    fn parses_porcelain() {
        let checkouts = parse_worktree_porcelain(PORCELAIN);

        let branches: Vec<_> = checkouts.iter().map(|c| c.branch.as_deref()).collect();
        let names: Vec<_> = checkouts.iter().map(|c| c.dir_name.as_str()).collect();
        assert_eq!(branches, vec![Some("main"), Some("feat/x"), None]);
        assert_eq!(names, vec!["main", "feat-x", "detached"]);
        assert_eq!(checkouts[0].path, PathBuf::from("/src/akka/sdk/main"));
    }

    #[test]
    fn skips_bare_entries() {
        let output = "worktree /src/a/b/bare.git\nbare\n\nworktree /src/a/b/main\nHEAD 0\nbranch refs/heads/main\n";

        let checkouts = parse_worktree_porcelain(output);

        assert_eq!(checkouts.len(), 1);
        assert_eq!(checkouts[0].dir_name, "main");
    }

    #[test]
    fn missing_repo_dir_is_not_cloned() {
        let dir = tempfile::tempdir().unwrap();
        let git = FakeGit {
            outputs: HashMap::new(),
            calls: RefCell::new(Vec::new()),
        };

        let result = list(&dir.path().join("missing"), &git).unwrap();

        assert_eq!(result, RepoCheckouts::NotCloned);
    }

    #[test]
    fn merges_clones_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let main = repo.join("main");
        let other = repo.join("other");
        let shared = repo.join("shared-wt");
        fs::create_dir_all(main.join(".git")).unwrap();
        fs::create_dir_all(other.join(".git")).unwrap();

        let git = FakeGit {
            outputs: HashMap::from([
                (main.clone(), entry(&main, "main") + &entry(&shared, "wt")),
                (
                    other.clone(),
                    entry(&other, "other") + &entry(&shared, "wt"),
                ),
            ]),
            calls: RefCell::new(Vec::new()),
        };

        let RepoCheckouts::Checkouts(checkouts) = list(repo, &git).unwrap() else {
            panic!("expected checkouts");
        };

        let names: Vec<_> = checkouts.iter().map(|c| c.dir_name.as_str()).collect();
        assert_eq!(names, vec!["main", "other", "shared-wt"]);
    }

    #[test]
    fn system_git_lists_a_real_clone() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("sdk");
        let main = repo.join("main");
        fs::create_dir_all(&main).unwrap();
        let init = Command::new("git")
            .args(["init", "-q", "-b", "trunk"])
            .arg(&main)
            .status()
            .unwrap();
        assert!(init.success());
        let git = SystemGit {
            env: std::env::vars().collect(),
        };

        let RepoCheckouts::Checkouts(checkouts) = list(&repo, &git).unwrap() else {
            panic!("expected checkouts");
        };

        assert_eq!(checkouts.len(), 1);
        assert_eq!(checkouts[0].dir_name, "main");
        assert_eq!(checkouts[0].branch.as_deref(), Some("trunk"));
    }

    #[test]
    fn system_git_finds_git_through_its_env_path() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let repo = dir.path().join("sdk");
        let main = repo.join("main");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(main.join(".git")).unwrap();
        let fake_git = bin.join("git");
        fs::write(
            &fake_git,
            format!(
                "#!/bin/sh\nprintf 'worktree {}\\nHEAD 0\\nbranch refs/heads/from-fake\\n'\n",
                main.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();
        let git = SystemGit {
            env: Env::from([("PATH".to_string(), bin.display().to_string())]),
        };

        let RepoCheckouts::Checkouts(checkouts) = list(&repo, &git).unwrap() else {
            panic!("expected checkouts");
        };

        assert_eq!(checkouts[0].branch.as_deref(), Some("from-fake"));
    }

    #[test]
    fn ignores_children_without_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        fs::create_dir_all(repo.join("main/.git")).unwrap();
        fs::create_dir_all(repo.join("notes")).unwrap();
        let git = FakeGit {
            outputs: HashMap::new(),
            calls: RefCell::new(Vec::new()),
        };

        list(repo, &git).unwrap();

        assert_eq!(*git.calls.borrow(), vec![repo.join("main")]);
    }
}
