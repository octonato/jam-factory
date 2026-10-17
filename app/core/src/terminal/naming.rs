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

//! Names tmux sessions as `odeck/<org>/<repo>/<checkout>/<kind>`.

use serde::{Deserialize, Serialize};

/// What a terminal runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalKind {
    Shell,
    Claude,
}

impl TerminalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TerminalKind::Shell => "shell",
            TerminalKind::Claude => "claude",
        }
    }
}

/// The checkout a session belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTarget {
    pub org: String,
    pub repo: String,
    pub checkout: String,
}

/// The name prefix shared by all sessions of a repo.
pub fn session_prefix(org: &str, repo: &str) -> String {
    format!("odeck/{}/{}/", sanitize(org), sanitize(repo))
}

/// The session name for `target`. Index 1 has no suffix, and higher indexes add `-N`.
pub fn session_name(target: &SessionTarget, kind: TerminalKind, index: u32) -> String {
    let base = format!(
        "{}{}/{}",
        session_prefix(&target.org, &target.repo),
        sanitize(&target.checkout),
        kind.as_str()
    );

    if index <= 1 {
        base
    } else {
        format!("{base}-{index}")
    }
}

/// The lowest index whose session name is not in `live`.
pub fn next_index(target: &SessionTarget, kind: TerminalKind, live: &[String]) -> u32 {
    (1..)
        .find(|i| !live.contains(&session_name(target, kind, *i)))
        .expect("an unused index exists")
}

/// The kind and index of session `name` when it belongs to `target`.
pub fn parse_session(name: &str, target: &SessionTarget) -> Option<(TerminalKind, u32)> {
    [TerminalKind::Shell, TerminalKind::Claude]
        .into_iter()
        .find_map(|kind| {
            let base = session_name(target, kind, 1);
            if name == base {
                return Some((kind, 1));
            }

            let index: u32 = name.strip_prefix(&format!("{base}-"))?.parse().ok()?;
            (index >= 2).then_some((kind, index))
        })
}

fn sanitize(s: &str) -> String {
    s.replace(['.', ':'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(checkout: &str) -> SessionTarget {
        SessionTarget {
            org: "akka".to_string(),
            repo: "sdk".to_string(),
            checkout: checkout.to_string(),
        }
    }

    #[test]
    fn names_first_shell() {
        let name = session_name(&target("main"), TerminalKind::Shell, 1);

        assert_eq!(name, "odeck/akka/sdk/main/shell");
    }

    #[test]
    fn names_second_claude() {
        let name = session_name(&target("main"), TerminalKind::Claude, 2);

        assert_eq!(name, "odeck/akka/sdk/main/claude-2");
    }

    #[test]
    fn sanitizes_dot_and_colon() {
        let t = SessionTarget {
            org: "my.org".to_string(),
            repo: "re:po".to_string(),
            checkout: "v1.2:rc".to_string(),
        };

        let name = session_name(&t, TerminalKind::Shell, 1);

        assert_eq!(name, "odeck/my_org/re_po/v1_2_rc/shell");
        assert_eq!(session_prefix("my.org", "re:po"), "odeck/my_org/re_po/");
    }

    #[test]
    fn next_index_fills_gaps() {
        let live = vec![
            "odeck/akka/sdk/main/shell".to_string(),
            "odeck/akka/sdk/main/shell-3".to_string(),
            "odeck/akka/sdk/other/shell-2".to_string(),
        ];

        assert_eq!(next_index(&target("main"), TerminalKind::Shell, &live), 2);
        assert_eq!(next_index(&target("main"), TerminalKind::Claude, &live), 1);
        assert_eq!(next_index(&target("other"), TerminalKind::Shell, &live), 1);
        assert_eq!(next_index(&target("main"), TerminalKind::Shell, &[]), 1);
    }

    #[test]
    fn parses_session_names_of_a_target() {
        let t = target("v1.2");

        assert_eq!(
            parse_session("odeck/akka/sdk/v1_2/shell", &t),
            Some((TerminalKind::Shell, 1))
        );
        assert_eq!(
            parse_session("odeck/akka/sdk/v1_2/claude-3", &t),
            Some((TerminalKind::Claude, 3))
        );
        assert_eq!(parse_session("odeck/akka/sdk/v1_2/shell-x", &t), None);
        assert_eq!(parse_session("odeck/akka/sdk/v1_2/shell-1", &t), None);
        assert_eq!(parse_session("odeck/akka/sdk/main/shell", &t), None);
        assert_eq!(parse_session("work", &t), None);
    }
}
