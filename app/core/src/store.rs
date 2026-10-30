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

//! The factory store calls the board makes.

use jam_factory_store::{FileRead, SessionEntry, SessionRef, StoreError};

/// Reads the sessions of every org and archives them.
pub trait Store: Send + Sync {
    /// Lists the sessions that are not archived.
    fn list_sessions(&self) -> Result<Vec<SessionEntry>, StoreError>;

    /// Lists the relative paths of the files in the session `s`.
    fn list_files(&self, s: &SessionRef) -> Result<Vec<String>, StoreError>;

    /// Reads the file at `path` in the session `s`.
    fn read_file(&self, s: &SessionRef, path: &str) -> Result<FileRead, StoreError>;

    /// Writes `content` over `path` in the session `s` when its version is `expected_version`, and returns the new version.
    fn write_file(
        &self,
        s: &SessionRef,
        path: &str,
        content: &str,
        expected_version: &str,
    ) -> Result<String, StoreError>;

    /// Sets `archived: true` in the `session.json` of `s`.
    fn archive_session(&self, s: &SessionRef) -> Result<(), StoreError>;
}
