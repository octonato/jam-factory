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

//! Checks that the Jam Factory server answers on a port.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_millis(500);

const INTERVAL: Duration = Duration::from_millis(200);

/// Returns true when `GET /dashboard.html` on `127.0.0.1:{port}` answers `200`.
pub fn answers(port: u16) -> bool {
    dashboard_status(port) == Some(200)
}

/// Calls `answers` every 200 ms until it returns true or `deadline` passes.
pub fn wait_for(port: u16, deadline: Duration) -> bool {
    let end = Instant::now() + deadline;

    loop {
        if answers(port) {
            return true;
        }

        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }

        thread::sleep(INTERVAL.min(left));
    }
}

/// The status code of `GET /dashboard.html` on `127.0.0.1:{port}`, or `None` when nothing answers.
fn dashboard_status(port: u16) -> Option<u16> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;

    let request = format!(
        "GET /dashboard.html HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;

    let mut status_line = String::new();
    BufReader::new(stream).read_line(&mut status_line).ok()?;

    status_line.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::mpsc;

    use super::*;

    const QUICK: Duration = Duration::from_secs(2);

    /// A port on `127.0.0.1` that nothing listens on.
    fn closed_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();

        listener.local_addr().unwrap().port()
    }

    /// Answers one request on a free port with `reply`, and returns the port.
    fn serve_once(reply: &'static str) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&stream);
            stream.write_all(reply.as_bytes()).unwrap();
        });

        port
    }

    /// Reads the request line and headers up to the blank line.
    fn read_request(stream: &TcpStream) {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        while reader.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
            line.clear();
        }
    }

    #[test]
    fn answers_a_200() {
        let port = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");

        assert!(answers(port));
    }

    #[test]
    fn a_404_is_no_answer() {
        let port = serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");

        assert!(!answers(port));
    }

    #[test]
    fn a_closed_port_is_no_answer() {
        assert!(!answers(closed_port()));
    }

    #[test]
    fn a_silent_server_is_no_answer() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (release, held) = mpsc::channel::<()>();

        thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            let _ = held.recv();
        });

        let start = Instant::now();
        let answered = answers(port);
        let took = start.elapsed();
        drop(release);

        assert!(!answered);
        assert!(took < QUICK, "{took:?}");
    }

    #[test]
    fn wait_for_retries_until_the_server_answers() {
        let port = closed_port();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));

            let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });

        assert!(wait_for(port, Duration::from_secs(2)));
    }

    #[test]
    fn wait_for_gives_up_at_the_deadline() {
        let port = closed_port();
        let deadline = Duration::from_millis(600);

        let start = Instant::now();
        let answered = wait_for(port, deadline);
        let took = start.elapsed();

        assert!(!answered);
        assert!(took >= deadline, "{took:?}");
        assert!(took < QUICK, "{took:?}");
    }
}
