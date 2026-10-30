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

//! HTTP/1.1 requests and responses, one per connection.

use std::io::{self, Read, Write};

const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const MAX_HEADERS: usize = 100;

pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first value of the header `name`, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Response {
        Response {
            status,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body,
        }
    }

    /// A plain-text response whose body is the status code and its reason.
    pub fn error(status: u16) -> Response {
        let body = format!("{status} {}", reason(status));
        Response::new(status, "text/plain; charset=utf-8", body.into_bytes())
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Writes the response to `out`, adding `Content-Length`, `Cache-Control` and `Connection`.
    pub(crate) fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        let mut head = format!("HTTP/1.1 {} {}\r\n", self.status, reason(self.status));
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        head.push_str("Cache-Control: no-store\r\n");
        head.push_str("Connection: close\r\n\r\n");

        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        out.write_all(&bytes)
    }
}

/// Why no request came out of a connection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadError {
    /// The client closed the connection, timed out or failed to send a full request.
    Closed,
    /// The request is invalid and gets this status.
    Status(u16),
}

/// Reads one request, head and body, from `stream`.
pub(crate) fn read_request(stream: &mut impl Read) -> Result<Request, ReadError> {
    let mut buf = Vec::new();
    let mut chunk = [0; 4096];

    let (mut request, head_len) = loop {
        let n = stream.read(&mut chunk).map_err(|_| ReadError::Closed)?;
        if n == 0 {
            return Err(ReadError::Closed);
        }
        buf.extend_from_slice(&chunk[..n]);

        if let Some(parsed) = parse_head(&buf)? {
            break parsed;
        }
        if buf.len() > MAX_HEAD {
            return Err(ReadError::Status(400));
        }
    };

    if head_len > MAX_HEAD {
        return Err(ReadError::Status(400));
    }

    let length = match request.header("Content-Length") {
        None => 0,
        Some(value) => value
            .trim()
            .parse::<usize>()
            .map_err(|_| ReadError::Status(400))?,
    };
    if length > MAX_BODY {
        return Err(ReadError::Status(413));
    }

    let mut body = buf.split_off(head_len);
    let received = body.len();
    if received < length {
        body.resize(length, 0);
        stream
            .read_exact(&mut body[received..])
            .map_err(|_| ReadError::Closed)?;
    }
    body.truncate(length);

    request.body = body;
    Ok(request)
}

/// Parses the request head in `buf`, giving the request and the head length once it is complete.
fn parse_head(buf: &[u8]) -> Result<Option<(Request, usize)>, ReadError> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut headers);

    let head_len = match parsed.parse(buf) {
        Ok(httparse::Status::Complete(len)) => len,
        Ok(httparse::Status::Partial) => return Ok(None),
        Err(_) => return Err(ReadError::Status(400)),
    };

    let headers = parsed
        .headers
        .iter()
        .map(|h| {
            let value = String::from_utf8_lossy(h.value).into_owned();
            (h.name.to_string(), value)
        })
        .collect();

    let request = Request {
        method: parsed.method.unwrap_or_default().to_string(),
        target: parsed.path.unwrap_or_default().to_string(),
        headers,
        body: Vec::new(),
    };

    Ok(Some((request, head_len)))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        301 => "Moved Permanently",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        428 => "Precondition Required",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bytes: &[u8]) -> Result<Request, ReadError> {
        read_request(&mut &bytes[..])
    }

    #[test]
    fn reads_head_and_body() {
        let request =
            read(b"POST /a?b HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\nhi").unwrap();

        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/a?b");
        assert_eq!(request.header("host"), Some("x"));
        assert_eq!(request.body, b"hi");
    }

    #[test]
    fn malformed_head_is_400() {
        let error = read(b"GET\x01/ HTTP/1.1\r\n\r\n").err();

        assert_eq!(error, Some(ReadError::Status(400)));
    }

    #[test]
    fn oversized_head_is_400() {
        let mut bytes = b"GET / HTTP/1.1\r\nX: ".to_vec();
        bytes.resize(MAX_HEAD + 1, b'a');

        assert_eq!(read(&bytes).err(), Some(ReadError::Status(400)));
    }

    #[test]
    fn oversized_body_is_413() {
        let head = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );

        assert_eq!(read(head.as_bytes()).err(), Some(ReadError::Status(413)));
    }

    #[test]
    fn bad_content_length_is_400() {
        let error = read(b"POST / HTTP/1.1\r\nContent-Length: x\r\n\r\n").err();

        assert_eq!(error, Some(ReadError::Status(400)));
    }

    #[test]
    fn truncated_request_is_closed() {
        assert_eq!(read(b"GET / HTTP/1.1\r\n").err(), Some(ReadError::Closed));
    }

    #[test]
    fn writes_the_fixed_headers() {
        let mut out = Vec::new();

        Response::error(404).write_to(&mut out).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "HTTP/1.1 404 Not Found\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Length: 13\r\n\
             Cache-Control: no-store\r\n\
             Connection: close\r\n\
             \r\n\
             404 Not Found"
        );
    }
}
