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

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_jam-factory-mcp");
const USAGE: &str = "Usage: jam-factory-mcp export [dest]";
const ID: &str = "session-20261104-0912-3f00486";

/// A temp home and the git worktree `<tmp>/org/repo/wt` on the branch `octonato/feat`.
struct Fixture {
    _tmp: TempDir,
    home: PathBuf,
    wt: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let wt = tmp.path().join("org").join("repo").join("wt");

        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&wt).unwrap();

        let fixture = Fixture {
            _tmp: tmp,
            home,
            wt,
        };

        fixture.git(&["init", "-q", "-b", "octonato/feat"]);
        fixture.git(&["commit", "-q", "--allow-empty", "-m", "init"]);
        fixture
    }

    /// Runs git with `args` in the worktree and returns its trimmed stdout.
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&self.wt)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();

        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// The folder of the session `ID` on the branch `feat` of `org/repo` in the default root.
    fn session_dir(&self) -> PathBuf {
        self.home
            .join(".jam-factory")
            .join("factory")
            .join("org")
            .join("repo")
            .join("sessions")
            .join("feat")
            .join(ID)
    }

    /// Writes `session.json`, `spec.md`, `round-1/report.md` and a `.lock` to the session `ID`.
    fn write_session(&self) {
        let dir = self.session_dir();
        fs::create_dir_all(dir.join("round-1")).unwrap();

        fs::write(dir.join("session.json"), "{}\n").unwrap();
        fs::write(dir.join("spec.md"), "spec\n").unwrap();
        fs::write(dir.join("round-1").join("report.md"), "report\n").unwrap();
        fs::write(dir.join(".lock"), "").unwrap();
    }

    /// Runs `jam-factory-mcp export` with `args` in the worktree.
    fn export(&self, args: &[&str]) -> Output {
        self.export_in(&self.wt, args)
    }

    /// Runs `jam-factory-mcp export` with `args` in `dir`.
    fn export_in(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(BINARY)
            .arg("export")
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Asserts that `dest` holds the files of the session written by `write_session`.
fn assert_exported(dest: &Path) {
    assert_eq!(fs::read_to_string(dest.join("spec.md")).unwrap(), "spec\n");
    assert_eq!(
        fs::read_to_string(dest.join("round-1").join("report.md")).unwrap(),
        "report\n"
    );
    assert!(!dest.join(".lock").exists());
}

#[test]
fn export_copies_to_factory_session_id() {
    let fixture = Fixture::new();
    fixture.write_session();

    let output = fixture.export(&[]);
    let out = stdout(&output);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_exported(&fixture.wt.join("factory").join(ID));
    assert!(out.contains(&format!("factory/{ID}")), "{out}");
    assert!(out.contains("3 files"), "{out}");

    assert_eq!(fixture.git(&["diff", "--cached", "--name-only"]), "");
    assert_eq!(fixture.git(&["rev-list", "--count", "HEAD"]), "1");
}

#[test]
fn export_honours_a_relative_dest() {
    let fixture = Fixture::new();
    fixture.write_session();

    let output = fixture.export(&["out/s"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_exported(&fixture.wt.join("out").join("s"));
    assert!(!fixture.wt.join("factory").exists());
}

#[test]
fn export_from_a_subfolder_copies_to_the_worktree_root() {
    let fixture = Fixture::new();
    fixture.write_session();

    let app = fixture.wt.join("app");
    fs::create_dir_all(&app).unwrap();

    let output = fixture.export_in(&app, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_exported(&fixture.wt.join("factory").join(ID));
    assert!(!app.join("factory").exists());
}

#[test]
fn export_from_a_subfolder_keeps_a_relative_dest_in_the_subfolder() {
    let fixture = Fixture::new();
    fixture.write_session();

    let app = fixture.wt.join("app");
    fs::create_dir_all(&app).unwrap();

    let output = fixture.export_in(&app, &["out/s"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_exported(&app.join("out").join("s"));
    assert!(!fixture.wt.join("out").exists());
}

#[test]
fn export_help_prints_the_usage() {
    let fixture = Fixture::new();
    fixture.write_session();

    for flag in ["-h", "--help"] {
        let output = fixture.export(&[flag]);
        let out = stdout(&output);

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert!(out.contains(USAGE), "{out}");
        assert!(!fixture.wt.join(flag).exists());
        assert!(!fixture.wt.join("factory").exists());
    }
}

#[test]
fn export_refuses_a_dest_starting_with_a_dash() {
    let fixture = Fixture::new();
    fixture.write_session();

    let output = fixture.export(&["--force"]);
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(err.contains("Unknown option: --force"), "{err}");
    assert!(err.contains(USAGE), "{err}");
    assert!(!fixture.wt.join("--force").exists());
    assert!(!fixture.wt.join("factory").exists());
}

#[test]
fn export_refuses_detached_head() {
    let fixture = Fixture::new();
    fixture.write_session();
    fixture.git(&["checkout", "-q", "--detach"]);

    let output = fixture.export(&[]);
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(err.contains("detached"), "{err}");
    assert!(!fixture.wt.join("factory").exists());
}

#[test]
fn export_of_a_missing_session_fails() {
    let fixture = Fixture::new();

    let output = fixture.export(&[]);
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        err.contains("Branch feat has no live session in org/repo"),
        "{err}"
    );
    assert!(!fixture.wt.join("factory").exists());
}

#[test]
fn export_with_an_existing_dest_fails() {
    let fixture = Fixture::new();
    fixture.write_session();

    let dest = fixture.wt.join("factory").join(ID);
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep.md"), "keep\n").unwrap();

    let output = fixture.export(&[]);
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(err.contains("already exists"), "{err}");
    assert!(!dest.join("spec.md").exists());
    assert_eq!(fs::read_to_string(dest.join("keep.md")).unwrap(), "keep\n");
}

#[test]
fn export_with_an_unreadable_root_fails() {
    let fixture = Fixture::new();
    fixture.write_session();

    let config = fixture.home.join(".jam-factory").join("config.json");
    fs::write(&config, r#"{ "root": 3 }"#).unwrap();

    let output = fixture.export(&[]);
    let err = stderr(&output);

    assert_eq!(output.status.code(), Some(1));
    assert!(err.contains("Cannot read the factory root"), "{err}");
    assert!(!fixture.wt.join("factory").exists());
}
