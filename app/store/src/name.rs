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

//! The naming rules for sessions and for the paths of their files.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::StoreError;

/// A session named by its org, its repo, its branch and its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionRef {
    org: String,
    repo: String,
    branch: String,
    session: String,
}

impl SessionRef {
    /// Returns the session `org/repo/branch/session`, or `bad_name` when a name is not safe.
    pub fn new(
        org: &str,
        repo: &str,
        branch: &str,
        session: &str,
    ) -> Result<SessionRef, StoreError> {
        check_name(org)?;
        check_name(repo)?;
        check_name(branch)?;
        check_name(session)?;

        Ok(SessionRef {
            org: org.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
            session: session.to_string(),
        })
    }

    pub fn org(&self) -> &str {
        &self.org
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn session(&self) -> &str {
        &self.session
    }
}

impl fmt::Display for SessionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}/{}",
            self.org, self.repo, self.branch, self.session
        )
    }
}

/// The kind of a session, named by the prefix of its folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Session,
    Review,
    Walk,
    Lesson,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Category::Session,
        Category::Review,
        Category::Walk,
        Category::Lesson,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Session => "session",
            Category::Review => "review",
            Category::Walk => "walk",
            Category::Lesson => "lesson",
        }
    }

    /// Returns the category of the folder `session`, or `None` when its prefix is unknown.
    pub fn of(session: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|category| {
            session
                .strip_prefix(category.name())
                .and_then(|rest| rest.strip_prefix('-'))
                .is_some_and(|rest| !rest.is_empty())
        })
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Category {
    type Err = StoreError;

    fn from_str(name: &str) -> Result<Category, StoreError> {
        Category::ALL
            .into_iter()
            .find(|category| category.name() == name)
            .ok_or_else(|| {
                StoreError::BadName(format!(
                    "{name:?} is not a category: use session, review, walk or lesson"
                ))
            })
    }
}

/// Returns `bad_name` unless `name` is a single safe folder name.
pub fn check_name(name: &str) -> Result<(), StoreError> {
    let safe = !matches!(name, "" | "." | "..") && !name.contains(['/', '\\', '\0']);

    if safe {
        Ok(())
    } else {
        Err(StoreError::BadName(format!("{name:?} is not a safe name")))
    }
}

/// Returns `bad_name` unless `path` is relative, with non-empty segments that do not start with `.`.
pub fn check_path(path: &str) -> Result<(), StoreError> {
    let safe = !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && !segment.starts_with('.'));

    if safe {
        Ok(())
    } else {
        Err(StoreError::BadName(format!("{path:?} is not a safe path")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_of_reads_the_folder_prefix() {
        let cases = [
            ("session-20261104-0912-3f00486", Some(Category::Session)),
            ("review-20261104-0912-3f00486", Some(Category::Review)),
            ("review-pr42-20261104-0912-3f00486", Some(Category::Review)),
            ("walk-20261104-0912-3f00486-2", Some(Category::Walk)),
            ("walk-", None),
            ("walk", None),
            ("walking-1", None),
            ("lesson-20261104-0912-3f00486", Some(Category::Lesson)),
            ("lessons-1", None),
            ("rate-limit", None),
        ];

        for (session, category) in cases {
            assert_eq!(Category::of(session), category, "{session}");
        }
    }

    #[test]
    fn category_parses_its_name() {
        for category in Category::ALL {
            assert_eq!(category.name().parse::<Category>(), Ok(category));
        }

        assert_eq!("build".parse::<Category>().unwrap_err().code(), "bad_name");
    }

    #[test]
    fn session_ref_prints_four_parts() {
        let s = SessionRef::new("o", "r", "b", "walk-1").unwrap();

        assert_eq!(s.to_string(), "o/r/b/walk-1");
        assert_eq!(s.branch(), "b");
    }
}
