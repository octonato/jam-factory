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

//! Copies the session of the current worktree into a folder of the worktree.

use std::path::{Path, PathBuf};
use std::process::Command;

use jam_factory_store::{Category, SessionFilter, SessionRef, Store, root};

const DEFAULT_DEST: &str = "factory";
const DETACHED: &str = "HEAD";
const HELP: [&str; 2] = ["-h", "--help"];
const USAGE: &str = "Usage: jam-factory-mcp export [dest]";

/// Copies the live `session` run of the branch that holds `cwd`.
/// The target is `args[0]`, or `factory/{session id}` when absent.
/// Returns the line to print.
/// Returns the usage for `-h` and `--help`.
pub fn run(args: &[String], home: &Path, cwd: &Path) -> Result<String, String> {
    if args.len() > 1 {
        return Err(USAGE.to_string());
    }

    if let Some(arg) = args.first() {
        if HELP.contains(&arg.as_str()) {
            return Ok(USAGE.to_string());
        }

        if arg.starts_with('-') {
            return Err(format!("Unknown option: {arg}\n{USAGE}"));
        }
    }

    let worktree = worktree(cwd)?;
    let (org, repo) = org_and_repo(&worktree)?;
    let branch = branch(&worktree)?;
    let branch = branch.rsplit('/').next().unwrap_or(&branch);

    let root = root::resolve(home).map_err(|e| e.to_string())?;
    let store = Store::new(root);
    let session = live_session(&store, org, repo, branch)?;

    let dest = match args.first() {
        Some(dest) => cwd.join(dest),
        None => worktree.join(DEFAULT_DEST).join(session.session()),
    };

    let count = store
        .export_session(&session, &dest)
        .map_err(|e| e.to_string())?;

    let noun = if count == 1 { "file" } else { "files" };

    Ok(format!("Copied {count} {noun} to {}", dest.display()))
}

/// Returns the live `session` run of `branch`, or an error when it has none.
fn live_session(store: &Store, org: &str, repo: &str, branch: &str) -> Result<SessionRef, String> {
    let filter = SessionFilter {
        org: Some(org.to_string()),
        repo: Some(repo.to_string()),
        branch: Some(branch.to_string()),
        category: Some(Category::Session),
        include_archived: false,
    };

    let sessions = store.list_sessions(&filter).map_err(|e| e.to_string())?;

    let Some(entry) = sessions.first() else {
        return Err(format!(
            "Branch {branch} has no live session in {org}/{repo}"
        ));
    };

    SessionRef::new(&entry.org, &entry.repo, &entry.branch, &entry.session)
        .map_err(|e| e.to_string())
}

/// Returns the root folder of the worktree holding `cwd`.
fn worktree(cwd: &Path) -> Result<PathBuf, String> {
    let root = git(
        cwd,
        &["rev-parse", "--show-toplevel"],
        "Cannot find the worktree",
    )?;

    Ok(PathBuf::from(root))
}

/// Returns the names of the folder above the parent of `worktree` and of the parent.
fn org_and_repo(worktree: &Path) -> Result<(&str, &str), String> {
    let parent = worktree.parent();
    let repo = parent.and_then(Path::file_name).and_then(|n| n.to_str());
    let org = parent
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|n| n.to_str());

    match (org, repo) {
        (Some(org), Some(repo)) => Ok((org, repo)),
        _ => Err(format!(
            "Cannot find the org and repo of {}",
            worktree.display()
        )),
    }
}

/// Returns the branch checked out in `worktree`, or an error on a detached HEAD.
fn branch(worktree: &Path) -> Result<String, String> {
    let branch = git(
        worktree,
        &["rev-parse", "--abbrev-ref", "HEAD"],
        "Cannot read the branch",
    )?;

    if branch == DETACHED {
        return Err("HEAD is detached. Check out the session's branch.".to_string());
    }

    Ok(branch)
}

/// Runs git with `args` in `dir` and returns its trimmed stdout, or `failure` with git's message.
fn git(dir: &Path, args: &[&str], failure: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("Cannot run git: {e}"))?;

    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{failure}: {}", reason.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
