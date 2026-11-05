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

//! Opens a session's worktree or open pull request in a tool.
//! Route: `POST /api/open/<tool>/<org>/<repo>/<branch>/<id>`.

use std::path::Path;
use std::process::Command;

use jam_factory_store::sessions::SESSION_FILE;

use super::api::{error_response, same_origin, segments, session_ref, worktree};
use super::http::{Request, Response};
use crate::store::Store;

const TEXT: &str = "text/plain; charset=utf-8";

/// A `sh` script that opens the open pull request of the worktree `$1` on GitHub.
const GITHUB_SCRIPT: &str = r#"cd "$1" || exit 1
PATH="$PATH:/opt/homebrew/bin:/usr/local/bin"
url=$(gh pr view --json url,state --jq 'select(.state == "OPEN") | .url') || exit 1
if [ -z "$url" ]; then
    echo "no open pull request" >&2
    exit 1
fi
open "$url"
"#;

/// A tool that opens a worktree or its open pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    IntelliJ,
    Zed,
    ITerm,
    GitHub,
}

impl Tool {
    /// The tool with the id `id`.
    pub fn parse(id: &str) -> Option<Tool> {
        match id {
            "intellij" => Some(Tool::IntelliJ),
            "zed" => Some(Tool::Zed),
            "iterm" => Some(Tool::ITerm),
            "github" => Some(Tool::GitHub),
            _ => None,
        }
    }

    /// The program and arguments that open `worktree` in this tool.
    pub fn command(&self, worktree: &Path) -> (String, Vec<String>) {
        let worktree = worktree.to_string_lossy().into_owned();

        match self {
            Tool::IntelliJ => open_app("IntelliJ IDEA", worktree),
            Tool::Zed => open_app("Zed", worktree),
            Tool::ITerm => (
                "osascript".to_string(),
                vec!["-e".to_string(), iterm_script(&worktree)],
            ),
            Tool::GitHub => (
                "sh".to_string(),
                vec![
                    "-c".to_string(),
                    GITHUB_SCRIPT.to_string(),
                    "sh".to_string(),
                    worktree,
                ],
            ),
        }
    }
}

/// Runs a program and waits for it to exit.
pub trait Runner {
    /// Runs `program` with `args`, giving its stderr or the start error on failure.
    fn run(&self, program: &str, args: &[String]) -> Result<(), String>;
}

/// Runs programs with `std::process::Command`.
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<(), String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|error| error.to_string())?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(output.status.to_string())
        } else {
            Err(stderr)
        }
    }
}

/// Answers `POST` with `store`.
pub fn post(store: &dyn Store, request: &Request) -> Response {
    post_with(store, request, &SystemRunner)
}

/// Answers `POST` with `store`, running the tool with `runner`.
pub fn post_with(store: &dyn Store, request: &Request, runner: &dyn Runner) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let segments = segments(&request.target);

    let ["", "api", "open", tool, org, repo, branch, id] = segments.as_slice() else {
        return Response::error(404);
    };

    let Some(tool) = Tool::parse(tool) else {
        return Response::error(404);
    };

    let session =
        session_ref([org, repo, branch, id]).and_then(|s| store.read_file(&s, SESSION_FILE));

    let session = match session {
        Ok(file) => file.content,
        Err(error) => return error_response(&error),
    };

    let Some(worktree) = worktree(&session) else {
        return Response::error(404);
    };

    let (program, args) = tool.command(&worktree);

    match runner.run(&program, &args) {
        Ok(()) => Response::new(204, TEXT, Vec::new()),
        Err(error) => Response::new(500, TEXT, error.into_bytes()),
    }
}

/// The `open -a <app> <worktree>` command.
fn open_app(app: &str, worktree: String) -> (String, Vec<String>) {
    let args = vec!["-a".to_string(), app.to_string(), worktree];
    ("open".to_string(), args)
}

/// An AppleScript that focuses the iTerm session in `worktree`, or opens a tab there.
fn iterm_script(worktree: &str) -> String {
    let path = applescript_escape(worktree);
    let cd = applescript_escape(&format!("cd {}", shell_single_quote(worktree)));

    format!(
        r#"tell application "iTerm"
    repeat with aWindow in windows
        repeat with aTab in tabs of aWindow
            repeat with aSession in sessions of aTab
                tell aSession to set sessionPath to (variable named "path")
                if sessionPath is "{path}" then
                    select aWindow
                    select aTab
                    select aSession
                    activate
                    return
                end if
            end repeat
        end repeat
    end repeat

    activate
    if (count of windows) = 0 then
        create window with default profile
    else
        tell current window to create tab with default profile
    end if
    tell current session of current window to write text "{cd}"
end tell"#
    )
}

/// Wraps `s` in single quotes for a POSIX shell, escaping embedded single quotes.
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Escapes `s` for an AppleScript double-quoted string literal.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;

    use jam_factory_store::StoreError;
    use serde_json::Value;

    use super::*;
    use crate::server::fake_store::FakeStore;

    const TARGET: &str = "/api/open/zed/o/stack/main/a";

    /// A runner that records each call and gives `result`.
    struct RecordingRunner {
        calls: RefCell<Vec<(String, Vec<String>)>>,
        result: Result<(), String>,
    }

    impl RecordingRunner {
        fn new(result: Result<(), String>) -> RecordingRunner {
            RecordingRunner {
                calls: RefCell::new(Vec::new()),
                result,
            }
        }
    }

    impl Runner for RecordingRunner {
        fn run(&self, program: &str, args: &[String]) -> Result<(), String> {
            let call = (program.to_string(), args.to_vec());
            self.calls.borrow_mut().push(call);
            self.result.clone()
        }
    }

    /// A store holding the session `o/stack/main/a`, and that session's worktree.
    struct Fixture {
        store: FakeStore,
        worktree: PathBuf,
        _dir: tempfile::TempDir,
    }

    impl Fixture {
        fn write_session(&self, session: &Value) {
            self.store.write("session.json", &session.to_string());
        }
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("worktree");
        fs::create_dir(&worktree).unwrap();

        let f = Fixture {
            store: FakeStore::default(),
            worktree,
            _dir: dir,
        };
        f.write_session(&serde_json::json!({ "worktree": f.worktree }));
        f
    }

    fn request(target: &str, headers: &[(&str, &str)]) -> Request {
        let mut all = vec![("Host".to_string(), "127.0.0.1:1".to_string())];
        for (name, value) in headers {
            all.push((name.to_string(), value.to_string()));
        }

        Request {
            method: "POST".to_string(),
            target: target.to_string(),
            headers: all,
            body: Vec::new(),
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn command(program: &str, args: &[&str]) -> (String, Vec<String>) {
        (program.to_string(), strings(args))
    }

    /// A folder with stub `gh` and `open` programs in `bin/`, and the worktree `work tree/`.
    struct Stubs {
        dir: tempfile::TempDir,
    }

    impl Stubs {
        /// Stubs where `gh` runs its `--jq` filter on a pull request in `state` at `url`, if `jq` is on `PATH`.
        fn pr(state: &str, url: &str) -> Option<Stubs> {
            let jq = find_program("jq")?;
            let record = serde_json::json!({ "state": state, "url": url }).to_string();

            let gh = format!(
                r#"while [ $# -gt 0 ]; do
    if [ "$1" = --jq ]; then filter=$2; fi
    shift
done
printf '%s' {record} | {jq} -r "$filter"
"#,
                record = shell_single_quote(&record),
                jq = shell_single_quote(&jq.to_string_lossy()),
            );

            Some(Stubs::new(&gh))
        }

        /// Stubs where `gh` prints `error` to stderr and exits 1.
        fn failing(error: &str) -> Stubs {
            Stubs::new(&format!("echo {} >&2\nexit 1\n", shell_single_quote(error)))
        }

        /// Stubs where `gh` runs the shell script `gh`.
        fn new(gh: &str) -> Stubs {
            let dir = tempfile::tempdir().unwrap();
            let bin = dir.path().join("bin");
            fs::create_dir(&bin).unwrap();
            fs::create_dir(dir.path().join("work tree")).unwrap();

            write_program(&bin.join("gh"), gh);

            let opened = dir.path().join("opened").to_string_lossy().into_owned();
            let open = format!("printf '%s' \"$1\" > {}\n", shell_single_quote(&opened));
            write_program(&bin.join("open"), &open);

            Stubs { dir }
        }

        /// Runs `GITHUB_SCRIPT` on the worktree with only the stubs and the system folders on `PATH`.
        fn run(&self) -> Result<(), String> {
            let bin = self.dir.path().join("bin");
            let path = format!("PATH={}:/usr/bin:/bin", bin.display());
            let worktree = self.dir.path().join("work tree");
            let worktree = worktree.to_string_lossy();

            let args = strings(&[&path, "sh", "-c", GITHUB_SCRIPT, "sh", &worktree]);
            SystemRunner.run("env", &args)
        }

        /// The argument the `open` stub got, if it ran.
        fn opened(&self) -> Option<String> {
            fs::read_to_string(self.dir.path().join("opened")).ok()
        }
    }

    /// The file `name` in the first folder on `PATH` that has it.
    fn find_program(name: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;

        std::env::split_paths(&path)
            .map(|folder| folder.join(name))
            .find(|file| file.is_file())
    }

    /// Writes the executable shell script `body` to `file`.
    fn write_program(file: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(file, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn tools_build_their_commands() {
        let worktree = Path::new("/w x");

        assert_eq!(
            Tool::IntelliJ.command(worktree),
            command("open", &["-a", "IntelliJ IDEA", "/w x"])
        );
        assert_eq!(
            Tool::Zed.command(worktree),
            command("open", &["-a", "Zed", "/w x"])
        );

        let (program, args) = Tool::ITerm.command(worktree);
        assert_eq!(program, "osascript");
        assert_eq!(args[0], "-e");
        assert_eq!(args[1], iterm_script("/w x"));
    }

    #[test]
    fn unknown_tool_is_none() {
        assert_eq!(Tool::parse("vim"), None);
        assert_eq!(Tool::parse("intellij"), Some(Tool::IntelliJ));
        assert_eq!(Tool::parse("zed"), Some(Tool::Zed));
        assert_eq!(Tool::parse("iterm"), Some(Tool::ITerm));
        assert_eq!(Tool::parse("github"), Some(Tool::GitHub));
    }

    #[test]
    fn github_passes_the_worktree_as_an_argument() {
        let (program, args) = Tool::GitHub.command(Path::new("/w x"));

        assert_eq!(program, "sh");
        assert_eq!(args, strings(&["-c", GITHUB_SCRIPT, "sh", "/w x"]));
    }

    #[test]
    fn github_script_without_an_open_pr_fails() {
        for state in ["MERGED", "CLOSED"] {
            let Some(stubs) = Stubs::pr(state, "https://github.com/o/r/pull/1") else {
                eprintln!("skipped: jq is not on PATH");
                return;
            };

            assert_eq!(
                stubs.run(),
                Err("no open pull request".to_string()),
                "{state}"
            );
            assert_eq!(stubs.opened(), None, "{state}");
        }
    }

    #[test]
    fn github_script_opens_the_pr_url() {
        let Some(stubs) = Stubs::pr("OPEN", "https://github.com/o/r/pull/1") else {
            eprintln!("skipped: jq is not on PATH");
            return;
        };

        assert_eq!(stubs.run(), Ok(()));
        assert_eq!(
            stubs.opened().as_deref(),
            Some("https://github.com/o/r/pull/1")
        );
    }

    #[test]
    fn github_script_gives_the_gh_error() {
        let stubs = Stubs::failing("no pull requests found");

        assert_eq!(stubs.run(), Err("no pull requests found".to_string()));
    }

    #[test]
    fn iterm_script_quotes_the_worktree() {
        let script = iterm_script(r#"/a b/it's "q""#);

        assert!(
            script.contains(r#"write text "cd '/a b/it'\\''s \"q\"'""#),
            "{script}"
        );
        assert!(
            script.contains(r#"if sessionPath is "/a b/it's \"q\"" then"#),
            "{script}"
        );
        assert!(script.contains(r#"variable named "path""#), "{script}");
    }

    #[test]
    fn runs_the_tool_on_the_worktree() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));

        let response = post_with(&f.store, &request(TARGET, &[]), &runner);

        assert_eq!(response.status, 204);
        assert!(response.body.is_empty());
        assert_eq!(f.store.reads(), ["o/stack/main/a/session.json"]);

        let worktree = f.worktree.to_string_lossy().into_owned();
        assert_eq!(
            runner.calls.into_inner(),
            [command("open", &["-a", "Zed", worktree.as_str()])]
        );
    }

    #[test]
    fn runner_failure_is_500_with_the_error() {
        let f = fixture();
        let error = "Unable to find application named 'Zed'";
        let runner = RecordingRunner::new(Err(error.to_string()));

        let response = post_with(&f.store, &request(TARGET, &[]), &runner);

        assert_eq!(response.status, 500);
        assert_eq!(response.body, error.as_bytes());
    }

    #[test]
    fn store_failure_is_500_with_the_message() {
        let mut f = fixture();
        f.store.failure = Some(StoreError::Io("the disk is full".to_string()));
        let runner = RecordingRunner::new(Ok(()));

        let response = post_with(&f.store, &request(TARGET, &[]), &runner);

        assert_eq!(response.status, 500);
        assert_eq!(response.body, b"the disk is full");
        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn missing_worktree_is_404() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));
        let gone = f.worktree.join("gone");
        let file = f.worktree.join("file");
        fs::write(&file, "").unwrap();

        for session in [
            serde_json::json!({ "worktree": 3 }),
            serde_json::json!({}),
            serde_json::json!({ "worktree": gone }),
            serde_json::json!({ "worktree": file }),
        ] {
            f.write_session(&session);

            let response = post_with(&f.store, &request(TARGET, &[]), &runner);

            assert_eq!(response.status, 404, "{session}");
        }

        f.store.write("session.json", "not json");
        let response = post_with(&f.store, &request(TARGET, &[]), &runner);

        assert_eq!(response.status, 404);
        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn unknown_tool_or_session_is_404() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));

        for target in [
            "/api/open/vim/o/stack/main/a",
            "/api/open/zed/o/stack/main/b",
            "/api/open/zed/o/stack/main/a/extra",
            "/api/open/zed/stack/a",
            "/open/zed/stack/a",
        ] {
            let response = post_with(&f.store, &request(target, &[]), &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn bad_names_are_404() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));

        for target in [
            "/api/open/zed/%2e%2e/stack/main/a",
            "/api/open/zed/o/stack/main/..",
            "/api/open/zed/o/stack/main/.",
            "/api/open/zed/o/a%2Fb/main/a",
            "/api/open/zed/o/stack/main/a%5Cb",
            "/api/open/zed//stack/main/a",
            "/api/open/zed/o/%FF/main/a",
        ] {
            let response = post_with(&f.store, &request(target, &[]), &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(f.store.reads().is_empty());
        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn open_ignores_the_old_job_layout() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));
        f.store.remove("session.json");
        let job = serde_json::json!({ "worktree": f.worktree });
        f.store.write("job.json", &job.to_string());

        let response = post_with(&f.store, &request(TARGET, &[]), &runner);

        assert_eq!(response.status, 404);
        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn foreign_origin_is_403() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));
        let foreign = request(TARGET, &[("Origin", "http://evil")]);

        let response = post_with(&f.store, &foreign, &runner);

        assert_eq!(response.status, 403);
        assert!(f.store.reads().is_empty());
        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn system_runner_gives_stderr_or_status_on_failure() {
        let run = |script: &str| SystemRunner.run("sh", &strings(&["-c", script]));

        assert_eq!(run("true"), Ok(()));
        assert_eq!(run("echo ' oops ' >&2; exit 1"), Err("oops".to_string()));
        assert_eq!(run("exit 3"), Err("exit status: 3".to_string()));
    }

    #[test]
    fn system_runner_fails_when_the_program_does_not_start() {
        assert!(SystemRunner.run("no-such-program", &[]).is_err());
    }
}
