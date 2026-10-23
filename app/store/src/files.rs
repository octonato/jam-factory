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

//! Versioned reads and locked, atomic writes of session files.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::error::StoreError;
use crate::name::{SessionRef, check_path};

pub const SESSIONS_DIR: &str = "sessions";
pub const LOCK_FILE: &str = ".lock";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The factory folder that holds the sessions of every org.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

/// The content of a file and its version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRead {
    pub content: String,
    pub version: String,
}

/// An exclusive lock on a session folder, released on drop.
pub(crate) struct SessionLock {
    _file: File,
}

impl Store {
    /// Returns the store for the factory root `root`.
    pub fn new(root: PathBuf) -> Store {
        Store { root }
    }

    /// Lists the files of the session `s`, recursively, as sorted relative paths without dot files.
    pub fn list_files(&self, s: &SessionRef) -> Result<Vec<String>, StoreError> {
        let dir = self.dir(s);

        if !dir.is_dir() {
            return Err(session_not_found(s));
        }

        let mut files = Vec::new();
        walk(&dir, "", &mut files)?;
        files.sort();

        Ok(files)
    }

    /// Returns the content of `path` in the session `s` and its version.
    pub fn read_file(&self, s: &SessionRef, path: &str) -> Result<FileRead, StoreError> {
        check_path(path)?;

        let Some(bytes) = read(&self.dir(s).join(path))? else {
            return Err(file_not_found(s, path));
        };

        Ok(FileRead {
            content: text(path, &bytes)?.to_string(),
            version: version(&bytes),
        })
    }

    /// Writes `content` to a new `path`, or over the one whose version is `expected_version`.
    pub fn write_file(
        &self,
        s: &SessionRef,
        path: &str,
        content: &str,
        expected_version: Option<&str>,
    ) -> Result<String, StoreError> {
        check_path(path)?;
        let _lock = self.lock_or_create(s)?;

        rewrite(&self.dir(s), path, |current| {
            match (current, expected_version) {
                (None, None) => Ok(content.to_string()),
                (None, Some(_)) => Err(file_not_found(s, path)),
                (Some(_), None) => Err(file_exists(s, path)),

                (Some(bytes), Some(expected)) => {
                    check_version(bytes, expected)?;
                    Ok(content.to_string())
                }
            }
        })
    }

    /// Replaces the one occurrence of `old_str` in `path` with `new_str`.
    pub fn edit_file(
        &self,
        s: &SessionRef,
        path: &str,
        old_str: &str,
        new_str: &str,
    ) -> Result<String, StoreError> {
        check_path(path)?;
        let _lock = self.lock(s)?;

        rewrite(&self.dir(s), path, |current| {
            let Some(bytes) = current else {
                return Err(file_not_found(s, path));
            };

            replace_once(text(path, bytes)?, old_str, new_str)
        })
    }

    /// Appends `content` to `path`, and creates the file when missing.
    pub fn append_file(
        &self,
        s: &SessionRef,
        path: &str,
        content: &str,
    ) -> Result<String, StoreError> {
        check_path(path)?;
        let _lock = self.lock_or_create(s)?;

        rewrite(&self.dir(s), path, |current| match current {
            None => Ok(content.to_string()),
            Some(bytes) => Ok(format!("{}{content}", text(path, bytes)?)),
        })
    }

    /// Renames `from` to `to` inside the session `s`, when `to` does not exist.
    pub fn move_file(&self, s: &SessionRef, from: &str, to: &str) -> Result<String, StoreError> {
        check_path(from)?;
        check_path(to)?;
        let _lock = self.lock(s)?;

        let dir = self.dir(s);
        let source = dir.join(from);
        let target = dir.join(to);

        let Some(bytes) = read(&source)? else {
            return Err(file_not_found(s, from));
        };

        if fs::exists(&target).map_err(|e| io_error(&target, e))? {
            return Err(file_exists(s, to));
        }

        create_parent(&target)?;
        fs::rename(&source, &target).map_err(|e| io_error(&source, e))?;

        Ok(version(&bytes))
    }

    /// Deletes `path` when its version is `expected_version`.
    pub fn delete_file(
        &self,
        s: &SessionRef,
        path: &str,
        expected_version: &str,
    ) -> Result<(), StoreError> {
        check_path(path)?;
        let _lock = self.lock(s)?;

        let target = self.dir(s).join(path);

        let Some(bytes) = read(&target)? else {
            return Err(file_not_found(s, path));
        };

        check_version(&bytes, expected_version)?;

        fs::remove_file(&target).map_err(|e| io_error(&target, e))
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// The folder of the session `s`.
    pub(crate) fn dir(&self, s: &SessionRef) -> PathBuf {
        self.root
            .join(s.org())
            .join(s.repo())
            .join(SESSIONS_DIR)
            .join(s.session())
    }

    /// Takes the lock of the session `s`, or returns `not_found` when its folder is missing.
    pub(crate) fn lock(&self, s: &SessionRef) -> Result<SessionLock, StoreError> {
        let path = self.dir(s).join(LOCK_FILE);

        let opened = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path);

        let file = match opened {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(session_not_found(s)),
            Err(e) => return Err(io_error(&path, e)),
        };

        file.lock().map_err(|e| io_error(&path, e))?;

        Ok(SessionLock { _file: file })
    }

    /// Creates the folder of the session `s` when missing, and takes its lock.
    pub(crate) fn lock_or_create(&self, s: &SessionRef) -> Result<SessionLock, StoreError> {
        let dir = self.dir(s);
        fs::create_dir_all(&dir).map_err(|e| io_error(&dir, e))?;

        self.lock(s)
    }
}

/// Applies `change` to the current bytes of `path` in `dir`. Writes the result and returns the new version.
pub(crate) fn rewrite<F>(dir: &Path, path: &str, change: F) -> Result<String, StoreError>
where
    F: FnOnce(Option<&[u8]>) -> Result<String, StoreError>,
{
    let target = dir.join(path);

    let current = read(&target)?;
    let content = change(current.as_deref())?;

    write_atomic(&target, content.as_bytes())?;

    Ok(version(content.as_bytes()))
}

/// The lowercase hex SHA-256 of `bytes`.
pub(crate) fn version(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Returns `conflict` with the current version unless the version of `bytes` is `expected`.
fn check_version(bytes: &[u8], expected: &str) -> Result<(), StoreError> {
    let current = version(bytes);

    if current == expected {
        Ok(())
    } else {
        Err(StoreError::Conflict { current })
    }
}

/// Returns `text` with its one occurrence of `old` replaced by `new`.
fn replace_once(text: &str, old: &str, new: &str) -> Result<String, StoreError> {
    if old.is_empty() {
        return Err(StoreError::ManyMatches);
    }

    let Some(at) = text.find(old) else {
        return Err(StoreError::NoMatch);
    };

    let next = at + text[at..].chars().next().map_or(1, char::len_utf8);

    if text[next..].contains(old) {
        return Err(StoreError::ManyMatches);
    }

    Ok(format!("{}{new}{}", &text[..at], &text[at + old.len()..]))
}

/// Returns the bytes of `target`, or `None` when it does not exist.
pub(crate) fn read(target: &Path) -> Result<Option<Vec<u8>>, StoreError> {
    match fs::read(target) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(target, e)),
    }
}

/// Returns `bytes` as UTF-8 text, or `io` when they are not.
fn text<'a>(path: &str, bytes: &'a [u8]) -> Result<&'a str, StoreError> {
    std::str::from_utf8(bytes).map_err(|_| StoreError::Io(format!("{path} is not UTF-8 text")))
}

/// Writes `bytes` to a temp file next to `target`, then renames it over `target`.
fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let folder = create_parent(target)?;

    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = folder.join(format!(".{name}.{}.{count}.tmp", process::id()));

    let written = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, target));

    if let Err(e) = written {
        let _ = fs::remove_file(&temp);
        return Err(io_error(target, e));
    }

    Ok(())
}

/// Creates the folder of `target` when missing, and returns it.
fn create_parent(target: &Path) -> Result<&Path, StoreError> {
    let folder = target.parent().expect("a session file has a folder");
    fs::create_dir_all(folder).map_err(|e| io_error(folder, e))?;

    Ok(folder)
}

/// Adds the files under `dir` to `files` as paths after `prefix`, skipping names that start with `.`.
fn walk(dir: &Path, prefix: &str, files: &mut Vec<String>) -> Result<(), StoreError> {
    let entries = fs::read_dir(dir).map_err(|e| io_error(dir, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| io_error(dir, e))?;

        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };

        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| io_error(&path, e))?;

        if file_type.is_dir() {
            walk(&path, &format!("{prefix}{name}/"), files)?;
        } else if path.is_file() {
            files.push(format!("{prefix}{name}"));
        }
    }

    Ok(())
}

fn session_not_found(s: &SessionRef) -> StoreError {
    StoreError::NotFound(format!("Session {s} does not exist"))
}

pub(crate) fn file_not_found(s: &SessionRef, path: &str) -> StoreError {
    StoreError::NotFound(format!("{path} does not exist in session {s}"))
}

fn file_exists(s: &SessionRef, path: &str) -> StoreError {
    StoreError::Exists(format!("{path} already exists in session {s}"))
}

pub(crate) fn io_error(path: &Path, e: io::Error) -> StoreError {
    StoreError::Io(format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::thread;

    use tempfile::TempDir;

    use super::*;

    const HELLO: &str = "hello\n";
    const HELLO_VERSION: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";

    fn store() -> (TempDir, Store) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path().to_path_buf());
        (root, store)
    }

    fn session() -> SessionRef {
        SessionRef::new("octonato", "jam-factory", "s1").unwrap()
    }

    fn code<T: Debug>(result: Result<T, StoreError>) -> &'static str {
        result.unwrap_err().code()
    }

    #[test]
    fn bad_names_are_refused() {
        for name in ["", ".", "..", "a/b", "a\\b", "a\0"] {
            let refs = [
                SessionRef::new(name, "r", "s"),
                SessionRef::new("o", name, "s"),
                SessionRef::new("o", "r", name),
            ];

            for result in refs {
                assert_eq!(code(result), "bad_name", "{name:?}");
            }
        }

        assert!(SessionRef::new("octonato", "a b%c", "café").is_ok());
    }

    #[test]
    fn bad_paths_are_refused() {
        let (_root, store) = store();
        let s = session();
        store.write_file(&s, "ok.md", HELLO, None).unwrap();

        let paths = [
            "/a", "a//b", "../a", "a/./b", ".lock", "r/.x.tmp", "", "a/", "a\\b", "a\0",
        ];

        for path in paths {
            assert_eq!(code(store.read_file(&s, path)), "bad_name", "{path:?}");
            assert_eq!(
                code(store.write_file(&s, path, HELLO, None)),
                "bad_name",
                "{path:?}"
            );
            assert_eq!(
                code(store.move_file(&s, "ok.md", path)),
                "bad_name",
                "{path:?}"
            );
        }
    }

    #[test]
    fn write_creates_and_read_returns_the_version() {
        let (_root, store) = store();
        let s = session();

        let written = store
            .write_file(&s, "round-1/notes.md", HELLO, None)
            .unwrap();
        let read = store.read_file(&s, "round-1/notes.md").unwrap();

        assert_eq!(written, HELLO_VERSION);
        assert_eq!(
            read,
            FileRead {
                content: HELLO.to_string(),
                version: HELLO_VERSION.to_string(),
            }
        );
    }

    #[test]
    fn write_without_version_refuses_an_existing_file() {
        let (_root, store) = store();
        let s = session();
        store.write_file(&s, "spec.md", "one", None).unwrap();

        let result = store.write_file(&s, "spec.md", "two", None);

        assert_eq!(code(result), "exists");
        assert_eq!(store.read_file(&s, "spec.md").unwrap().content, "one");
    }

    #[test]
    fn write_with_a_stale_version_conflicts() {
        let (_root, store) = store();
        let s = session();
        let first = store
            .write_file(&s, "session.json", "{\"mode\": \"a\"}\n", None)
            .unwrap();

        let outside = "{\"mode\": \"b\"}\n";
        fs::write(store.dir(&s).join("session.json"), outside).unwrap();

        let result = store.write_file(&s, "session.json", "{\"mode\": \"c\"}\n", Some(&first));

        assert_eq!(
            result.unwrap_err(),
            StoreError::Conflict {
                current: version(outside.as_bytes()),
            }
        );
        assert_eq!(
            store.read_file(&s, "session.json").unwrap().content,
            outside
        );
    }

    #[test]
    fn edit_replaces_one_match() {
        let (_root, store) = store();
        let s = session();
        store
            .write_file(&s, "plan.md", "one two three", None)
            .unwrap();

        let edited = store.edit_file(&s, "plan.md", "two", "2").unwrap();
        let read = store.read_file(&s, "plan.md").unwrap();

        assert_eq!(read.content, "one 2 three");
        assert_eq!(read.version, edited);
    }

    #[test]
    fn edit_without_match_fails() {
        let (_root, store) = store();
        let s = session();
        store
            .write_file(&s, "plan.md", "one two three", None)
            .unwrap();

        let result = store.edit_file(&s, "plan.md", "four", "4");

        assert_eq!(code(result), "no_match");
        assert_eq!(
            store.read_file(&s, "plan.md").unwrap().content,
            "one two three"
        );
    }

    #[test]
    fn edit_with_two_matches_fails() {
        let (_root, store) = store();
        let s = session();
        store.write_file(&s, "plan.md", "a b a", None).unwrap();
        store.write_file(&s, "spec.md", "aaa", None).unwrap();

        assert_eq!(
            code(store.edit_file(&s, "plan.md", "a", "x")),
            "many_matches"
        );
        assert_eq!(
            code(store.edit_file(&s, "spec.md", "aa", "x")),
            "many_matches"
        );
        assert_eq!(store.read_file(&s, "plan.md").unwrap().content, "a b a");
    }

    #[test]
    fn append_creates_then_appends() {
        let (_root, store) = store();
        let s = session();

        store.append_file(&s, "runs.jsonl", "{\"n\":1}\n").unwrap();
        let appended = store.append_file(&s, "runs.jsonl", "{\"n\":2}\n").unwrap();
        let read = store.read_file(&s, "runs.jsonl").unwrap();

        assert_eq!(read.content, "{\"n\":1}\n{\"n\":2}\n");
        assert_eq!(read.version, appended);
    }

    #[test]
    fn move_refuses_an_existing_target() {
        let (_root, store) = store();
        let s = session();
        store.write_file(&s, "a.md", "a", None).unwrap();
        store.write_file(&s, "b.md", "b", None).unwrap();

        let result = store.move_file(&s, "a.md", "b.md");

        assert_eq!(code(result), "exists");
        assert_eq!(store.read_file(&s, "a.md").unwrap().content, "a");
        assert_eq!(store.read_file(&s, "b.md").unwrap().content, "b");
    }

    #[test]
    fn move_renames() {
        let (_root, store) = store();
        let s = session();
        store
            .write_file(&s, "staging/feedback.md", HELLO, None)
            .unwrap();

        let moved = store
            .move_file(&s, "staging/feedback.md", "round-2/feedback.md")
            .unwrap();

        assert_eq!(moved, HELLO_VERSION);
        assert_eq!(
            store.read_file(&s, "round-2/feedback.md").unwrap().content,
            HELLO
        );
        assert_eq!(
            code(store.read_file(&s, "staging/feedback.md")),
            "not_found"
        );
    }

    #[test]
    fn delete_needs_the_current_version() {
        let (_root, store) = store();
        let s = session();
        let written = store.write_file(&s, "a.md", HELLO, None).unwrap();

        let stale = store.delete_file(&s, "a.md", "stale");

        assert_eq!(
            stale.unwrap_err(),
            StoreError::Conflict {
                current: written.clone(),
            }
        );

        store.delete_file(&s, "a.md", &written).unwrap();

        assert_eq!(code(store.read_file(&s, "a.md")), "not_found");
    }

    #[test]
    fn list_files_is_recursive_and_hides_dot_files() {
        let (_root, store) = store();
        let s = session();
        store.write_file(&s, "spec.md", HELLO, None).unwrap();
        store.write_file(&s, "plan.md", HELLO, None).unwrap();
        store
            .write_file(&s, "round-1/reviews/independent-review.md", HELLO, None)
            .unwrap();

        let dir = store.dir(&s);
        fs::write(dir.join(".a.1.2.tmp"), "partial").unwrap();
        fs::write(dir.join("round-1").join(".b.md.3.4.tmp"), "partial").unwrap();
        assert!(dir.join(".lock").is_file());

        let files = store.list_files(&s).unwrap();

        assert_eq!(
            files,
            [
                "plan.md",
                "round-1/reviews/independent-review.md",
                "spec.md"
            ]
        );
    }

    #[test]
    fn missing_session_is_not_found() {
        let (_root, store) = store();
        let s = session();

        assert_eq!(code(store.read_file(&s, "spec.md")), "not_found");
        assert_eq!(code(store.list_files(&s)), "not_found");
        assert_eq!(code(store.edit_file(&s, "spec.md", "a", "b")), "not_found");
        assert_eq!(code(store.delete_file(&s, "spec.md", "v")), "not_found");
        assert!(!store.dir(&s).exists());
    }

    #[test]
    fn concurrent_appends_lose_nothing() {
        let (_root, store) = store();
        let s = session();

        thread::scope(|scope| {
            for worker in 0..8 {
                let (store, s) = (&store, &s);

                scope.spawn(move || {
                    for call in 0..50 {
                        let line = format!("{worker}-{call}\n");
                        store.append_file(s, "runs.jsonl", &line).unwrap();
                    }
                });
            }
        });

        let content = store.read_file(&s, "runs.jsonl").unwrap().content;
        let lines: HashSet<&str> = content.lines().collect();

        assert_eq!(content.lines().count(), 400);
        assert_eq!(lines.len(), 400);
    }

    /// The line of `worker` for its call `call`, marked `state`.
    fn task_line(worker: usize, call: usize, state: &str) -> String {
        format!("task {worker}-{call}: {state}\n")
    }

    #[test]
    fn concurrent_edits_lose_nothing() {
        let (_root, store) = store();
        let s = session();

        let mut todo = String::new();
        let mut done = String::new();

        for worker in 0..8 {
            for call in 0..25 {
                todo.push_str(&task_line(worker, call, "todo"));
                done.push_str(&task_line(worker, call, "done"));
            }
        }

        store.write_file(&s, "tasks.md", &todo, None).unwrap();

        thread::scope(|scope| {
            for worker in 0..8 {
                let (store, s) = (&store, &s);

                scope.spawn(move || {
                    for call in 0..25 {
                        let old = task_line(worker, call, "todo");
                        let new = task_line(worker, call, "done");

                        store.edit_file(s, "tasks.md", &old, &new).unwrap();
                    }
                });
            }
        });

        assert_eq!(store.read_file(&s, "tasks.md").unwrap().content, done);
    }
}
