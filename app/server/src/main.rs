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

//! Serves the board on the port in `~/.jam-factory/config.json` until killed.

use std::env;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;

use jam_board_core::mcp_client::{MCP_BINARY, McpStore, find_mcp_binary};
use jam_board_core::server::Server;
use jam_board_core::server::repos::ConfigRepos;
use jam_factory_store::root::resolve_port;

fn main() -> ExitCode {
    let Some(home) = env::home_dir() else {
        eprintln!("Cannot find the home folder");
        return ExitCode::FAILURE;
    };

    let port = match resolve_port(&home) {
        Ok(port) => port,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let Some(program) = find_mcp_binary(&home, env::var_os("PATH").as_deref()) else {
        eprintln!(
            "Cannot find {MCP_BINARY} in ~/.local/bin or on PATH. Run `just install` in app/."
        );
        return ExitCode::FAILURE;
    };

    let store = match McpStore::spawn(program) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let server = match Server::start(Arc::new(store), Arc::new(ConfigRepos { home }), port) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("Cannot listen on 127.0.0.1:{port}: {error}");
            return ExitCode::FAILURE;
        }
    };

    eprintln!("Serving on http://127.0.0.1:{}", server.port());

    loop {
        thread::park();
    }
}
