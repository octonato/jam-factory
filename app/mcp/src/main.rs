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

//! Serves the factory folder over MCP on stdio.

mod tools;

use std::process::ExitCode;

use jam_factory_store::{Store, root};
use rmcp::ServiceExt;
use rmcp::transport::stdio;

use crate::tools::FactoryServer;

#[tokio::main]
async fn main() -> ExitCode {
    let Some(home) = std::env::home_dir() else {
        eprintln!("Cannot find the home folder");
        return ExitCode::FAILURE;
    };

    let root = match root::resolve(&home) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let server = FactoryServer::new(Store::new(root));

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
