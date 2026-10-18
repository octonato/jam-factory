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

//! The dashboard files embedded in the app.

const HTML: &str = "text/html; charset=utf-8";
const JAVASCRIPT: &str = "text/javascript; charset=utf-8";

/// An embedded file and its content type.
pub struct Embedded {
    pub bytes: &'static [u8],
    pub content_type: &'static str,
}

/// The embedded file served at the request path `path`, if any.
pub fn embedded(path: &str) -> Option<Embedded> {
    let (bytes, content_type): (&'static [u8], _) = match path {
        "/dashboard.html" => (include_bytes!("../../dashboard/dashboard.html"), HTML),
        "/marked.min.js" => (include_bytes!("../../dashboard/marked.min.js"), JAVASCRIPT),
        "/purify.min.js" => (include_bytes!("../../dashboard/purify.min.js"), JAVASCRIPT),
        _ => return None,
    };

    Some(Embedded {
        bytes,
        content_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_uses_the_in_page_dialog() {
        let html = std::str::from_utf8(embedded("/dashboard.html").unwrap().bytes).unwrap();

        assert!(!html.contains("confirm("));
        assert!(!html.contains("alert("));

        let dialog = html.find("<dialog id=\"modal\"").unwrap();
        let main_end = html.find("</main>").unwrap();
        assert!(dialog > main_end);
    }

    #[test]
    fn dashboard_is_titled_jam_board() {
        let html = std::str::from_utf8(embedded("/dashboard.html").unwrap().bytes).unwrap();

        assert!(html.contains("<title>Jam Board</title>"));
        assert!(html.contains("<h1>Jam Board</h1>"));
        assert!(!html.contains("Octo Factory"));
    }

    #[test]
    fn dashboard_has_no_file_tree() {
        let html = std::str::from_utf8(embedded("/dashboard.html").unwrap().bytes).unwrap();

        assert!(!html.contains("id=\"tree\""));
        assert!(!html.contains("flow-button"));
    }
}
