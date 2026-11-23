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

//! Writes and reads the untracked notes of a worktree in its `.nogit` folder.

use std::fs;
use std::path::Path;

/// The folder of a worktree that holds its untracked notes.
const NOTES_FOLDER: &str = ".nogit";

/// The note of a worktree that holds the title and the link of its issue.
pub const ISSUE_NOTE: &str = "issue.md";

/// The note of a worktree that holds the title and the link of its pull request.
pub const PR_NOTE: &str = "pr.md";

/// Writes `title` and `url` to the note `name` of the worktree `folder`.
pub fn write_note(folder: &Path, name: &str, title: &str, url: &str) -> Result<(), String> {
    let notes = folder.join(NOTES_FOLDER);
    let note = format!("# {title}\n\n{url}\n");

    fs::create_dir_all(&notes).map_err(|error| error.to_string())?;
    fs::write(notes.join(name), note).map_err(|error| error.to_string())
}

/// The first `https://` line of the note `name` of the worktree `folder`, or `None` when the note or the line is missing.
pub fn note_url(folder: &Path, name: &str) -> Option<String> {
    let note = fs::read_to_string(folder.join(NOTES_FOLDER).join(name)).ok()?;

    note.lines()
        .map(str::trim)
        .find(|line| line.starts_with("https://"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_note_gives_its_url() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://github.com/o/r/pull/12";

        write_note(dir.path(), PR_NOTE, "Fix it", url).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join(".nogit/pr.md")).unwrap(),
            format!("# Fix it\n\n{url}\n")
        );
        assert_eq!(note_url(dir.path(), PR_NOTE).as_deref(), Some(url));
    }

    #[test]
    fn a_missing_note_or_link_gives_no_url() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(note_url(dir.path(), ISSUE_NOTE), None);

        fs::create_dir(dir.path().join(".nogit")).unwrap();
        fs::write(dir.path().join(".nogit/issue.md"), "# No link\n").unwrap();

        assert_eq!(note_url(dir.path(), ISSUE_NOTE), None);
    }
}
