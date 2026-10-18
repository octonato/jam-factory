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

//! Opens a job's worktree in a tool on `POST /open/<tool>/<repo>/<id>`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::archive::{name, same_origin};
use super::http::{Request, Response};

const TEXT: &str = "text/plain; charset=utf-8";

/// A tool that opens a worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    IntelliJ,
    Zed,
    ITerm,
}

impl Tool {
    /// The tool with the id `id`.
    pub fn parse(id: &str) -> Option<Tool> {
        match id {
            "intellij" => Some(Tool::IntelliJ),
            "zed" => Some(Tool::Zed),
            "iterm" => Some(Tool::ITerm),
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

/// Answers `POST` for the factory root `root`.
pub fn post(root: &Path, request: &Request) -> Response {
    post_with(root, request, &SystemRunner)
}

/// Answers `POST` for the factory root `root`, running the tool with `runner`.
pub fn post_with(root: &Path, request: &Request, runner: &dyn Runner) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let Some((tool, job)) = tool_and_job(root, &request.target) else {
        return Response::error(404);
    };

    let Some(worktree) = worktree(&job) else {
        return Response::error(404);
    };

    let (program, args) = tool.command(&worktree);

    match runner.run(&program, &args) {
        Ok(()) => Response::new(204, TEXT, Vec::new()),
        Err(error) => Response::new(500, TEXT, error.into_bytes()),
    }
}

/// The tool and the `job.json` named in `target`, if the names are safe and the file exists.
fn tool_and_job(root: &Path, target: &str) -> Option<(Tool, PathBuf)> {
    let path = target.split('?').next().unwrap_or_default();

    let ["", "open", tool, repo, id] = path.split('/').collect::<Vec<_>>()[..] else {
        return None;
    };

    let tool = Tool::parse(tool)?;
    let repo = name(repo)?;
    let id = name(id)?;

    let file = root.join(repo).join("jobs").join(id).join("job.json");
    file.is_file().then_some((tool, file))
}

/// The `worktree` folder named in the job file `file`, if it is a string naming an existing folder.
fn worktree(file: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(file).ok()?;
    let job: Value = serde_json::from_str(&text).ok()?;

    let worktree = PathBuf::from(job.get("worktree")?.as_str()?);
    worktree.is_dir().then_some(worktree)
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

    use super::*;

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

    /// A factory root holding the job `stack/a`, and that job's worktree.
    struct Fixture {
        root: PathBuf,
        worktree: PathBuf,
        _dir: tempfile::TempDir,
    }

    impl Fixture {
        fn write_job(&self, job: &Value) {
            self.write("stack/jobs/a/job.json", job);
        }

        /// Writes `job` to `file`, a path relative to the root.
        fn write(&self, file: &str, job: &Value) {
            let file = self.root.join(file);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, job.to_string()).unwrap();
        }
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let worktree = dir.path().join("worktree");
        fs::create_dir_all(root.join("stack/jobs/a")).unwrap();
        fs::create_dir(&worktree).unwrap();

        let f = Fixture {
            root,
            worktree,
            _dir: dir,
        };
        f.write_job(&serde_json::json!({ "worktree": f.worktree }));
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

        let response = post_with(&f.root, &request("/open/zed/stack/a", &[]), &runner);

        assert_eq!(response.status, 204);
        assert!(response.body.is_empty());

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

        let response = post_with(&f.root, &request("/open/zed/stack/a", &[]), &runner);

        assert_eq!(response.status, 500);
        assert_eq!(response.body, error.as_bytes());
    }

    #[test]
    fn missing_worktree_is_404() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));
        let gone = f.worktree.join("gone");
        let file = f.worktree.join("file");
        fs::write(&file, "").unwrap();

        for job in [
            serde_json::json!({ "worktree": 3 }),
            serde_json::json!({}),
            serde_json::json!({ "worktree": gone }),
            serde_json::json!({ "worktree": file }),
        ] {
            f.write_job(&job);

            let response = post_with(&f.root, &request("/open/zed/stack/a", &[]), &runner);

            assert_eq!(response.status, 404, "{job}");
        }

        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn unknown_tool_or_job_is_404() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));

        for target in [
            "/open/vim/stack/a",
            "/open/zed/stack/b",
            "/open/zed/stack/..",
            "/open/zed/stack/a/extra",
        ] {
            let response = post_with(&f.root, &request(target, &[]), &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn bad_names_are_404() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));
        let job = serde_json::json!({ "worktree": f.worktree });

        // Each target next to the `job.json` its names point at.
        let targets = [
            ("/open/zed/../a", "../jobs/a/job.json"),
            ("/open/zed/stack/..", "stack/job.json"),
        ];

        for (_, file) in targets {
            f.write(file, &job);
        }

        for (target, _) in targets {
            let response = post_with(&f.root, &request(target, &[]), &runner);

            assert_eq!(response.status, 404, "{target}");
        }

        assert!(runner.calls.into_inner().is_empty());
    }

    #[test]
    fn foreign_origin_is_403() {
        let f = fixture();
        let runner = RecordingRunner::new(Ok(()));
        let foreign = request("/open/zed/stack/a", &[("Origin", "http://evil")]);

        let response = post_with(&f.root, &foreign, &runner);

        assert_eq!(response.status, 403);
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
