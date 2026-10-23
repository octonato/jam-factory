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

use crate::error::StoreError;

/// A session named by its org, its repo and its id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionRef {
    org: String,
    repo: String,
    session: String,
}

impl SessionRef {
    /// Returns the session `org/repo/session`, or `bad_name` when a name is not safe.
    pub fn new(org: &str, repo: &str, session: &str) -> Result<SessionRef, StoreError> {
        check_name(org)?;
        check_name(repo)?;
        check_name(session)?;

        Ok(SessionRef {
            org: org.to_string(),
            repo: repo.to_string(),
            session: session.to_string(),
        })
    }

    pub fn org(&self) -> &str {
        &self.org
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    pub fn session(&self) -> &str {
        &self.session
    }
}

impl fmt::Display for SessionRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.org, self.repo, self.session)
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
