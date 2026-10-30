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

//! A local HTTP server for the factory store and the embedded dashboard.

pub mod api;
pub mod dashboard;
pub mod http;
pub mod open;

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dashboard::embedded;
use http::{ReadError, Request, Response, read_request};

use crate::store::Store;

const READ_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(10);

/// A server on `127.0.0.1` that runs until dropped.
pub struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `store` on `127.0.0.1:port`, or on a free port if `port` is 0.
    pub fn start(store: Arc<dyn Store>, port: u16) -> io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));

        let accept = {
            let stop = Arc::clone(&stop);
            let port = address.port();
            thread::Builder::new()
                .name(format!("jam-board-server-{port}"))
                .spawn(move || accept_loop(listener, store, port, &stop))?
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

/// Answers one request with `store`, served on `port`.
pub fn handle(store: &dyn Store, port: u16, request: &Request) -> Response {
    if !local_host(request, port) {
        return Response::error(403);
    }

    match request.method.as_str() {
        "GET" if request.target.starts_with("/api/") => api::get(store, &request.target),
        "GET" => dashboard_file(&request.target),
        "POST" => api::post(store, request),
        "PUT" => api::put(store, request),
        _ => Response::error(405),
    }
}

/// Whether the `Host` header is `127.0.0.1:<port>` or `localhost:<port>`.
fn local_host(request: &Request, port: u16) -> bool {
    request.header("Host").is_some_and(|host| {
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    })
}

/// The embedded dashboard file at the path in `target`, or `404`.
fn dashboard_file(target: &str) -> Response {
    let path = target.split(['?', '#']).next().unwrap_or_default();

    match embedded(path) {
        Some(file) => Response::new(200, file.content_type, file.bytes.to_vec()),
        None => Response::error(404),
    }
}

fn accept_loop(listener: TcpListener, store: Arc<dyn Store>, port: u16, stop: &AtomicBool) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            break;
        }

        let Ok(stream) = stream else {
            thread::sleep(ACCEPT_BACKOFF);
            continue;
        };

        let store = Arc::clone(&store);
        let _ = thread::Builder::new().spawn(move || serve(store.as_ref(), port, stream));
    }
}

fn serve(store: &dyn Store, port: u16, mut stream: TcpStream) {
    if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err() {
        return;
    }

    let response = match read_request(&mut stream) {
        Ok(request) => handle(store, port, &request),
        Err(ReadError::Status(status)) => Response::error(status),
        Err(ReadError::Closed) => return,
    };

    let _ = response.write_to(&mut stream);
}
