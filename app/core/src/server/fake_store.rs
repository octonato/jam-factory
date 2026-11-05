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

//! A fake store for the server unit tests.

use std::collections::HashMap;
use std::sync::Mutex;

use jam_factory_store::{FileRead, SessionEntry, SessionRef, StoreError};

use crate::store::Store;

/// A store holding the files of the session `o/stack/main/a`.
/// It logs each read and returns `failure` when set.
#[derive(Default)]
pub(crate) struct FakeStore {
    files: Mutex<HashMap<String, String>>,
    reads: Mutex<Vec<String>>,
    pub(crate) failure: Option<StoreError>,
}

impl FakeStore {
    pub(crate) fn write(&self, path: &str, content: &str) {
        let mut files = self.files.lock().unwrap();
        files.insert(path.to_string(), content.to_string());
    }

    pub(crate) fn remove(&self, path: &str) {
        self.files.lock().unwrap().remove(path);
    }

    pub(crate) fn reads(&self) -> Vec<String> {
        self.reads.lock().unwrap().clone()
    }
}

impl Store for FakeStore {
    fn list_sessions(&self) -> Result<Vec<SessionEntry>, StoreError> {
        unreachable!("lists no sessions")
    }

    fn list_files(&self, _s: &SessionRef) -> Result<Vec<String>, StoreError> {
        unreachable!("lists no files")
    }

    fn read_file(&self, s: &SessionRef, path: &str) -> Result<FileRead, StoreError> {
        self.reads.lock().unwrap().push(format!("{s}/{path}"));

        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }

        let content = match s.to_string().as_str() {
            "o/stack/main/a" => self.files.lock().unwrap().get(path).cloned(),
            _ => None,
        };

        content
            .map(|content| FileRead {
                content,
                version: "v1".to_string(),
            })
            .ok_or_else(|| StoreError::NotFound(format!("{s}/{path} does not exist")))
    }

    fn write_file(
        &self,
        _s: &SessionRef,
        _path: &str,
        _content: &str,
        _expected_version: &str,
    ) -> Result<String, StoreError> {
        unreachable!("writes no file")
    }

    fn archive_session(&self, _s: &SessionRef) -> Result<(), StoreError> {
        unreachable!("archives no session")
    }
}
