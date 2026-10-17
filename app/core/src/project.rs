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

//! A project: a folder whose `.odeck-repos` file lists `org/repo` entries.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

pub const PROJECT_FILE: &str = ".odeck-repos";

/// One `org/repo` entry of a project file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoRef {
    pub org: String,
    pub name: String,
}

impl RepoRef {
    /// The repo directory under `github_src`.
    pub fn path(&self, github_src: &Path) -> PathBuf {
        github_src.join(&self.org).join(&self.name)
    }
}

/// A project file line that could not be read as a repo entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineError {
    pub line: usize,
    pub text: String,
    pub reason: String,
}

/// The parsed content of a project file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectFile {
    pub repos: Vec<RepoRef>,
    pub errors: Vec<LineError>,
}

/// A loaded project.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub file: ProjectFile,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("no {PROJECT_FILE} file in {0}")]
    NoRepoFile(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Parses the content of a project file.
pub fn parse(content: &str) -> ProjectFile {
    let mut file = ProjectFile::default();
    let mut seen: HashMap<RepoRef, usize> = HashMap::new();

    for (index, raw) in content.lines().enumerate() {
        let line = index + 1;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }

        let error = |reason: String| LineError {
            line,
            text: text.to_string(),
            reason,
        };

        match parse_entry(text) {
            None => file.errors.push(error("expected org/repo".to_string())),
            Some(repo) => match seen.get(&repo) {
                Some(first) => file
                    .errors
                    .push(error(format!("duplicate of line {first}"))),
                None => {
                    seen.insert(repo.clone(), line);
                    file.repos.push(repo);
                }
            },
        }
    }

    file
}

/// Reads the project file in `root`.
pub fn load(root: &Path) -> Result<Project, ProjectError> {
    let path = root.join(PROJECT_FILE);
    if !path.is_file() {
        return Err(ProjectError::NoRepoFile(root.to_path_buf()));
    }

    let content = fs::read_to_string(&path)?;
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    Ok(Project {
        root: root.to_path_buf(),
        name,
        file: parse(&content),
    })
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(at) => &line[..at],
        None => line,
    }
}

fn parse_entry(text: &str) -> Option<RepoRef> {
    let (org, name) = text.split_once('/')?;
    if !is_segment(org) || !is_segment(name) {
        return None;
    }

    Some(RepoRef {
        org: org.to_string(),
        name: name.to_string(),
    })
}

fn is_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo(org: &str, name: &str) -> RepoRef {
        RepoRef {
            org: org.to_string(),
            name: name.to_string(),
        }
    }

    fn error_lines(file: &ProjectFile) -> Vec<usize> {
        file.errors.iter().map(|e| e.line).collect()
    }

    #[test]
    fn parses_org_repo_lines() {
        let file = parse("akka/runtime\nlightbend/console\n");

        assert_eq!(
            file.repos,
            vec![repo("akka", "runtime"), repo("lightbend", "console")]
        );
        assert!(file.errors.is_empty());
    }

    #[test]
    fn skips_blank_lines_and_comments() {
        let file = parse("# Akka\n\n  akka/sdk  # main sdk\n");

        assert_eq!(file.repos, vec![repo("akka", "sdk")]);
        assert!(file.errors.is_empty());
    }

    #[test]
    fn handles_crlf() {
        let file = parse("akka/runtime\r\nakka/sdk\r\n");

        assert_eq!(
            file.repos,
            vec![repo("akka", "runtime"), repo("akka", "sdk")]
        );
        assert!(file.errors.is_empty());
    }

    #[test]
    fn reports_bad_lines_with_line_number() {
        let file = parse("akka/runtime\nnoslash\na/b/c\n/b\n");

        assert_eq!(file.repos, vec![repo("akka", "runtime")]);
        assert_eq!(error_lines(&file), vec![2, 3, 4]);
        assert_eq!(file.errors[0].text, "noslash");
    }

    #[test]
    fn reports_duplicates() {
        let file = parse("akka/sdk\nakka/sdk\n");

        assert_eq!(file.repos, vec![repo("akka", "sdk")]);
        assert_eq!(error_lines(&file), vec![2]);
    }

    #[test]
    fn repo_path_joins_github_src() {
        let path = repo("akka", "sdk").path(Path::new("/src"));

        assert_eq!(path, PathBuf::from("/src/akka/sdk"));
    }

    #[test]
    fn load_fails_without_repo_file() {
        let dir = tempfile::tempdir().unwrap();

        let result = load(dir.path());

        assert!(matches!(result, Err(ProjectError::NoRepoFile(_))));
    }

    #[test]
    fn load_names_project_after_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("akka");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(PROJECT_FILE), "akka/sdk\n").unwrap();

        let project = load(&root).unwrap();

        assert_eq!(project.name, "akka");
        assert_eq!(project.root, root);
        assert_eq!(project.file.repos, vec![repo("akka", "sdk")]);
    }
}
