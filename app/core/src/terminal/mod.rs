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

//! Terminals backed by tmux sessions.

mod host;
mod naming;
mod tmux;

pub use host::{
    AttachRequest, SessionHost, Sink, TerminalError, TerminalEvent, TerminalId, TmuxHost,
};
pub use naming::{
    SessionTarget, TerminalKind, next_index, parse_session, session_name, session_prefix,
};
pub use tmux::{Tmux, TmuxError};
