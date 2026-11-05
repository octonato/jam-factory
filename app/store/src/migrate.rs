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

//! Moves sessions from `sessions/{id}` into `sessions/{branch}/{category}-{stamp}-migrated`.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use crate::error::StoreError;
use crate::files::{self, LOCK_FILE, SESSIONS_DIR, Store};
use crate::name::{Category, check_name};
use crate::sessions::{self, SESSION_FILE};

const MIGRATE_LOCK: &str = ".migrate.lock";
const MIGRATING: &str = ".migrating";
const SUFFIX: &str = "migrated";

/// A session `migrate` moved, as paths relative to the factory root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
}

impl Store {
    /// Moves each session folder under `sessions/` that holds `session.json` into its branch folder.
    pub fn migrate(&self) -> Result<Vec<Moved>, StoreError> {
        self.migrate_at(Utc::now())
    }

    pub(crate) fn migrate_at(&self, now: DateTime<Utc>) -> Result<Vec<Moved>, StoreError> {
        if !self.root().is_dir() {
            return Ok(Vec::new());
        }

        let _lock = files::lock_file(&self.root().join(MIGRATE_LOCK))?;
        let mut moved = Vec::new();

        for org in sessions::folders(self.root(), None)? {
            let org_dir = self.root().join(&org);

            for repo in sessions::folders(&org_dir, None)? {
                let sessions_dir = org_dir.join(&repo).join(SESSIONS_DIR);

                for old in sessions::folders(&sessions_dir, None)? {
                    if !sessions_dir.join(&old).join(SESSION_FILE).is_file() {
                        continue;
                    }

                    let to = migrate_one(&sessions_dir, &old, now)?;

                    moved.push(Moved {
                        from: format!("{org}/{repo}/{SESSIONS_DIR}/{old}"),
                        to: format!("{org}/{repo}/{SESSIONS_DIR}/{to}"),
                    });
                }
            }
        }

        Ok(moved)
    }
}

/// Moves the session `old` into its branch folder. Returns the new `{branch}/{id}`.
fn migrate_one(sessions_dir: &Path, old: &str, now: DateTime<Utc>) -> Result<String, StoreError> {
    let old_dir = sessions_dir.join(old);
    let data = read_data(&old_dir.join(SESSION_FILE));

    let branch = branch_of(&data, old);
    let category = category_of(&data);
    let stamp = stamp_of(&data, now);

    let temp = sessions_dir.join(format!("{MIGRATING}-{old}"));
    fs::rename(&old_dir, &temp).map_err(|e| files::io_error(&old_dir, e))?;

    let branch_dir = sessions_dir.join(&branch);
    fs::create_dir_all(&branch_dir).map_err(|e| files::io_error(&branch_dir, e))?;

    let id = sessions::free_name(&branch_dir, &format!("{category}-{stamp}-{SUFFIX}"));
    let new_dir = branch_dir.join(&id);
    fs::rename(&temp, &new_dir).map_err(|e| files::io_error(&temp, e))?;

    let _ = fs::remove_file(new_dir.join(LOCK_FILE));

    if let Some(mut data) = data {
        data.insert("id".to_string(), Value::String(id.clone()));
        data.insert("category".to_string(), Value::String(category.to_string()));

        let text =
            serde_json::to_string_pretty(&data).map_err(|e| StoreError::BadJson(e.to_string()))?;
        let file = new_dir.join(SESSION_FILE);
        fs::write(&file, format!("{text}\n")).map_err(|e| files::io_error(&file, e))?;
    }

    Ok(format!("{branch}/{id}"))
}

/// Returns the JSON object in `file`, or `None` when it does not parse as one.
fn read_data(file: &Path) -> Option<Map<String, Value>> {
    let bytes = fs::read(file).ok()?;

    match serde_json::from_slice(&bytes) {
        Ok(Value::Object(data)) => Some(data),
        _ => None,
    }
}

/// Returns the last segment of the `branch` field, or `old` when the field is missing or is not a valid name.
fn branch_of(data: &Option<Map<String, Value>>, old: &str) -> String {
    let branch = data
        .as_ref()
        .and_then(|data| data.get("branch"))
        .and_then(Value::as_str)
        .and_then(|branch| branch.rsplit('/').next())
        .filter(|branch| check_name(branch).is_ok() && !branch.starts_with('.'));

    branch.unwrap_or(old).to_string()
}

/// Returns the category for the `flow` field: `walk`, `review`, or `session` for any other value.
fn category_of(data: &Option<Map<String, Value>>) -> Category {
    let flow = data
        .as_ref()
        .and_then(|data| data.get("flow"))
        .and_then(Value::as_str);

    match flow {
        Some("walk") => Category::Walk,
        Some("review") => Category::Review,
        _ => Category::Session,
    }
}

/// Returns the `updated` time as `YYYYMMDD-HHMM`, or `now` when it is missing or does not parse.
fn stamp_of(data: &Option<Map<String, Value>>, now: DateTime<Utc>) -> String {
    let updated = data
        .as_ref()
        .and_then(|data| data.get("updated"))
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc));

    updated
        .unwrap_or(now)
        .format(sessions::STARTED_FORMAT)
        .to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::name::SessionRef;
    use crate::sessions::SessionFilter;

    fn store() -> (TempDir, Store) {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path().to_path_buf());
        (root, store)
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-04T09:12:30Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// Writes an old-layout session.
    fn old_session(store: &Store, id: &str, data: &str) {
        let dir = store.root().join("o").join("r").join(SESSIONS_DIR).join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(SESSION_FILE), data).unwrap();
        fs::write(dir.join("spec.md"), "spec\n").unwrap();
        fs::write(dir.join(LOCK_FILE), "").unwrap();
    }

    fn moves(moved: &[Moved]) -> Vec<(&str, &str)> {
        moved
            .iter()
            .map(|m| (m.from.as_str(), m.to.as_str()))
            .collect()
    }

    #[test]
    fn moves_old_sessions_into_their_branch_and_category() {
        let (_root, store) = store();
        let updated = r#""updated": "2026-10-15T14:02:11Z""#;

        old_session(
            &store,
            "rate-limit",
            &format!(
                r#"{{"id": "rate-limit", "branch": "octonato/rate-limit", "flow": "build", {updated}}}"#
            ),
        );
        old_session(
            &store,
            "walk-pr-247-20261104-0912",
            &format!(r#"{{"flow": "walk", "branch": "main", {updated}}}"#),
        );
        old_session(&store, "pr-9", r#"{"flow": "review"}"#);

        let moved = store.migrate_at(now()).unwrap();

        assert_eq!(
            moves(&moved),
            [
                (
                    "o/r/sessions/pr-9",
                    "o/r/sessions/pr-9/review-20261104-0912-migrated"
                ),
                (
                    "o/r/sessions/rate-limit",
                    "o/r/sessions/rate-limit/session-20261015-1402-migrated"
                ),
                (
                    "o/r/sessions/walk-pr-247-20261104-0912",
                    "o/r/sessions/main/walk-20261015-1402-migrated"
                ),
            ]
        );

        let s = SessionRef::new("o", "r", "rate-limit", "session-20261015-1402-migrated").unwrap();
        let files = store.list_files(&s).unwrap();
        let data: Value =
            serde_json::from_str(&store.read_file(&s, SESSION_FILE).unwrap().content).unwrap();

        assert_eq!(files, ["session.json", "spec.md"]);
        assert_eq!(data["id"], "session-20261015-1402-migrated");
        assert_eq!(data["category"], "session");
        assert_eq!(data["branch"], "octonato/rate-limit");

        let listed = store.list_sessions(&SessionFilter::default()).unwrap();
        assert_eq!(listed.len(), 3);
    }

    #[test]
    fn a_second_run_moves_nothing() {
        let (_root, store) = store();
        old_session(&store, "fix", r#"{"branch": "fix"}"#);

        assert_eq!(store.migrate_at(now()).unwrap().len(), 1);
        assert!(store.migrate_at(now()).unwrap().is_empty());
    }

    #[test]
    fn two_sessions_of_one_branch_get_distinct_names() {
        let (_root, store) = store();
        old_session(&store, "a", r#"{"branch": "main", "flow": "walk"}"#);
        old_session(&store, "b", r#"{"branch": "main", "flow": "walk"}"#);

        let moved = store.migrate_at(now()).unwrap();

        assert_eq!(
            moves(&moved),
            [
                (
                    "o/r/sessions/a",
                    "o/r/sessions/main/walk-20261104-0912-migrated"
                ),
                (
                    "o/r/sessions/b",
                    "o/r/sessions/main/walk-20261104-0912-migrated-2"
                ),
            ]
        );
    }

    #[test]
    fn a_broken_session_json_moves_as_a_session_of_its_folder() {
        let (_root, store) = store();
        old_session(&store, "odd", "{ broken");

        let moved = store.migrate_at(now()).unwrap();

        assert_eq!(
            moves(&moved),
            [(
                "o/r/sessions/odd",
                "o/r/sessions/odd/session-20261104-0912-migrated"
            )]
        );

        let file = store
            .root()
            .join("o/r/sessions/odd/session-20261104-0912-migrated")
            .join(SESSION_FILE);
        assert_eq!(fs::read_to_string(file).unwrap(), "{ broken");
    }

    #[test]
    fn new_sessions_and_a_missing_root_are_left_alone() {
        let (root, store) = store();
        let fields = json!({ "flow": "walk" });
        let fields = fields.as_object().unwrap();
        store
            .start_session("o", "r", "main", Category::Walk, "3f00486", fields)
            .unwrap();

        assert!(store.migrate_at(now()).unwrap().is_empty());

        let missing = Store::new(root.path().join("missing"));
        assert!(missing.migrate_at(now()).unwrap().is_empty());
        assert!(!root.path().join("missing").exists());
    }
}
