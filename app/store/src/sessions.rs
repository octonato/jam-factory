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

//! Lists, starts and updates sessions through their `session.json`.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::StoreError;
use crate::files::{self, LOCK_FILE, SESSIONS_DIR, Store};
use crate::name::{Category, SessionRef, check_name};

pub const SESSION_FILE: &str = "session.json";

const ARCHIVED: &str = "archived";
const CATEGORY: &str = "category";
const COUNTS: &str = "counts";
const ID: &str = "id";
const UPDATED: &str = "updated";
const UPDATED_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";
pub(crate) const STARTED_FORMAT: &str = "%Y%m%d-%H%M";

/// Filters sessions by org, repo, branch, category and archived state.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub org: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub category: Option<Category>,
    pub include_archived: bool,
}

/// A session with its parsed `session.json`, or the reason it does not parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub org: String,
    pub repo: String,
    pub branch: String,
    pub session: String,
    pub category: Category,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The session `start_session` created and the version of its `session.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub session: SessionRef,
    pub version: String,
}

impl Store {
    /// Lists the sessions that match `filter`, sorted by org, repo, branch and id.
    pub fn list_sessions(&self, filter: &SessionFilter) -> Result<Vec<SessionEntry>, StoreError> {
        for name in [&filter.org, &filter.repo, &filter.branch]
            .into_iter()
            .flatten()
        {
            check_name(name)?;
        }

        let mut entries = Vec::new();

        for org in folders(self.root(), filter.org.as_deref())? {
            let org_dir = self.root().join(&org);

            for repo in folders(&org_dir, filter.repo.as_deref())? {
                let sessions_dir = org_dir.join(&repo).join(SESSIONS_DIR);

                for branch in folders(&sessions_dir, filter.branch.as_deref())? {
                    let branch_dir = sessions_dir.join(&branch);

                    for session in folders(&branch_dir, None)? {
                        let Some(category) = Category::of(&session) else {
                            continue;
                        };

                        if filter.category.is_some_and(|wanted| wanted != category) {
                            continue;
                        }

                        let entry = SessionEntry {
                            org: org.clone(),
                            repo: repo.clone(),
                            branch: branch.clone(),
                            session: session.clone(),
                            category,
                            data: None,
                            error: None,
                        };

                        let file = branch_dir.join(&session).join(SESSION_FILE);
                        entries.extend(load(entry, &file, filter.include_archived));
                    }
                }
            }
        }

        Ok(entries)
    }

    /// Creates a session of `category` on `branch`.
    /// The name comes from the current UTC minute and `head`.
    /// `fields` go in its `session.json`.
    ///
    /// Returns `exists` when the branch already holds a live `session` or `review` run.
    pub fn start_session(
        &self,
        org: &str,
        repo: &str,
        branch: &str,
        category: Category,
        head: &str,
        fields: &Map<String, Value>,
    ) -> Result<Started, StoreError> {
        self.start_session_at(org, repo, branch, category, head, fields, Utc::now())
    }

    #[allow(clippy::too_many_arguments)]
    fn start_session_at(
        &self,
        org: &str,
        repo: &str,
        branch: &str,
        category: Category,
        head: &str,
        fields: &Map<String, Value>,
        now: DateTime<Utc>,
    ) -> Result<Started, StoreError> {
        for name in [org, repo, branch] {
            check_name(name)?;
        }

        check_head(head)?;

        let branch_dir = self.branch_dir(org, repo, branch);
        fs::create_dir_all(&branch_dir).map_err(|e| files::io_error(&branch_dir, e))?;

        let _lock = files::lock_file(&branch_dir.join(LOCK_FILE))?;

        if category != Category::Walk {
            self.refuse_live(org, repo, branch, category)?;
        }

        let base = format!("{category}-{}-{head}", now.format(STARTED_FORMAT));
        let session = free_name(&branch_dir, &base);
        let s = SessionRef::new(org, repo, branch, &session)?;

        let mut data = fields.clone();
        data.insert(ID.to_string(), Value::String(session.clone()));
        data.insert(CATEGORY.to_string(), Value::String(category.to_string()));
        data.insert(ARCHIVED.to_string(), Value::Bool(false));
        data.insert(
            UPDATED.to_string(),
            Value::String(now.format(UPDATED_FORMAT).to_string()),
        );

        let text =
            serde_json::to_string_pretty(&data).map_err(|e| StoreError::BadJson(e.to_string()))?;

        let dir = self.dir(&s);
        fs::create_dir(&dir).map_err(|e| files::io_error(&dir, e))?;
        let version = self.write_file(&s, SESSION_FILE, &format!("{text}\n"), None)?;

        Ok(Started {
            session: s,
            version,
        })
    }

    /// Returns `exists` when `branch` holds a session of `category` that is not archived.
    fn refuse_live(
        &self,
        org: &str,
        repo: &str,
        branch: &str,
        category: Category,
    ) -> Result<(), StoreError> {
        let filter = SessionFilter {
            org: Some(org.to_string()),
            repo: Some(repo.to_string()),
            branch: Some(branch.to_string()),
            category: Some(category),
            include_archived: false,
        };

        match self.list_sessions(&filter)?.first() {
            Some(live) => Err(StoreError::Exists(format!(
                "Branch {branch} already holds the live {category} {}. Archive it first.",
                live.session
            ))),
            None => Ok(()),
        }
    }

    /// Merges `fields` into the `session.json` of `s`, and sets `updated`.
    pub fn update_session(
        &self,
        s: &SessionRef,
        fields: &Map<String, Value>,
    ) -> Result<String, StoreError> {
        let _lock = self.lock(s)?;

        files::rewrite(&self.dir(s), SESSION_FILE, |current| {
            let Some(bytes) = current else {
                return Err(files::file_not_found(s, SESSION_FILE));
            };

            let mut data = parse(bytes).map_err(StoreError::BadJson)?;
            merge(&mut data, fields);

            let now = Utc::now().format(UPDATED_FORMAT).to_string();
            data.insert(UPDATED.to_string(), Value::String(now));

            let text = serde_json::to_string_pretty(&data)
                .map_err(|e| StoreError::BadJson(e.to_string()))?;

            Ok(format!("{text}\n"))
        })
    }

    /// Sets `archived: true` and `updated` in the `session.json` of `s`.
    pub fn archive_session(&self, s: &SessionRef) -> Result<String, StoreError> {
        let mut fields = Map::new();
        fields.insert(ARCHIVED.to_string(), Value::Bool(true));

        self.update_session(s, &fields)
    }
}

/// Returns `bad_name` unless `head` is a short or full commit sha.
fn check_head(head: &str) -> Result<(), StoreError> {
    let sha = (4..=40).contains(&head.len()) && head.bytes().all(|b| b.is_ascii_hexdigit());

    if sha {
        Ok(())
    } else {
        Err(StoreError::BadName(format!("{head:?} is not a commit sha")))
    }
}

/// Returns the first of `base`, `base-2`, `base-3`, … that is not in `dir`.
pub(crate) fn free_name(dir: &Path, base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2;

    while dir.join(&name).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }

    name
}

/// Returns the sorted names of the folders in `dir`, skipping names that start with `.`. When `name` is given, returns only `name` if it is a folder.
pub(crate) fn folders(dir: &Path, name: Option<&str>) -> Result<Vec<String>, StoreError> {
    if let Some(name) = name {
        if dir.join(name).is_dir() {
            return Ok(vec![name.to_string()]);
        }

        return Ok(Vec::new());
    }

    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();

    for entry in fs::read_dir(dir).map_err(|e| files::io_error(dir, e))? {
        let entry = entry.map_err(|e| files::io_error(dir, e))?;

        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };

        if entry.path().is_dir() && !name.starts_with('.') {
            names.push(name);
        }
    }

    names.sort();

    Ok(names)
}

/// Fills `entry` from the `session.json` at `file`. Returns `None` for a missing file or an unwanted archived session.
fn load(mut entry: SessionEntry, file: &Path, include_archived: bool) -> Option<SessionEntry> {
    let bytes = match files::read(file) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return None,

        Err(e) => {
            entry.error = Some(e.to_string());
            return Some(entry);
        }
    };

    match parse(&bytes) {
        Ok(data) => {
            if data.get(ARCHIVED) == Some(&Value::Bool(true)) && !include_archived {
                return None;
            }

            entry.data = Some(Value::Object(data));
        }

        Err(reason) => entry.error = Some(reason),
    }

    Some(entry)
}

/// Parses `bytes` as a JSON object, or returns why they are not one.
fn parse(bytes: &[u8]) -> Result<Map<String, Value>, String> {
    match serde_json::from_slice(bytes) {
        Ok(Value::Object(data)) => Ok(data),
        Ok(_) => Err(format!("{SESSION_FILE} is not a JSON object")),
        Err(e) => Err(format!("{SESSION_FILE} does not parse: {e}")),
    }
}

/// Sets each of `fields` in `data`, merging `counts` per key when both are objects.
fn merge(data: &mut Map<String, Value>, fields: &Map<String, Value>) {
    for (key, value) in fields {
        if key == COUNTS
            && let (Some(Value::Object(old)), Value::Object(new)) = (data.get_mut(key), value)
        {
            old.extend(new.clone());
            continue;
        }

        data.insert(key.clone(), value.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::thread;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn store() -> (TempDir, Store) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path().to_path_buf());
        (root, store)
    }

    fn session(org: &str, repo: &str, id: &str) -> SessionRef {
        SessionRef::new(org, repo, "main", id).unwrap()
    }

    fn create(store: &Store, s: &SessionRef) {
        fs::create_dir_all(store.dir(s)).unwrap();
    }

    fn write_session(store: &Store, s: &SessionRef, data: Value) {
        create(store, s);
        let text = format!("{}\n", serde_json::to_string_pretty(&data).unwrap());
        store.write_file(s, SESSION_FILE, &text, None).unwrap();
    }

    fn read_session(store: &Store, s: &SessionRef) -> Map<String, Value> {
        let content = store.read_file(s, SESSION_FILE).unwrap().content;
        serde_json::from_str(&content).unwrap()
    }

    fn names(entries: &[SessionEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| format!("{}/{}/{}/{}", e.org, e.repo, e.branch, e.session))
            .collect()
    }

    fn fields(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(fields) => fields,
            _ => panic!("not an object: {value}"),
        }
    }

    fn is_utc_timestamp(text: &str) -> bool {
        let pattern = "0000-00-00T00:00:00Z";

        text.len() == pattern.len()
            && text.chars().zip(pattern.chars()).all(|(c, p)| match p {
                '0' => c.is_ascii_digit(),
                _ => c == p,
            })
    }

    #[test]
    fn lists_sessions_across_orgs_sorted() {
        let (_root, store) = store();

        for (org, repo, id) in [
            ("b-org", "repo", "session-1"),
            ("a-org", "z-repo", "walk-2"),
            ("a-org", "a-repo", "walk-3"),
            ("a-org", "a-repo", "review-1"),
        ] {
            write_session(&store, &session(org, repo, id), json!({ "id": id }));
        }

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert_eq!(
            names(&entries),
            [
                "a-org/a-repo/main/review-1",
                "a-org/a-repo/main/walk-3",
                "a-org/z-repo/main/walk-2",
                "b-org/repo/main/session-1"
            ]
        );
        assert_eq!(entries[0].category, Category::Review);
        assert_eq!(entries[3].category, Category::Session);
        assert_eq!(entries[3].data, Some(json!({ "id": "session-1" })));
        assert_eq!(entries[3].error, None);
    }

    #[test]
    fn missing_root_lists_nothing() {
        let (root, _store) = store();
        let store = Store::new(root.path().join("missing"));

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert!(entries.is_empty());
    }

    #[test]
    fn skips_archived_unless_asked() {
        let (_root, store) = store();
        write_session(&store, &session("o", "r", "walk-live"), json!({}));
        write_session(
            &store,
            &session("o", "r", "walk-kept"),
            json!({ "archived": false }),
        );
        write_session(
            &store,
            &session("o", "r", "walk-old"),
            json!({ "archived": true }),
        );

        let listed = store.list_sessions(&SessionFilter::default()).unwrap();
        let all = store
            .list_sessions(&SessionFilter {
                include_archived: true,
                ..SessionFilter::default()
            })
            .unwrap();

        assert_eq!(names(&listed), ["o/r/main/walk-kept", "o/r/main/walk-live"]);
        assert_eq!(
            names(&all),
            [
                "o/r/main/walk-kept",
                "o/r/main/walk-live",
                "o/r/main/walk-old"
            ]
        );
    }

    #[test]
    fn filters_by_org_and_repo() {
        let (_root, store) = store();

        for (org, repo) in [("o1", "r1"), ("o1", "r2"), ("o2", "r1")] {
            write_session(&store, &session(org, repo, "walk-1"), json!({}));
        }

        let filter = |org: Option<&str>, repo: Option<&str>| SessionFilter {
            org: org.map(str::to_string),
            repo: repo.map(str::to_string),
            ..SessionFilter::default()
        };

        let list = |org, repo| names(&store.list_sessions(&filter(org, repo)).unwrap());

        assert_eq!(
            list(Some("o1"), None),
            ["o1/r1/main/walk-1", "o1/r2/main/walk-1"]
        );
        assert_eq!(list(Some("o1"), Some("r2")), ["o1/r2/main/walk-1"]);
        assert_eq!(
            list(None, Some("r1")),
            ["o1/r1/main/walk-1", "o2/r1/main/walk-1"]
        );
        assert!(list(Some("o3"), None).is_empty());

        for (org, repo) in [(Some(".."), None), (None, Some("a/b"))] {
            let result = store.list_sessions(&filter(org, repo));

            assert_eq!(result.unwrap_err().code(), "bad_name");
        }
    }

    #[test]
    fn filters_by_branch_and_category() {
        let (_root, store) = store();

        for (branch, id) in [
            ("main", "walk-1"),
            ("main", "session-1"),
            ("fix", "walk-1"),
            ("fix", "review-1"),
        ] {
            let s = SessionRef::new("o", "r", branch, id).unwrap();
            write_session(&store, &s, json!({}));
        }

        let list = |branch: Option<&str>, category: Option<Category>| {
            let filter = SessionFilter {
                branch: branch.map(str::to_string),
                category,
                ..SessionFilter::default()
            };

            names(&store.list_sessions(&filter).unwrap())
        };

        assert_eq!(
            list(Some("fix"), None),
            ["o/r/fix/review-1", "o/r/fix/walk-1"]
        );
        assert_eq!(
            list(None, Some(Category::Walk)),
            ["o/r/fix/walk-1", "o/r/main/walk-1"]
        );
        assert_eq!(
            list(Some("main"), Some(Category::Session)),
            ["o/r/main/session-1"]
        );
        assert!(list(Some("none"), None).is_empty());

        let bad = SessionFilter {
            branch: Some("a/b".to_string()),
            ..SessionFilter::default()
        };

        assert_eq!(store.list_sessions(&bad).unwrap_err().code(), "bad_name");
    }

    #[test]
    fn broken_session_json_is_listed_with_error() {
        let (_root, store) = store();
        let s = session("o", "r", "walk-broken");
        create(&store, &s);
        store
            .write_file(&s, SESSION_FILE, "{ broken", None)
            .unwrap();

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert_eq!(names(&entries), ["o/r/main/walk-broken"]);
        assert_eq!(entries[0].data, None);
        assert!(entries[0].error.is_some());

        let reply = fields(serde_json::to_value(&entries[0]).unwrap());

        assert!(!reply.contains_key("data"));
        assert!(reply.contains_key("error"));
        assert_eq!(reply["category"], "walk");
    }

    #[test]
    fn folder_without_session_json_or_category_is_skipped() {
        let (_root, store) = store();
        let s = session("o", "r", "walk-bare");
        create(&store, &s);
        store.write_file(&s, "spec.md", "spec\n", None).unwrap();
        create(&store, &session("o", "r", "walk-empty"));
        write_session(&store, &session("o", "r", "rate-limit"), json!({}));
        fs::create_dir_all(store.root().join("o").join("no-sessions")).unwrap();

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert!(entries.is_empty());
    }

    fn at(minute: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("2026-11-04T{minute}:30Z"))
            .unwrap()
            .with_timezone(&Utc)
    }

    fn start(store: &Store, category: Category, now: DateTime<Utc>) -> Result<Started, StoreError> {
        let fields = fields(json!({ "flow": "walk", "branch": "octonato/fix" }));
        store.start_session_at("o", "r", "fix", category, "3f00486", &fields, now)
    }

    #[test]
    fn start_session_names_the_folder_and_writes_session_json() {
        let (_root, store) = store();

        let started = start(&store, Category::Walk, at("09:12")).unwrap();

        assert_eq!(
            started.session.to_string(),
            "o/r/fix/walk-20261104-0912-3f00486"
        );

        let read = store.read_file(&started.session, SESSION_FILE).unwrap();
        let data = read_session(&store, &started.session);

        assert_eq!(read.version, started.version);
        assert_eq!(data["id"], "walk-20261104-0912-3f00486");
        assert_eq!(data["category"], "walk");
        assert_eq!(data["flow"], "walk");
        assert_eq!(data["branch"], "octonato/fix");
        assert_eq!(data["archived"], false);
        assert_eq!(data["updated"], "2026-11-04T09:12:30Z");
    }

    #[test]
    fn start_session_suffixes_a_taken_name() {
        let (_root, store) = store();

        let ids: Vec<String> = (0..3)
            .map(|_| start(&store, Category::Walk, at("09:12")).unwrap())
            .map(|started| started.session.session().to_string())
            .collect();

        assert_eq!(
            ids,
            [
                "walk-20261104-0912-3f00486",
                "walk-20261104-0912-3f00486-2",
                "walk-20261104-0912-3f00486-3"
            ]
        );
    }

    #[test]
    fn start_session_refuses_a_second_live_session_or_review() {
        let (_root, store) = store();

        for category in [Category::Session, Category::Review] {
            let first = start(&store, category, at("09:12")).unwrap();
            let second = start(&store, category, at("09:13"));

            let error = second.unwrap_err();
            assert_eq!(error.code(), "exists");
            assert!(
                error.to_string().contains(first.session.session()),
                "{error}"
            );

            store.archive_session(&first.session).unwrap();

            assert!(start(&store, category, at("09:14")).is_ok());
        }
    }

    #[test]
    fn start_session_refuses_bad_names() {
        let (_root, store) = store();
        let fields = Map::new();
        let now = at("09:12");

        for head in ["", "xyz1234", "3f0", "3f00486/.."] {
            let result =
                store.start_session_at("o", "r", "fix", Category::Walk, head, &fields, now);
            assert_eq!(result.unwrap_err().code(), "bad_name", "{head:?}");
        }

        let result =
            store.start_session_at("o", "r", "a/b", Category::Walk, "3f00486", &fields, now);
        assert_eq!(result.unwrap_err().code(), "bad_name");
    }

    #[test]
    fn concurrent_starts_get_distinct_names() {
        let (_root, store) = store();

        let ids: HashSet<String> = thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| start(&store, Category::Walk, at("09:12")).unwrap()))
                .collect();

            workers
                .into_iter()
                .map(|worker| worker.join().unwrap().session.session().to_string())
                .collect()
        });

        assert_eq!(ids.len(), 8);
    }

    #[test]
    fn concurrent_session_starts_leave_one_live() {
        let (_root, store) = store();

        let started = thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| start(&store, Category::Session, at("09:12")).is_ok()))
                .collect();

            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(|ok| *ok)
                .count()
        });

        assert_eq!(started, 1);
    }

    #[test]
    fn update_merges_fields_and_counts() {
        let (_root, store) = store();
        let s = session("o", "r", "walk-1");
        write_session(
            &store,
            &s,
            json!({ "id": "s1", "stage": "spec", "counts": { "a": 1, "b": 2 }, "mode": "auto" }),
        );

        let update = fields(json!({ "counts": { "b": 5 }, "stage": "plan" }));
        let version = store.update_session(&s, &update).unwrap();

        let read = store.read_file(&s, SESSION_FILE).unwrap();
        let data = read_session(&store, &s);
        let keys: Vec<&str> = data.keys().map(String::as_str).collect();

        assert_eq!(read.version, version);
        assert!(read.content.ends_with("}\n"));
        assert_eq!(
            read.content,
            format!("{}\n", serde_json::to_string_pretty(&data).unwrap())
        );
        assert_eq!(keys, ["id", "stage", "counts", "mode", "updated"]);
        assert_eq!(data["stage"], "plan");
        assert_eq!(
            serde_json::to_string(&data["counts"]).unwrap(),
            r#"{"a":1,"b":5}"#
        );
        assert!(is_utc_timestamp(data["updated"].as_str().unwrap()));
    }

    #[test]
    fn update_replaces_counts_that_are_not_objects() {
        let (_root, store) = store();
        let s = session("o", "r", "walk-1");
        write_session(&store, &s, json!({ "counts": 3 }));

        let update = fields(json!({ "counts": { "a": 1 } }));
        store.update_session(&s, &update).unwrap();

        assert_eq!(read_session(&store, &s)["counts"], json!({ "a": 1 }));
    }

    #[test]
    fn update_needs_an_existing_session_json() {
        let (_root, store) = store();
        let update = fields(json!({ "stage": "plan" }));

        let missing = session("o", "r", "walk-missing");
        let bare = session("o", "r", "walk-bare");
        create(&store, &bare);
        store.write_file(&bare, "spec.md", "spec\n", None).unwrap();

        assert_eq!(
            store.update_session(&missing, &update).unwrap_err().code(),
            "not_found"
        );
        assert_eq!(
            store.update_session(&bare, &update).unwrap_err().code(),
            "not_found"
        );

        for content in ["[1, 2]\n", "{ broken"] {
            let s = session("o", "r", "walk-odd");
            let _ = fs::remove_dir_all(store.dir(&s));
            create(&store, &s);
            store.write_file(&s, SESSION_FILE, content, None).unwrap();

            let result = store.update_session(&s, &update);

            assert_eq!(result.unwrap_err().code(), "bad_json", "{content:?}");
            assert_eq!(store.read_file(&s, SESSION_FILE).unwrap().content, content);
        }
    }

    #[test]
    fn archive_sets_archived_and_updated() {
        let (_root, store) = store();
        let s = session("o", "r", "walk-1");
        write_session(&store, &s, json!({ "stage": "done" }));

        let version = store.archive_session(&s).unwrap();

        let data = read_session(&store, &s);
        let listed = store.list_sessions(&SessionFilter::default()).unwrap();

        assert_eq!(store.read_file(&s, SESSION_FILE).unwrap().version, version);
        assert_eq!(data["stage"], "done");
        assert_eq!(data["archived"], true);
        assert!(is_utc_timestamp(data["updated"].as_str().unwrap()));
        assert!(listed.is_empty());
    }

    #[test]
    fn concurrent_updates_lose_nothing() {
        let (_root, store) = store();
        let s = session("o", "r", "walk-1");
        write_session(&store, &s, json!({ "id": "s1" }));

        thread::scope(|scope| {
            for worker in 0..8 {
                let (store, s) = (&store, &s);

                scope.spawn(move || {
                    for call in 0..25 {
                        let mut update = Map::new();
                        update.insert(format!("{worker}-{call}"), json!(call));

                        store.update_session(s, &update).unwrap();
                    }
                });
            }

            scope.spawn(|| {
                for _ in 0..25 {
                    store.archive_session(&s).unwrap();
                }
            });
        });

        let data = read_session(&store, &s);

        for worker in 0..8 {
            for call in 0..25 {
                let key = format!("{worker}-{call}");
                assert_eq!(data.get(&key), Some(&json!(call)), "{key}");
            }
        }

        assert_eq!(data["archived"], true);
    }
}
