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

//! The shapes the commands send to the UI.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;

use odeck_core::checkouts::Checkout;
use odeck_core::project::LineError;
use odeck_core::terminal::{SessionTarget, TerminalEvent, TerminalKind};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub tmux_available: bool,
    pub github_src: Option<String>,
    pub config_error: Option<String>,
    pub font_size: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub name: String,
    pub root: String,
    pub errors: Vec<LineErrorDto>,
    pub repos: Vec<RepoDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineErrorDto {
    pub line: usize,
    pub text: String,
    pub reason: String,
}

impl From<&LineError> for LineErrorDto {
    fn from(e: &LineError) -> Self {
        Self {
            line: e.line,
            text: e.text.clone(),
            reason: e.reason.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDto {
    pub org: String,
    pub name: String,
    pub path: Option<String>,
    pub cloned: bool,
    pub error: Option<String>,
    pub checkouts: Vec<CheckoutDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutDto {
    pub path: String,
    pub dir_name: String,
    pub branch: Option<String>,
}

impl From<&Checkout> for CheckoutDto {
    fn from(c: &Checkout) -> Self {
        Self {
            path: c.path.display().to_string(),
            dir_name: c.dir_name.clone(),
            branch: c.branch.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedTerminal {
    pub id: u64,
    pub name: String,
    pub index: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub name: String,
    pub target: SessionTarget,
    pub kind: TerminalKind,
    pub index: u32,
    pub dir: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TerminalEventDto {
    Output { data: String },
    Exit { code: Option<u32> },
}

impl From<TerminalEvent> for TerminalEventDto {
    fn from(event: TerminalEvent) -> Self {
        match event {
            TerminalEvent::Output(bytes) => Self::Output {
                data: STANDARD.encode(bytes),
            },
            TerminalEvent::Exited(code) => Self::Exit { code },
        }
    }
}
