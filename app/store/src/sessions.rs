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

//! Lists sessions and updates their `session.json`.

use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::StoreError;
use crate::files::{self, SESSIONS_DIR, Store};
use crate::name::{SessionRef, check_name};

pub const SESSION_FILE: &str = "session.json";

const ARCHIVED: &str = "archived";
const COUNTS: &str = "counts";
const UPDATED: &str = "updated";
const UPDATED_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

/// The org, the repo and the archived state of the sessions to list.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub org: Option<String>,
    pub repo: Option<String>,
    pub include_archived: bool,
}

/// A session with its parsed `session.json`, or the reason it does not parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub org: String,
    pub repo: String,
    pub session: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Store {
    /// Lists the sessions that match `filter`, sorted by org, repo and id.
    pub fn list_sessions(&self, filter: &SessionFilter) -> Result<Vec<SessionEntry>, StoreError> {
        for name in [&filter.org, &filter.repo].into_iter().flatten() {
            check_name(name)?;
        }

        let mut entries = Vec::new();

        for org in folders(self.root(), filter.org.as_deref())? {
            let org_dir = self.root().join(&org);

            for repo in folders(&org_dir, filter.repo.as_deref())? {
                let sessions_dir = org_dir.join(&repo).join(SESSIONS_DIR);

                for session in folders(&sessions_dir, None)? {
                    let entry = SessionEntry {
                        org: org.clone(),
                        repo: repo.clone(),
                        session: session.clone(),
                        data: None,
                        error: None,
                    };

                    let file = sessions_dir.join(&session).join(SESSION_FILE);
                    entries.extend(load(entry, &file, filter.include_archived));
                }
            }
        }

        Ok(entries)
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

/// Returns the sorted names of the folders in `dir`, or only `name` when it is given and is a folder.
fn folders(dir: &Path, name: Option<&str>) -> Result<Vec<String>, StoreError> {
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

        if entry.path().is_dir() {
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
        SessionRef::new(org, repo, id).unwrap()
    }

    fn write_session(store: &Store, s: &SessionRef, data: Value) {
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
            .map(|e| format!("{}/{}/{}", e.org, e.repo, e.session))
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
            ("b-org", "repo", "s1"),
            ("a-org", "z-repo", "s2"),
            ("a-org", "a-repo", "s3"),
            ("a-org", "a-repo", "s1"),
        ] {
            write_session(&store, &session(org, repo, id), json!({ "id": id }));
        }

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert_eq!(
            names(&entries),
            [
                "a-org/a-repo/s1",
                "a-org/a-repo/s3",
                "a-org/z-repo/s2",
                "b-org/repo/s1"
            ]
        );
        assert_eq!(entries[3].data, Some(json!({ "id": "s1" })));
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
        write_session(&store, &session("o", "r", "live"), json!({}));
        write_session(
            &store,
            &session("o", "r", "kept"),
            json!({ "archived": false }),
        );
        write_session(
            &store,
            &session("o", "r", "old"),
            json!({ "archived": true }),
        );

        let listed = store.list_sessions(&SessionFilter::default()).unwrap();
        let all = store
            .list_sessions(&SessionFilter {
                include_archived: true,
                ..SessionFilter::default()
            })
            .unwrap();

        assert_eq!(names(&listed), ["o/r/kept", "o/r/live"]);
        assert_eq!(names(&all), ["o/r/kept", "o/r/live", "o/r/old"]);
    }

    #[test]
    fn filters_by_org_and_repo() {
        let (_root, store) = store();

        for (org, repo) in [("o1", "r1"), ("o1", "r2"), ("o2", "r1")] {
            write_session(&store, &session(org, repo, "s"), json!({}));
        }

        let filter = |org: Option<&str>, repo: Option<&str>| SessionFilter {
            org: org.map(str::to_string),
            repo: repo.map(str::to_string),
            include_archived: false,
        };

        let list = |org, repo| names(&store.list_sessions(&filter(org, repo)).unwrap());

        assert_eq!(list(Some("o1"), None), ["o1/r1/s", "o1/r2/s"]);
        assert_eq!(list(Some("o1"), Some("r2")), ["o1/r2/s"]);
        assert_eq!(list(None, Some("r1")), ["o1/r1/s", "o2/r1/s"]);
        assert!(list(Some("o3"), None).is_empty());

        for (org, repo) in [(Some(".."), None), (None, Some("a/b"))] {
            let result = store.list_sessions(&filter(org, repo));

            assert_eq!(result.unwrap_err().code(), "bad_name");
        }
    }

    #[test]
    fn broken_session_json_is_listed_with_error() {
        let (_root, store) = store();
        let s = session("o", "r", "broken");
        store
            .write_file(&s, SESSION_FILE, "{ broken", None)
            .unwrap();

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert_eq!(names(&entries), ["o/r/broken"]);
        assert_eq!(entries[0].data, None);
        assert!(entries[0].error.is_some());

        let reply = fields(serde_json::to_value(&entries[0]).unwrap());

        assert!(!reply.contains_key("data"));
        assert!(reply.contains_key("error"));
    }

    #[test]
    fn folder_without_session_json_is_skipped() {
        let (_root, store) = store();
        let s = session("o", "r", "bare");
        store.write_file(&s, "spec.md", "spec\n", None).unwrap();
        fs::create_dir_all(store.dir(&session("o", "r", "empty"))).unwrap();
        fs::create_dir_all(store.root().join("o").join("no-sessions")).unwrap();

        let entries = store.list_sessions(&SessionFilter::default()).unwrap();

        assert!(entries.is_empty());
    }

    #[test]
    fn update_merges_fields_and_counts() {
        let (_root, store) = store();
        let s = session("o", "r", "s1");
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
        let s = session("o", "r", "s1");
        write_session(&store, &s, json!({ "counts": 3 }));

        let update = fields(json!({ "counts": { "a": 1 } }));
        store.update_session(&s, &update).unwrap();

        assert_eq!(read_session(&store, &s)["counts"], json!({ "a": 1 }));
    }

    #[test]
    fn update_needs_an_existing_session_json() {
        let (_root, store) = store();
        let update = fields(json!({ "stage": "plan" }));

        let missing = session("o", "r", "missing");
        let bare = session("o", "r", "bare");
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
            let s = session("o", "r", "odd");
            let _ = fs::remove_dir_all(store.dir(&s));
            store.write_file(&s, SESSION_FILE, content, None).unwrap();

            let result = store.update_session(&s, &update);

            assert_eq!(result.unwrap_err().code(), "bad_json", "{content:?}");
            assert_eq!(store.read_file(&s, SESSION_FILE).unwrap().content, content);
        }
    }

    #[test]
    fn archive_sets_archived_and_updated() {
        let (_root, store) = store();
        let s = session("o", "r", "s1");
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
        let s = session("o", "r", "s1");
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
