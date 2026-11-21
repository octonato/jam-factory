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

/// Host patterns that always stay in the GitHub view.
const GITHUB_HOSTS: [&str; 4] = [
    "github.com",
    "*.github.com",
    "*.githubusercontent.com",
    "*.githubassets.com",
];

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

/// Keeps GitHub and the sign-in hosts in the GitHub view and sends other web URLs to the browser.
pub fn decide_github(url: &Url, sign_in_hosts: &[String]) -> Navigation {
    if matches!(url.as_str(), "about:blank" | "about:srcdoc") {
        return Navigation::Stay;
    }

    match url.scheme() {
        "http" | "https" => {
            let mut patterns = GITHUB_HOSTS
                .into_iter()
                .chain(sign_in_hosts.iter().map(String::as_str));

            let stays = url
                .host_str()
                .is_some_and(|host| patterns.any(|pattern| host_matches(host, pattern)));

            if stays {
                Navigation::Stay
            } else {
                Navigation::Browser
            }
        }
        "mailto" => Navigation::Browser,
        _ => Navigation::Block,
    }
}

/// Tells whether `host` matches `pattern`, a plain host or `*.` plus a domain, ignoring case.
pub fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.to_ascii_lowercase();

    match pattern.strip_prefix("*.") {
        Some(domain) => host.ends_with(&format!(".{domain}")),
        None => host == pattern,
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

    fn decide_github_str(url: &str, sign_in_hosts: &[&str]) -> Navigation {
        let sign_in_hosts: Vec<String> = sign_in_hosts.iter().map(|h| h.to_string()).collect();
        decide_github(&tauri::Url::parse(url).unwrap(), &sign_in_hosts)
    }

    #[test]
    fn the_github_view_keeps_github() {
        for url in [
            "https://github.com/o/r/pull/1",
            "https://gist.github.com/x",
            "https://viewscreen.githubusercontent.com/x",
            "https://github.githubassets.com/a.js",
            "http://github.com/",
            "HTTPS://GITHUB.COM/o",
        ] {
            assert_eq!(decide_github_str(url, &[]), Navigation::Stay, "{url}");
        }
    }

    #[test]
    fn the_github_view_keeps_sign_in_hosts() {
        let sign_in_hosts = ["*.okta.com", "sso.example.org"];

        for url in [
            "https://akka.okta.com/app",
            "https://a.b.okta.com/",
            "https://SSO.example.org/",
        ] {
            assert_eq!(
                decide_github_str(url, &sign_in_hosts),
                Navigation::Stay,
                "{url}"
            );
        }

        for url in [
            "https://okta.com/",
            "https://evilokta.com/",
            "https://okta.com.evil.org/",
        ] {
            assert_eq!(
                decide_github_str(url, &sign_in_hosts),
                Navigation::Browser,
                "{url}"
            );
        }
    }

    #[test]
    fn the_github_view_keeps_blank_frames() {
        for url in ["about:blank", "about:srcdoc"] {
            assert_eq!(decide_github_str(url, &[]), Navigation::Stay, "{url}");
        }
    }

    #[test]
    fn the_github_view_sends_other_links_to_the_browser() {
        for url in [
            "https://example.org/",
            "http://127.0.0.1:8731/dashboard.html",
            "mailto:me@example.org",
            "https://notgithub.com/",
        ] {
            assert_eq!(decide_github_str(url, &[]), Navigation::Browser, "{url}");
        }
    }

    #[test]
    fn the_github_view_blocks_other_schemes() {
        for url in [
            "file:///etc/passwd",
            "data:text/html,hi",
            "jam-board://open?path=%2Fa",
            "javascript:alert(1)",
        ] {
            assert_eq!(decide_github_str(url, &[]), Navigation::Block, "{url}");
        }
    }

    #[test]
    fn host_patterns_ignore_case() {
        assert!(host_matches("Akka.Okta.com", "*.OKTA.com"));
        assert!(host_matches("SSO.example.org", "sso.EXAMPLE.org"));
        assert!(!host_matches("okta.com", "*.okta.com"));
        assert!(!host_matches("evilokta.com", "*.okta.com"));
    }
}
