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

//! Reads git diffs and file lines from a worktree, and wraps them in Markdown fences.

use std::fmt;
use std::fs;
use std::path::Path;
use std::process::Command;

use jam_factory_store::StoreError;

const FILE_START: &str = "diff --git ";
const HUNK_START: &str = "@@ ";
const RENAME_LINES: [&str; 2] = ["rename from ", "rename to "];
const MIN_TICKS: usize = 3;

/// An excerpt failure, with the code the MCP tools report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcerptError {
    /// A store call failed.
    Store(StoreError),

    /// git failed, or the file is missing.
    GitFailed(String),

    /// The diff holds no change to show.
    EmptyDiff(String),

    /// The line range is not inside the file.
    BadRange(String),

    /// The repo path is empty, absolute, or has a `.` or `..` segment.
    BadPath(String),
}

impl ExcerptError {
    pub fn code(&self) -> &'static str {
        match self {
            ExcerptError::Store(error) => error.code(),
            ExcerptError::GitFailed(_) => "git_failed",
            ExcerptError::EmptyDiff(_) => "empty_diff",
            ExcerptError::BadRange(_) => "bad_range",
            ExcerptError::BadPath(_) => "bad_path",
        }
    }
}

impl fmt::Display for ExcerptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExcerptError::Store(error) => write!(f, "{error}"),
            ExcerptError::GitFailed(message)
            | ExcerptError::EmptyDiff(message)
            | ExcerptError::BadRange(message)
            | ExcerptError::BadPath(message) => f.write_str(message),
        }
    }
}

impl From<StoreError> for ExcerptError {
    fn from(error: StoreError) -> ExcerptError {
        ExcerptError::Store(error)
    }
}

/// Returns `body` in a fenced block with `info`, its fence longer than any backtick run in `body`.
pub fn fence(info: &str, body: &str) -> String {
    let longest_run = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let ticks = "`".repeat((longest_run + 1).max(MIN_TICKS));
    let newline = if body.ends_with('\n') { "" } else { "\n" };

    format!("\n{ticks}{info}\n{body}{newline}{ticks}\n")
}

/// Returns each file section of `diff` with its header and only the hunks whose new side meets `ranges`.
/// Leaves out a section with no such hunk.
pub fn filter_hunks(diff: &str, ranges: &[(u32, u32)]) -> String {
    let mut kept = String::new();

    for section in split_before(diff, FILE_START) {
        let parts = split_before(section, HUNK_START);

        let (header, hunks) = match parts.split_first() {
            Some((first, rest)) if !first.starts_with(HUNK_START) => (*first, rest),
            _ => ("", parts.as_slice()),
        };

        let meeting: Vec<&str> = hunks
            .iter()
            .copied()
            .filter(|hunk| new_range(hunk).is_some_and(|range| meets(range, ranges)))
            .collect();

        if !meeting.is_empty() {
            kept.push_str(header);
            meeting.iter().for_each(|hunk| kept.push_str(hunk));
        }
    }

    kept
}

/// Returns the highlight.js name of the language of `file`, by its extension, or `text`.
pub fn language(file: &str) -> &'static str {
    let extension = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match extension {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "cjs" | "mjs" => "javascript",
        "json" => "json",
        "md" => "markdown",
        "py" => "python",
        "java" => "java",
        "scala" | "sbt" => "scala",
        "kt" => "kotlin",
        "go" => "go",
        "rb" => "ruby",
        "sh" | "bash" | "zsh" => "bash",
        "toml" => "ini",
        "yaml" | "yml" => "yaml",
        "html" | "xml" => "xml",
        "css" => "css",
        "sql" => "sql",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "swift" => "swift",
        _ => "text",
    }
}

/// Returns `bad_path` unless `file` is a non-empty relative path without `.` or `..` segments.
pub fn check_repo_path(file: &str) -> Result<(), ExcerptError> {
    let safe = !file.is_empty()
        && !Path::new(file).is_absolute()
        && file
            .split('/')
            .all(|segment| !matches!(segment, "." | ".."));

    if safe {
        Ok(())
    } else {
        Err(ExcerptError::BadPath(format!(
            "{file:?} is not a relative path inside the worktree"
        )))
    }
}

/// Returns the file sections of the git diff from `base` to `head` in `worktree` whose old or new path is in `files`.
pub fn git_diff(
    worktree: &Path,
    base: &str,
    head: &str,
    files: &[String],
) -> Result<String, ExcerptError> {
    let args = [
        "diff",
        "--no-color",
        "--no-ext-diff",
        "-M",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--end-of-options",
        base,
        head,
        "--",
    ];

    let output = git(worktree, &args)?;

    let diff: String = split_before(&output, FILE_START)
        .into_iter()
        .filter(|section| {
            section_paths(section)
                .iter()
                .any(|path| files.contains(path))
        })
        .collect();

    if diff.is_empty() {
        return Err(ExcerptError::EmptyDiff(format!(
            "git diff {base} {head} shows no change in {}",
            files.join(", ")
        )));
    }

    Ok(diff)
}

/// Returns lines `start` to `end` of `file`, 1-based and each with its line ending.
/// Reads the file in `worktree`, or at `git_ref` when set.
pub fn code_lines(
    worktree: &Path,
    file: &str,
    start: u32,
    end: u32,
    git_ref: Option<&str>,
) -> Result<String, ExcerptError> {
    let text = match git_ref {
        Some(git_ref) => git(
            worktree,
            &["show", "--end-of-options", &format!("{git_ref}:{file}")],
        )?,

        None => fs::read(worktree.join(file))
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|e| ExcerptError::GitFailed(format!("Cannot read {file}: {e}")))?,
    };

    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let count = lines.len();

    if start < 1 || end < start || end as usize > count {
        return Err(ExcerptError::BadRange(format!(
            "Lines {start}-{end} are not in {file}, which has {count} lines"
        )));
    }

    Ok(lines[start as usize - 1..end as usize].concat())
}

/// Splits `text` before each line that starts with `prefix`.
fn split_before<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut offset = 0;

    for line in text.split_inclusive('\n') {
        if line.starts_with(prefix) && offset > start {
            parts.push(&text[start..offset]);
            start = offset;
        }

        offset += line.len();
    }

    if offset > start {
        parts.push(&text[start..]);
    }

    parts
}

/// Returns the old and new paths of a renamed file's diff section, or the one path of any other section.
fn section_paths(section: &str) -> Vec<String> {
    let header = section
        .lines()
        .take_while(|line| !line.starts_with(HUNK_START));

    let renamed: Vec<String> = header
        .filter_map(|line| {
            RENAME_LINES
                .iter()
                .find_map(|start| line.strip_prefix(start))
        })
        .map(unquote)
        .collect();

    if !renamed.is_empty() {
        return renamed;
    }

    section
        .lines()
        .next()
        .and_then(|line| line.strip_prefix(FILE_START))
        .and_then(same_path)
        .into_iter()
        .collect()
}

/// Returns the path of the names `a/{path} b/{path}` on a `diff --git` line.
fn same_path(names: &str) -> Option<String> {
    let middle = names.len() / 2;

    let old = unquote(names.get(..middle)?);
    let new = unquote(names.get(middle..)?.strip_prefix(' ')?);

    let path = old.strip_prefix("a/")?;

    (new.strip_prefix("b/") == Some(path)).then(|| path.to_string())
}

/// Returns `name` without the C-style quotes and escapes git puts around unusual paths.
fn unquote(name: &str) -> String {
    let Some(quoted) = name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) else {
        return name.to_string();
    };

    let mut bytes = Vec::with_capacity(quoted.len());
    let mut rest = quoted.as_bytes();

    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;

        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }

        let Some((&escaped, tail)) = rest.split_first() else {
            break;
        };
        rest = tail;

        let octal = rest.len() >= 2 && rest[..2].iter().all(|d| (b'0'..=b'7').contains(d));

        let unescaped = match escaped {
            b'a' => 0x07,
            b'b' => 0x08,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'r' => b'\r',
            b'0'..=b'3' if octal => {
                let (digits, tail) = rest.split_at(2);
                rest = tail;
                digits
                    .iter()
                    .fold(escaped - b'0', |value, digit| value * 8 + (digit - b'0'))
            }
            other => other,
        };

        bytes.push(unescaped);
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

/// Returns the new-side line range of the hunk that starts with `@@ -a[,b] +c[,d] @@`.
fn new_range(hunk: &str) -> Option<(u32, u32)> {
    let new_side = hunk
        .strip_prefix(HUNK_START)?
        .split(' ')
        .find_map(|word| word.strip_prefix('+'))?;

    let (start, count) = match new_side.split_once(',') {
        Some((start, count)) => (start.parse::<u32>().ok()?, count.parse::<u32>().ok()?),
        None => (new_side.parse::<u32>().ok()?, 1),
    };

    Some((start, start.saturating_add(count.max(1) - 1)))
}

/// Returns whether `range` meets one of `ranges`.
fn meets((start, end): (u32, u32), ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|&(from, to)| start <= to && from <= end)
}

/// Runs git with `args` in `worktree` and returns its stdout, or `git_failed` with git's message.
fn git(worktree: &Path, args: &[&str]) -> Result<String, ExcerptError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(args)
        .output()
        .map_err(|e| ExcerptError::GitFailed(format!("Cannot run git: {e}")))?;

    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        return Err(ExcerptError::GitFailed(format!(
            "git {} failed: {}",
            args.join(" "),
            reason.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    const HEADER_A: &str =
        "diff --git a/a.txt b/a.txt\nindex 1111111..2222222 100644\n--- a/a.txt\n+++ b/a.txt\n";
    const HUNK_A1: &str = "@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";
    const HUNK_A2: &str = "@@ -40,3 +40,3 @@ fn x\n x\n-y\n+Y\n z\n";

    const HEADER_B: &str =
        "diff --git a/b.txt b/b.txt\nindex 3333333..4444444 100644\n--- a/b.txt\n+++ b/b.txt\n";
    const HUNK_B: &str = "@@ -10,2 +10,3 @@\n p\n+q\n r\n";

    const BINARY: &str =
        "diff --git a/x b/x\nindex 5555555..6666666 100644\nBinary files a/x and b/x differ\n";

    /// Returns the diff of `a.txt`, with two hunks, then of `b.txt`, with one.
    fn two_files() -> String {
        [HEADER_A, HUNK_A1, HUNK_A2, HEADER_B, HUNK_B].concat()
    }

    /// Runs git with `args` in `dir`, and panics when it fails.
    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();

        assert!(output.status.success(), "git {args:?}: {output:?}");
    }

    /// Writes `text` to `f.txt` in `dir` and commits it.
    fn commit(dir: &Path, text: &str) {
        fs::write(dir.join("f.txt"), text).unwrap();
        git(dir, &["add", "f.txt"]);
        git(dir, &["commit", "-q", "-m", "change"]);
    }

    /// Returns a git repo where `f.txt` holds `old` at `HEAD~1` and `new` at `HEAD`.
    fn repo(old: &str, new: &str) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        commit(dir.path(), old);
        commit(dir.path(), new);
        dir
    }

    fn files() -> Vec<String> {
        vec!["f.txt".to_string()]
    }

    #[test]
    fn errors_carry_their_codes() {
        let errors = [
            (ExcerptError::Store(StoreError::NoMatch), "no_match"),
            (ExcerptError::GitFailed("x".to_string()), "git_failed"),
            (ExcerptError::EmptyDiff("x".to_string()), "empty_diff"),
            (ExcerptError::BadRange("x".to_string()), "bad_range"),
            (ExcerptError::BadPath("x".to_string()), "bad_path"),
        ];

        for (error, code) in errors {
            assert_eq!(error.code(), code);
        }

        let from_store = ExcerptError::from(StoreError::BadJson("broken".to_string()));

        assert_eq!(from_store.code(), "bad_json");
        assert_eq!(from_store.to_string(), "broken");
    }

    #[test]
    fn fence_uses_three_ticks_by_default() {
        assert_eq!(fence("diff", "a\n"), "\n```diff\na\n```\n");
    }

    #[test]
    fn fence_outgrows_backtick_runs() {
        assert_eq!(fence("md", "x ```` y\n"), "\n`````md\nx ```` y\n`````\n");
    }

    #[test]
    fn fence_adds_a_missing_final_newline() {
        assert_eq!(fence("text", "a"), "\n```text\na\n```\n");
    }

    #[test]
    fn filter_keeps_hunks_that_meet_a_range() {
        let diff = [HEADER_A, HUNK_A1, HUNK_A2].concat();

        assert_eq!(
            filter_hunks(&diff, &[(41, 41)]),
            [HEADER_A, HUNK_A2].concat()
        );
    }

    #[test]
    fn filter_drops_files_without_a_kept_hunk() {
        assert_eq!(
            filter_hunks(&two_files(), &[(11, 11)]),
            [HEADER_B, HUNK_B].concat()
        );
    }

    #[test]
    fn filter_reads_counts_that_default_to_one() {
        let diff = [HEADER_A, "@@ -3 +3 @@\n-c\n+C\n"].concat();

        assert_eq!(filter_hunks(&diff, &[(3, 3)]), diff);
        assert_eq!(filter_hunks(&diff, &[(4, 4)]), "");
    }

    #[test]
    fn filter_reads_a_zero_count_as_its_start_line() {
        let diff = [HEADER_A, "@@ -5,2 +4,0 @@\n-e\n-f\n"].concat();

        assert_eq!(filter_hunks(&diff, &[(4, 4)]), diff);
        assert_eq!(filter_hunks(&diff, &[(5, 5)]), "");
    }

    #[test]
    fn filter_drops_a_binary_section() {
        let diff = [BINARY, HEADER_A, HUNK_A1].concat();

        assert_eq!(
            filter_hunks(&diff, &[(1, 1000)]),
            [HEADER_A, HUNK_A1].concat()
        );
        assert_eq!(filter_hunks(BINARY, &[(1, 1000)]), "");
    }

    #[test]
    fn filter_returns_empty_when_nothing_meets() {
        assert_eq!(filter_hunks(&two_files(), &[(100, 200)]), "");
    }

    #[test]
    fn filter_keeps_bytes_exactly() {
        let kept = [
            "diff --git a/c.txt b/c.txt\nindex 7777777..8888888 100644\n--- a/c.txt\n+++ b/c.txt\n",
            "@@ -1,2 +1,2 @@\n a\r\n-b\n\\ No newline at end of file\n+B\r\n",
        ]
        .concat();
        let diff = [kept.as_str(), HEADER_B, HUNK_B].concat();

        assert_eq!(filter_hunks(&diff, &[(2, 2)]), kept);
    }

    #[test]
    fn language_maps_known_extensions_and_falls_back() {
        assert_eq!(language("a/b.rs"), "rust");
        assert_eq!(language("x.tsx"), "typescript");
        assert_eq!(language("build.sbt"), "scala");
        assert_eq!(language("Cargo.toml"), "ini");
        assert_eq!(language("page.html"), "xml");
        assert_eq!(language("Makefile"), "text");
        assert_eq!(language("notes.unknown"), "text");
    }

    #[test]
    fn repo_paths_refuse_escapes() {
        for file in ["", "/etc/passwd", "../x", "a/../b", "./a", "a/./b"] {
            let error = check_repo_path(file).unwrap_err();

            assert_eq!(error.code(), "bad_path", "{file:?}");
        }

        assert!(check_repo_path("a/b.rs").is_ok());
        assert!(check_repo_path(".github/workflows/ci.yml").is_ok());
    }

    #[test]
    fn git_diff_reads_a_temp_repo() {
        let dir = repo("one\n", "two\n");

        let diff = git_diff(dir.path(), "HEAD~1", "HEAD", &files()).unwrap();

        assert!(diff.starts_with("diff --git a/f.txt b/f.txt\n"), "{diff}");
        assert!(diff.contains("-one\n+two\n"), "{diff}");
    }

    #[test]
    fn git_diff_follows_a_rename_from_either_path() {
        let twenty: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let dir = repo("one\n", &twenty);

        git(dir.path(), &["mv", "f.txt", "g.txt"]);
        fs::write(
            dir.path().join("g.txt"),
            twenty.replace("\n10\n", "\nten\n"),
        )
        .unwrap();
        fs::write(dir.path().join("other.txt"), "x\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-q", "-m", "rename"]);

        for file in ["g.txt", "f.txt"] {
            let diff = git_diff(dir.path(), "HEAD~1", "HEAD", &[file.to_string()]).unwrap();

            assert!(diff.starts_with("diff --git a/f.txt b/g.txt\n"), "{diff}");
            assert!(
                diff.contains("rename from f.txt\nrename to g.txt\n"),
                "{diff}"
            );
            assert_eq!(diff.matches("\n@@ ").count(), 1, "{diff}");
            assert!(diff.contains("-10\n+ten\n"), "{diff}");
            assert!(!diff.contains("other.txt"), "{diff}");
        }
    }

    #[test]
    fn sections_name_their_paths() {
        let renamed = concat!(
            "diff --git a/f.txt b/g.txt\nsimilarity index 92%\n",
            "rename from f.txt\nrename to g.txt\n"
        );
        let spaced = "diff --git a/a b c.txt b/a b c.txt\nold mode 100644\nnew mode 100755\n";
        let quoted = concat!(
            r#"diff --git "a/caf\303\251 \"x\".md" "b/caf\303\251 \"x\".md""#,
            "\nnew file mode 100644\n"
        );
        let quoted_rename = concat!(
            r#"diff --git a/t.txt "b/tab\there.txt""#,
            "\nsimilarity index 100%\nrename from t.txt\n",
            r#"rename to "tab\there.txt""#,
            "\n"
        );

        assert_eq!(section_paths(HEADER_A), ["a.txt"]);
        assert_eq!(section_paths(BINARY), ["x"]);
        assert_eq!(section_paths(renamed), ["f.txt", "g.txt"]);
        assert_eq!(section_paths(spaced), ["a b c.txt"]);
        assert_eq!(section_paths(quoted), ["café \"x\".md"]);
        assert_eq!(section_paths(quoted_rename), ["t.txt", "tab\there.txt"]);
    }

    #[test]
    fn git_diff_of_no_change_is_empty_diff() {
        let dir = repo("one\n", "two\n");

        let error = git_diff(dir.path(), "HEAD", "HEAD", &files()).unwrap_err();

        assert_eq!(error.code(), "empty_diff");
    }

    #[test]
    fn git_diff_of_a_bad_ref_is_git_failed() {
        let dir = repo("one\n", "two\n");

        let error = git_diff(dir.path(), "nope", "HEAD", &files()).unwrap_err();

        assert_eq!(error.code(), "git_failed");
    }

    #[test]
    fn git_reads_an_option_as_a_ref() {
        let dir = repo("one\n", "two\n");
        let out = dir.path().join("out.txt");
        let option = format!("--output={}", out.display());

        let diff = git_diff(dir.path(), &option, "HEAD", &files()).unwrap_err();
        let code = code_lines(dir.path(), "f.txt", 1, 1, Some(&option)).unwrap_err();

        assert_eq!(diff.code(), "git_failed");
        assert_eq!(code.code(), "git_failed");
        assert!(!out.exists());
    }

    #[test]
    fn code_lines_reads_the_worktree_and_a_ref() {
        let dir = repo("1\n2\n3\n4\n", "one\ntwo\nthree\nfour\n");

        let now = code_lines(dir.path(), "f.txt", 2, 3, None).unwrap();
        let before = code_lines(dir.path(), "f.txt", 2, 3, Some("HEAD~1")).unwrap();

        assert_eq!(now, "two\nthree\n");
        assert_eq!(before, "2\n3\n");
    }

    #[test]
    fn code_lines_refuses_bad_ranges() {
        let dir = repo("1\n2\n3\n4\n", "one\ntwo\nthree\nfour\n");

        for (start, end) in [(0, 1), (3, 2), (1, 9)] {
            let error = code_lines(dir.path(), "f.txt", start, end, None).unwrap_err();

            assert_eq!(error.code(), "bad_range", "{start}-{end}");
        }
    }

    #[test]
    fn code_lines_of_a_missing_file_is_git_failed() {
        let dir = repo("one\n", "two\n");

        let in_worktree = code_lines(dir.path(), "gone.txt", 1, 1, None).unwrap_err();
        let at_ref = code_lines(dir.path(), "gone.txt", 1, 1, Some("HEAD")).unwrap_err();

        assert_eq!(in_worktree.code(), "git_failed");
        assert_eq!(at_ref.code(), "git_failed");
    }
}
