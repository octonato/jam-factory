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

//! A local HTTP server for one factory root and the embedded dashboard.

pub mod archive;
pub mod dashboard;
pub mod files;
pub mod http;
pub mod open;

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use http::{ReadError, Request, Response, read_request};

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(10);

/// A server on `127.0.0.1` that runs until dropped.
pub struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `root` on `127.0.0.1:port`, or on a free port if `port` is 0.
    pub fn start(root: PathBuf, port: u16) -> io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));

        let accept = {
            let stop = Arc::clone(&stop);
            let port = address.port();
            thread::Builder::new()
                .name(format!("odeck-server-{port}"))
                .spawn(move || accept_loop(listener, root.into(), port, &stop))?
        };

        Ok(Server {
            address,
            stop,
            accept: Some(accept),
        })
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);

        let woken = TcpStream::connect(self.address).is_ok();
        if let Some(accept) = self.accept.take()
            && woken
        {
            let _ = accept.join();
        }
    }
}

/// Answers one request for the factory root `root`, served on `port`.
pub fn handle(root: &Path, port: u16, request: &Request) -> Response {
    if !local_host(request, port) {
        return Response::error(403);
    }

    match request.method.as_str() {
        "GET" => files::get(root, &request.target),
        "POST" if request.target.starts_with("/archive/") => archive::post(root, request),
        "POST" if request.target.starts_with("/open/") => open::post(root, request),
        "POST" => Response::error(404),
        _ => Response::error(405),
    }
}

/// Whether the `Host` header is `127.0.0.1:<port>` or `localhost:<port>`.
fn local_host(request: &Request, port: u16) -> bool {
    request.header("Host").is_some_and(|host| {
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    })
}

fn accept_loop(listener: TcpListener, root: Arc<Path>, port: u16, stop: &AtomicBool) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }

        let Ok(stream) = stream else {
            thread::sleep(ACCEPT_BACKOFF);
            continue;
        };

        let root = Arc::clone(&root);
        let _ = thread::Builder::new().spawn(move || serve(&root, port, stream));
    }
}

fn serve(root: &Path, port: u16, mut stream: TcpStream) {
    if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return;
    }

    let response = match read_request(&mut stream) {
        Ok(request) => handle(root, port, &request),
        Err(ReadError::Status(status)) => Response::error(status),
        Err(ReadError::Closed) => return,
    };

    let _ = response.write_to(&mut stream);
}
