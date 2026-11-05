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

//! Serves the factory folder over MCP on stdio, or exports a session with `export`.

mod excerpt;
mod export;
mod tools;

use std::path::Path;
use std::process::ExitCode;

use jam_factory_store::{Store, root};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

use crate::tools::FactoryServer;

const EXPORT: &str = "export";

#[tokio::main]
async fn main() -> ExitCode {
    let Some(home) = std::env::home_dir() else {
        eprintln!("Cannot find the home folder");
        return ExitCode::FAILURE;
    };

    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some((command, rest)) = args.split_first()
        && command == EXPORT
    {
        return run_export(rest, &home);
    }

    let root = match root::resolve(&home) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let store = Store::new(root);

    match store.migrate() {
        Ok(moved) => {
            for m in moved {
                eprintln!("Moved the session {} to {}", m.from, m.to);
            }
        }
        Err(error) => eprintln!("Cannot move the old sessions: {error}"),
    }

    let server = FactoryServer::new(store);

    let service = match server.serve(stdio()).await {
        Ok(service) => service,
        Err(error) => {
            eprintln!("Cannot start the MCP server: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = service.waiting().await {
        eprintln!("The MCP server stopped: {error}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// Runs `export` with `args` in the current folder, and prints its result or its error.
fn run_export(args: &[String], home: &Path) -> ExitCode {
    let result = std::env::current_dir()
        .map_err(|e| format!("Cannot read the current folder: {e}"))
        .and_then(|cwd| export::run(args, home, &cwd));

    match result {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }

        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
