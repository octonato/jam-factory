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

//! The commands the UI invokes.

use std::path::{Path, PathBuf};

use tauri::ipc::Channel;
use tauri::{State, Window};

use odeck_core::checkouts::{self, Checkout, RepoCheckouts, SystemGit};
use odeck_core::config::Editor;
use odeck_core::launcher;
use odeck_core::project::{self, RepoRef};
use odeck_core::terminal::{
    AttachRequest, SessionHost, SessionTarget, TerminalKind, next_index, parse_session,
    session_name,
};

use crate::dto::{
    AppInfo, CheckoutDto, LineErrorDto, LiveSession, OpenedTerminal, ProjectDto, RepoDto,
    TerminalEventDto,
};
use crate::owners::TerminalOwners;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

struct LoadedRepo {
    repo: RepoRef,
    dto: RepoDto,
    checkouts: Vec<Checkout>,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        tmux_available: state.tmux_available,
        github_src: state.github_src.as_ref().map(|p| p.display().to_string()),
        config_error: state.config_error.clone(),
        font_size: state.config.font_size,
    }
}

#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, path: String) -> CmdResult<ProjectDto> {
    let project = project::load(Path::new(&path)).map_err(|e| e.to_string())?;
    let repos = load_repos(&state, &project.file.repos);

    Ok(ProjectDto {
        name: project.name,
        root: project.root.display().to_string(),
        errors: project.file.errors.iter().map(LineErrorDto::from).collect(),
        repos: repos.into_iter().map(|r| r.dto).collect(),
    })
}

#[tauri::command]
pub fn list_editors(state: State<'_, AppState>) -> Vec<Editor> {
    state.config.editors.clone()
}

#[tauri::command]
pub fn open_in_editor(
    state: State<'_, AppState>,
    editor: String,
    checkout: String,
) -> CmdResult<()> {
    let editor = state
        .config
        .editors
        .iter()
        .find(|e| e.name == editor)
        .ok_or_else(|| format!("no editor named {editor}"))?;

    launcher::open_in_editor(editor, Path::new(&checkout), &state.env).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn open_terminal(
    state: State<'_, AppState>,
    owners: State<'_, TerminalOwners>,
    window: Window,
    target: SessionTarget,
    dir: String,
    kind: TerminalKind,
    index: Option<u32>,
    cols: u16,
    rows: u16,
    events: Channel<TerminalEventDto>,
) -> CmdResult<OpenedTerminal> {
    let index = match index {
        Some(i) => i,
        None => {
            let live = state.host.live_sessions().map_err(|e| e.to_string())?;
            next_index(&target, kind, &live)
        }
    };
    let name = session_name(&target, kind, index);

    let program = match kind {
        TerminalKind::Shell => None,
        TerminalKind::Claude => Some("claude".to_string()),
    };
    let request = AttachRequest {
        name: name.clone(),
        dir: PathBuf::from(dir),
        program,
        cols,
        rows,
    };

    let id = state
        .host
        .attach(
            request,
            Box::new(move |event| {
                let _ = events.send(event.into());
            }),
        )
        .map_err(|e| e.to_string())?;
    owners.add(window.label(), id);

    Ok(OpenedTerminal { id, name, index })
}

#[tauri::command]
pub fn write_terminal(state: State<'_, AppState>, id: u64, data: String) -> CmdResult<()> {
    state
        .host
        .write(id, data.as_bytes())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resize_terminal(state: State<'_, AppState>, id: u64, cols: u16, rows: u16) -> CmdResult<()> {
    state.host.resize(id, cols, rows).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_terminal(
    state: State<'_, AppState>,
    owners: State<'_, TerminalOwners>,
    id: u64,
) -> CmdResult<()> {
    owners.remove(id);
    state.host.detach(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kill_session(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    state.host.kill(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_live_sessions(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<Vec<LiveSession>> {
    let project = project::load(Path::new(&path)).map_err(|e| e.to_string())?;
    let live = state.host.live_sessions().map_err(|e| e.to_string())?;
    let mut sessions = Vec::new();

    for loaded in load_repos(&state, &project.file.repos) {
        for checkout in &loaded.checkouts {
            let target = SessionTarget {
                org: loaded.repo.org.clone(),
                repo: loaded.repo.name.clone(),
                checkout: checkout.dir_name.clone(),
            };

            for name in &live {
                if let Some((kind, index)) = parse_session(name, &target) {
                    sessions.push(LiveSession {
                        name: name.clone(),
                        target: target.clone(),
                        kind,
                        index,
                        dir: checkout.path.display().to_string(),
                    });
                }
            }
        }
    }

    Ok(sessions)
}

fn load_repos(state: &AppState, repos: &[RepoRef]) -> Vec<LoadedRepo> {
    let git = SystemGit {
        env: state.env.clone(),
    };

    repos
        .iter()
        .map(|repo| {
            let mut dto = RepoDto {
                org: repo.org.clone(),
                name: repo.name.clone(),
                path: None,
                cloned: false,
                error: None,
                checkouts: Vec::new(),
            };
            let mut found = Vec::new();

            if let Some(src) = &state.github_src {
                let path = repo.path(src);
                dto.path = Some(path.display().to_string());

                match checkouts::list(&path, &git) {
                    Ok(RepoCheckouts::NotCloned) => {}
                    Ok(RepoCheckouts::Checkouts(list)) => {
                        dto.cloned = true;
                        dto.checkouts = list.iter().map(CheckoutDto::from).collect();
                        found = list;
                    }
                    Err(e) => {
                        dto.cloned = true;
                        dto.error = Some(e.to_string());
                    }
                }
            }

            LoadedRepo {
                repo: repo.clone(),
                dto,
                checkouts: found,
            }
        })
        .collect()
}
