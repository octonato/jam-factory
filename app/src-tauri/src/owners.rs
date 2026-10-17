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

//! Records which window owns each terminal.

use std::collections::HashMap;
use std::sync::Mutex;

use odeck_core::terminal::TerminalId;

/// Maps terminal ids to the label of the window that opened them.
#[derive(Default)]
pub struct TerminalOwners {
    owners: Mutex<HashMap<TerminalId, String>>,
}

impl TerminalOwners {
    pub fn add(&self, label: &str, id: TerminalId) {
        self.owners.lock().unwrap().insert(id, label.to_string());
    }

    pub fn remove(&self, id: TerminalId) {
        self.owners.lock().unwrap().remove(&id);
    }

    /// Removes and returns the terminals that window `label` opened.
    pub fn take(&self, label: &str) -> Vec<TerminalId> {
        let mut owners = self.owners.lock().unwrap();
        let ids: Vec<_> = owners
            .iter()
            .filter(|(_, owner)| owner.as_str() == label)
            .map(|(id, _)| *id)
            .collect();

        for id in &ids {
            owners.remove(id);
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_returns_and_forgets_a_windows_terminals() {
        let owners = TerminalOwners::default();
        owners.add("project-1", 1);
        owners.add("project-1", 2);
        owners.add("project-2", 3);

        let mut taken = owners.take("project-1");
        taken.sort();

        assert_eq!(taken, vec![1, 2]);
        assert!(owners.take("project-1").is_empty());
        assert_eq!(owners.take("project-2"), vec![3]);
    }

    #[test]
    fn removed_terminals_are_not_taken() {
        let owners = TerminalOwners::default();
        owners.add("project-1", 1);
        owners.add("project-1", 2);

        owners.remove(1);

        assert_eq!(owners.take("project-1"), vec![2]);
    }
}
