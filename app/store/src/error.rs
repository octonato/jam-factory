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

//! The errors of the store, each with its code.

use thiserror::Error;

/// A store failure, with the code the MCP tools report.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StoreError {
    /// The session or the file does not exist.
    #[error("{0}")]
    NotFound(String),

    /// The file exists, but the call required a new file.
    #[error("{0}")]
    Exists(String),

    /// The expected version is not the current one.
    #[error("The version does not match. The current version is {current}.")]
    Conflict { current: String },

    #[error("old_str does not occur in the file")]
    NoMatch,

    #[error("old_str occurs more than once in the file")]
    ManyMatches,

    /// A name or a path breaks the naming rules.
    #[error("{0}")]
    BadName(String),

    /// `session.json` does not parse, or its session fields are not an object.
    #[error("{0}")]
    BadJson(String),

    /// Any other file error.
    #[error("{0}")]
    Io(String),
}

impl StoreError {
    pub fn code(&self) -> &'static str {
        match self {
            StoreError::NotFound(_) => "not_found",
            StoreError::Exists(_) => "exists",
            StoreError::Conflict { .. } => "conflict",
            StoreError::NoMatch => "no_match",
            StoreError::ManyMatches => "many_matches",
            StoreError::BadName(_) => "bad_name",
            StoreError::BadJson(_) => "bad_json",
            StoreError::Io(_) => "io",
        }
    }

    /// Returns the error with `code`, `message` and the `current` version, or `io` for an unknown code.
    pub fn from_code(code: &str, message: &str, current: Option<String>) -> StoreError {
        let message = message.to_string();

        match code {
            "not_found" => StoreError::NotFound(message),
            "exists" => StoreError::Exists(message),
            "conflict" => StoreError::Conflict {
                current: current.unwrap_or_default(),
            },
            "no_match" => StoreError::NoMatch,
            "many_matches" => StoreError::ManyMatches,
            "bad_name" => StoreError::BadName(message),
            "bad_json" => StoreError::BadJson(message),
            _ => StoreError::Io(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_code_inverts_code() {
        let errors = [
            StoreError::NotFound("gone".to_string()),
            StoreError::Exists("there".to_string()),
            StoreError::Conflict {
                current: "abc".to_string(),
            },
            StoreError::NoMatch,
            StoreError::ManyMatches,
            StoreError::BadName("bad".to_string()),
            StoreError::BadJson("broken".to_string()),
            StoreError::Io("disk".to_string()),
        ];

        for error in errors {
            let current = match &error {
                StoreError::Conflict { current } => Some(current.clone()),
                _ => None,
            };

            let back = StoreError::from_code(error.code(), &error.to_string(), current);

            assert_eq!(back, error);
        }
    }

    #[test]
    fn unknown_code_is_io() {
        let error = StoreError::from_code("weird", "something broke", None);

        assert_eq!(error, StoreError::Io("something broke".to_string()));
    }
}
