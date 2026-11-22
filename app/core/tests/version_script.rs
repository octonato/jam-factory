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
use std::path::Path;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

/// The path of the `app/version` script.
const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../version");

/// A git repo on `main` with a copy of the script and a `Cargo.toml` at `cargo_version`, in one commit.
fn repo(cargo_version: &str) -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    git(dir, &["init", "-q", "-b", "main"]);
    fs::copy(SCRIPT, dir.join("version")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!("[workspace.package]\nversion = \"{cargo_version}\"\n"),
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "init"]);

    tmp
}

/// Runs `git args` in `dir` as user `t`, without the user and system git config.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(status.success(), "git {args:?}");
}

/// Adds an empty commit with `message` in `dir`.
fn commit(dir: &Path, message: &str) {
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
}

/// Tags HEAD in `dir` with the annotated tag `name`.
fn tag(dir: &Path, name: &str) {
    git(dir, &["tag", "-a", name, "-m", name]);
}

/// Runs `bash {dir}/version args` from the system temp dir, without the user and system git config.
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(dir.join("version"))
        .args(args)
        .current_dir(std::env::temp_dir())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

/// The full sha of HEAD in `dir`.
fn head(dir: &Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();

    assert!(output.status.success(), "git rev-parse HEAD");
    stdout(&output)
}

/// The trimmed stdout of `output`.
fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn build_without_a_release_tag_is_the_cargo_version_and_the_hash() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();

    assert_eq!(
        stdout(&run(dir, &["build"])),
        format!("0.4.2-{}", &head(dir)[..8])
    );
}

#[test]
fn build_on_a_release_tag_is_the_tag_version() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    tag(dir, "v1.2.3");

    assert_eq!(stdout(&run(dir, &["build"])), "1.2.3");
}

#[test]
fn build_on_a_release_tag_with_a_modified_file_appends_the_hash() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    tag(dir, "v1.2.3");
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.4.2\"\nedition = \"2021\"\n",
    )
    .unwrap();

    assert_eq!(
        stdout(&run(dir, &["build"])),
        format!("1.2.3-{}", &head(dir)[..8])
    );
}

#[test]
fn build_on_a_release_tag_with_an_untracked_file_appends_the_hash() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    tag(dir, "v1.2.3");
    fs::write(dir.join("notes.txt"), "draft\n").unwrap();

    assert_eq!(
        stdout(&run(dir, &["build"])),
        format!("1.2.3-{}", &head(dir)[..8])
    );
}

#[test]
fn build_after_a_release_tag_appends_the_hash() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    tag(dir, "v1.2.3");
    commit(dir, "c");

    assert_eq!(
        stdout(&run(dir, &["build"])),
        format!("1.2.3-{}", &head(dir)[..8])
    );
}

#[test]
fn build_ignores_tags_that_are_not_releases() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    tag(dir, "jam-board-v9.9.9");
    tag(dir, "v2.0.0-rc1");
    tag(dir, "vnext");

    assert_eq!(
        stdout(&run(dir, &["build"])),
        format!("0.4.2-{}", &head(dir)[..8])
    );
}

#[test]
fn build_takes_the_highest_release_tag_reachable_from_head() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    commit(dir, "a");
    tag(dir, "v1.2.3");
    commit(dir, "b");
    tag(dir, "v1.10.0");
    git(dir, &["checkout", "-q", "-b", "side"]);
    commit(dir, "side");
    tag(dir, "v9.0.0");
    git(dir, &["checkout", "-q", "main"]);
    commit(dir, "c");

    assert_eq!(
        stdout(&run(dir, &["build"])),
        format!("1.10.0-{}", &head(dir)[..8])
    );
}

#[test]
fn next_bumps_the_patch_minor_and_major_parts() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();

    assert_eq!(stdout(&run(dir, &["next", "patch"])), "0.4.3");
    assert_eq!(stdout(&run(dir, &["next", "minor"])), "0.5.0");
    assert_eq!(stdout(&run(dir, &["next", "major"])), "1.0.0");
}

#[test]
fn next_accepts_an_explicit_version_with_or_without_v() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();

    assert_eq!(stdout(&run(dir, &["next", "0.9.0"])), "0.9.0");
    assert_eq!(stdout(&run(dir, &["next", "v0.9.0"])), "0.9.0");
}

#[test]
fn next_refuses_bad_and_unchanged_versions() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();

    for bump in ["1.2", "1.2.3-rc1", "banana", "0.4.2", "v0.4.2"] {
        let output = run(dir, &["next", bump]);

        assert_eq!(output.status.code(), Some(1), "{bump}");
        assert!(output.stdout.is_empty(), "{bump}");
    }
}

#[test]
fn next_refuses_a_version_whose_tag_exists() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();
    tag(dir, "v0.4.3");

    let output = run(dir, &["next", "patch"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn an_unknown_command_prints_the_usage() {
    let tmp = repo("0.4.2");
    let dir = tmp.path();

    assert_eq!(run(dir, &[]).status.code(), Some(2));
    assert_eq!(run(dir, &["publish"]).status.code(), Some(2));
    assert_eq!(run(dir, &["next"]).status.code(), Some(2));
}
