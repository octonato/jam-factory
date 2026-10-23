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

//! Serves the embedded dashboard, and the files and folder listings under a factory root.

use std::fs;
use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

use super::dashboard::embedded;
use super::http::Response;

const HTML: &str = "text/html; charset=utf-8";

/// The characters a listing `href` percent-encodes.
const HREF: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Answers `GET target` for the factory root `root`.
pub fn get(root: &Path, target: &str) -> Response {
    let raw = target.split(['?', '#']).next().unwrap_or_default();
    let Ok(path) = percent_decode_str(raw).decode_utf8() else {
        return Response::error(404);
    };

    if let Some(file) = embedded(&path) {
        return Response::new(200, file.content_type, file.bytes.to_vec());
    }

    let Some(local) = resolve(root, &path) else {
        return Response::error(404);
    };

    let trailing_slash = path.ends_with('/');

    if local.is_dir() && trailing_slash {
        listing(&local, &path)
    } else if local.is_dir() {
        Response::new(301, HTML, Vec::new()).with_header("Location", &format!("{raw}/"))
    } else if local.is_file() && !trailing_slash {
        match fs::read(&local) {
            Ok(bytes) => Response::new(200, content_type(&local), bytes),
            Err(_) => Response::error(404),
        }
    } else {
        Response::error(404)
    }
}

/// The path under `root` for the decoded request path `path`, or `None` if a segment is unsafe.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let mut local = root.to_path_buf();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => return None,
            _ if segment.contains(['\\', '\0']) => return None,
            _ => local.push(segment),
        }
    }

    Some(local)
}

/// An HTML page linking every entry of `folder`, titled with the request path `path`.
fn listing(folder: &Path, path: &str) -> Response {
    let Ok(entries) = fs::read_dir(folder) else {
        return Response::error(404);
    };

    let mut entries: Vec<(String, bool)> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, entry.path().is_dir())
        })
        .collect();
    entries.sort_by_cached_key(|(name, _)| name.to_lowercase());

    let title = format!("Directory listing for {}", escape(path));
    let mut page = format!(
        "<!DOCTYPE HTML>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <title>{title}</title>\n\
         </head>\n\
         <body>\n\
         <h1>{title}</h1>\n\
         <hr>\n\
         <ul>\n"
    );

    for (name, is_dir) in &entries {
        let slash = if *is_dir { "/" } else { "" };
        let href = utf8_percent_encode(name, HREF);
        page.push_str(&format!(
            "<li><a href=\"{href}{slash}\">{}{slash}</a></li>\n",
            escape(name)
        ));
    }

    page.push_str("</ul>\n<hr>\n</body>\n</html>\n");

    Response::new(200, HTML, page.into_bytes())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn content_type(file: &Path) -> &'static str {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();

    match extension.to_ascii_lowercase().as_str() {
        "html" => HTML,
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "jsonl" => "application/jsonl",
        "md" => "text/markdown; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(response: Response) -> String {
        String::from_utf8(response.body).unwrap()
    }

    #[test]
    fn listing_sorts_by_lowercase_name_and_keeps_hidden_entries() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.md", ".hidden", "A.md"] {
            fs::write(dir.path().join(name), "").unwrap();
        }

        let page = text(get(dir.path(), "/"));

        let hidden = page.find(r#"<a href=".hidden">"#).unwrap();
        let a = page.find(r#"<a href="A.md">"#).unwrap();
        let b = page.find(r#"<a href="b.md">"#).unwrap();
        assert!(hidden < a && a < b, "{page}");
    }

    #[test]
    fn listing_escapes_names_and_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("a<b")).unwrap();
        fs::write(dir.path().join("a<b").join("x&y"), "").unwrap();

        let page = text(get(dir.path(), "/a%3Cb/"));

        assert!(
            page.contains("<title>Directory listing for /a&lt;b/</title>"),
            "{page}"
        );
        assert!(
            page.contains(r#"<li><a href="x%26y">x&amp;y</a></li>"#),
            "{page}"
        );
    }

    #[test]
    fn content_types_follow_the_extension() {
        for (name, expected) in [
            ("a.html", "text/html; charset=utf-8"),
            ("a.js", "text/javascript; charset=utf-8"),
            ("a.json", "application/json"),
            ("runs.jsonl", "application/jsonl"),
            ("a.md", "text/markdown; charset=utf-8"),
            ("a.css", "text/css; charset=utf-8"),
            ("a.txt", "text/plain; charset=utf-8"),
            ("a.png", "application/octet-stream"),
            ("a", "application/octet-stream"),
        ] {
            assert_eq!(content_type(Path::new(name)), expected, "{name}");
        }
    }
}
