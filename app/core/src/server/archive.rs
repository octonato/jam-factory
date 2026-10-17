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

//! Archives a job on `POST /archive/<repo>/<id>`.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;
use serde_json::{Map, Value};

use super::http::{Request, Response};

/// Answers `POST` for the factory root `root`.
pub fn post(root: &Path, request: &Request) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let Some(job) = job_file(root, &request.target) else {
        return Response::error(404);
    };

    match archive(&job) {
        Ok(()) => Response::new(204, "text/plain; charset=utf-8", Vec::new()),
        Err(_) => Response::error(500),
    }
}

/// Whether the request has no `Origin`, or one equal to `http://<Host>`.
fn same_origin(request: &Request) -> bool {
    let Some(origin) = request.header("Origin") else {
        return true;
    };

    request
        .header("Host")
        .is_some_and(|host| origin == format!("http://{host}"))
}

/// The `job.json` of the job named in `target`, if the names are safe and the file exists.
fn job_file(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split('?').next().unwrap_or_default();

    let ["", "archive", repo, id] = path.split('/').collect::<Vec<_>>()[..] else {
        return None;
    };

    let repo = name(repo)?;
    let id = name(id)?;

    let file = root.join(repo).join("jobs").join(id).join("job.json");
    file.is_file().then_some(file)
}

/// The percent-decoded `segment`, if it is a single safe folder name.
fn name(segment: &str) -> Option<String> {
    let name = percent_decode_str(segment).decode_utf8().ok()?;

    match name.as_ref() {
        "" | "." | ".." => None,
        _ if name.contains(['/', '\\']) => None,
        _ => Some(name.into_owned()),
    }
}

/// Marks the job in `file` as archived, through a temp file and a rename.
fn archive(file: &Path) -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string(file)?;
    let mut job: Map<String, Value> = serde_json::from_str(&text)?;

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    job.insert("archived".to_string(), Value::Bool(true));
    job.insert("updated".to_string(), Value::String(now));

    let mut text = serde_json::to_string_pretty(&job)?;
    text.push('\n');

    let tmp = file.with_file_name("job.json.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, file)?;

    Ok(())
}
