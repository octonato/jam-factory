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

//! Decides where the board window's navigations go.

use std::process::Command;
use std::thread;

use tauri::Url;

const DASHBOARD_HOST: &str = "127.0.0.1";
const DASHBOARD_PATH: &str = "/dashboard.html";

/// Where a navigation goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Navigation {
    Stay,
    Browser,
    Block,
}

/// Keeps the dashboard served on `port` in the window and sends other web URLs to the browser.
pub fn decide(url: &Url, port: u16) -> Navigation {
    if url.as_str() == "about:blank" {
        return Navigation::Stay;
    }

    let is_dashboard = url.scheme() == "http"
        && url.host_str() == Some(DASHBOARD_HOST)
        && url.port_or_known_default() == Some(port)
        && url.path() == DASHBOARD_PATH;
    if is_dashboard {
        return Navigation::Stay;
    }

    match url.scheme() {
        "http" | "https" | "mailto" => Navigation::Browser,
        _ => Navigation::Block,
    }
}

/// Opens `url` in the default browser.
pub fn open_in_browser(url: &Url) {
    let url = url.to_string();

    thread::spawn(move || {
        let failure = match Command::new("/usr/bin/open").arg(&url).status() {
            Ok(status) if status.success() => return,
            Ok(status) => status.to_string(),
            Err(e) => e.to_string(),
        };

        eprintln!("jam-board: cannot open {url}: {failure}");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide_str(url: &str) -> Navigation {
        decide(&tauri::Url::parse(url).unwrap(), 8731)
    }

    #[test]
    fn stays_on_the_dashboard() {
        assert_eq!(
            decide_str("http://127.0.0.1:8731/dashboard.html"),
            Navigation::Stay
        );
        assert_eq!(
            decide_str("http://127.0.0.1:8731/dashboard.html#/stack/a/spec.md"),
            Navigation::Stay
        );
        assert_eq!(decide_str("about:blank"), Navigation::Stay);
    }

    #[test]
    fn opens_other_web_urls_in_the_browser() {
        for url in [
            "https://github.com/o/r/issues/1",
            "http://127.0.0.1:8731/stack/sessions/a/spec.md",
            "http://127.0.0.1:9999/dashboard.html",
            "mailto:me@example.org",
        ] {
            assert_eq!(decide_str(url), Navigation::Browser, "{url}");
        }
    }

    #[test]
    fn blocks_other_schemes() {
        for url in ["file:///etc/passwd", "data:text/html,hi"] {
            assert_eq!(decide_str(url), Navigation::Block, "{url}");
        }
    }
}
