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

//! App-wide state built once at startup.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use odeck_core::config::{self, Config};
use odeck_core::env::{self, Env};
use odeck_core::terminal::{Tmux, TmuxHost};

const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AppState {
    pub env: Env,
    pub config: Config,
    pub config_error: Option<String>,
    pub github_src: Option<PathBuf>,
    pub host: Arc<TmuxHost>,
    pub tmux_available: bool,
}

impl AppState {
    /// Reads the login-shell env and the config file, and sets up the tmux host.
    pub fn load() -> Self {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let env = env::login_env(Path::new(&shell), LOGIN_SHELL_TIMEOUT).unwrap_or_else(|e| {
            eprintln!("octodeck: {e}; using the process env");
            std::env::vars().collect()
        });

        let home = env
            .get("HOME")
            .cloned()
            .or_else(|| std::env::var("HOME").ok())
            .unwrap_or_default();
        let (config, config_error) = match config::load(&config::default_path(Path::new(&home))) {
            Ok(config) => (config, None),
            Err(e) => (Config::default(), Some(e.to_string())),
        };

        let github_src = env::resolve_github_src(&env, &config);
        let tmux = Tmux {
            socket: None,
            env: env.clone(),
        };
        let tmux_available = tmux.is_available();

        Self {
            env,
            config,
            config_error,
            github_src,
            host: Arc::new(TmuxHost::new(tmux)),
            tmux_available,
        }
    }
}
