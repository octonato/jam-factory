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

//! Runs a program on a pseudo-terminal.

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// The folders added to the end of the program's `PATH`.
const EXTRA_PATH: &str = "/opt/homebrew/bin:/usr/local/bin";

const READ_BUFFER: usize = 4096;

/// A program and its arguments.
#[derive(Debug, Clone)]
pub struct Program {
    pub name: String,
    pub args: Vec<String>,
}

/// The command that runs `program` in `dir` with `TERM=xterm-256color` and `path_var` plus `EXTRA_PATH` as `PATH`.
pub fn command(program: &Program, dir: &Path, path_var: Option<&OsStr>) -> CommandBuilder {
    let mut command = CommandBuilder::new(&program.name);
    command.args(&program.args);
    command.cwd(dir);
    command.env("TERM", "xterm-256color");
    command.env("PATH", path(path_var));
    command
}

/// The folders of `path_var`, if any, followed by `EXTRA_PATH`.
fn path(path_var: Option<&OsStr>) -> OsString {
    let mut path = OsString::new();

    if let Some(path_var) = path_var.filter(|path_var| !path_var.is_empty()) {
        path.push(path_var);
        path.push(":");
    }

    path.push(EXTRA_PATH);
    path
}

/// A program running on a pseudo-terminal, killed and reaped when dropped.
pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Receiver<Vec<u8>>,
}

impl Pty {
    /// Runs `program` in `dir` on a `cols` × `rows` terminal, with the current `PATH` extended.
    pub fn spawn(program: &Program, dir: &Path, cols: u16, rows: u16) -> Result<Pty, String> {
        let path_var = env::var_os("PATH");
        Pty::start(command(program, dir, path_var.as_deref()), cols, rows)
    }

    /// Runs `command` on a `cols` × `rows` terminal.
    pub fn start(command: CommandBuilder, cols: u16, rows: u16) -> Result<Pty, String> {
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|error| error.to_string())?;

        let master = pair.master;
        let writer = master.take_writer().map_err(|error| error.to_string())?;
        let reader = master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| error.to_string())?;
        drop(pair.slave);

        let (sender, output) = mpsc::channel();
        let pty = Pty {
            master,
            writer,
            child,
            output,
        };

        thread::Builder::new()
            .name("jam-board-pty".to_string())
            .spawn(move || forward(reader, &sender))
            .map_err(|error| error.to_string())?;

        Ok(pty)
    }

    /// The chunks the program writes to the terminal, until it closes.
    pub fn output(&self) -> &Receiver<Vec<u8>> {
        &self.output
    }

    /// Writes `bytes` to the program's terminal input.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    /// Sets the terminal size to `cols` × `rows`.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master
            .resize(size(cols, rows))
            .map_err(|error| error.to_string())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The terminal size `cols` × `rows`.
fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Sends each chunk read from `reader` to `sender`, until end of file, a read error or a closed channel.
fn forward(mut reader: Box<dyn Read + Send>, sender: &Sender<Vec<u8>>) {
    let mut buffer = [0; READ_BUFFER];

    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };

        if sender.send(buffer[..count].to_vec()).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::{Command, Stdio};
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(5);

    /// The program `sh -c script`.
    fn sh(script: &str) -> Program {
        Program {
            name: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
        }
    }

    /// The output of `pty` up to the first chunk that makes it hold `needle`.
    fn read_until(pty: &Pty, needle: &str) -> String {
        let deadline = Instant::now() + TIMEOUT;
        let mut bytes = Vec::new();

        loop {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.contains(needle) {
                return text;
            }

            let left = deadline.saturating_duration_since(Instant::now());
            match pty.output().recv_timeout(left) {
                Ok(chunk) => bytes.extend(chunk),
                Err(error) => panic!("no {needle:?} in {text:?}: {error}"),
            }
        }
    }

    /// The output of `pty` until its channel disconnects.
    fn drain(pty: &Pty) -> Vec<u8> {
        let deadline = Instant::now() + TIMEOUT;
        let mut bytes = Vec::new();

        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match pty.output().recv_timeout(left) {
                Ok(chunk) => bytes.extend(chunk),
                Err(RecvTimeoutError::Disconnected) => return bytes,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("no end after {:?}", String::from_utf8_lossy(&bytes))
                }
            }
        }
    }

    /// Writes the executable shell script `body` to `file`.
    fn write_program(file: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        fs::write(file, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Whether the process `pid` exists.
    fn alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn runs_the_program_in_the_dir() {
        let dir = tempfile::tempdir().unwrap();
        let expected = fs::canonicalize(dir.path()).unwrap();

        let pty = Pty::spawn(&sh("pwd"), dir.path(), 80, 24).unwrap();

        read_until(&pty, &expected.to_string_lossy());
    }

    #[test]
    fn runs_in_a_dir_with_a_space_and_a_quote() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("work tree's");
        fs::create_dir(&dir).unwrap();
        let expected = fs::canonicalize(&dir).unwrap();

        let pty = Pty::spawn(&sh("pwd"), &dir, 80, 24).unwrap();

        read_until(&pty, &expected.to_string_lossy());
    }

    #[test]
    fn passes_input_to_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh(r#"read line; printf "got:%s" "$line""#);
        let mut pty = Pty::spawn(&program, dir.path(), 80, 24).unwrap();

        pty.write(b"hi\r").unwrap();

        read_until(&pty, "got:hi");
    }

    #[test]
    fn starts_at_the_size_and_resizes() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh("stty size; read line; stty size; read line");
        let mut pty = Pty::spawn(&program, dir.path(), 120, 40).unwrap();

        read_until(&pty, "40 120");

        pty.resize(100, 30).unwrap();
        pty.write(b"\r").unwrap();

        read_until(&pty, "30 100");
    }

    #[test]
    fn sets_term_and_extends_path() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh(r#"printf "%s|%s" "$TERM" "$PATH""#);
        let pty = Pty::spawn(&program, dir.path(), 80, 24).unwrap();

        let output = String::from_utf8(drain(&pty)).unwrap();

        assert!(output.contains("xterm-256color|"), "{output}");
        assert!(
            output.contains(":/opt/homebrew/bin:/usr/local/bin"),
            "{output}"
        );
    }

    #[test]
    fn command_extends_the_path() {
        let program = sh("true");
        let dir = Path::new("/");

        let with_path = command(&program, dir, Some(OsStr::new("/usr/bin:/bin")));
        assert_eq!(
            with_path.get_env("PATH"),
            Some(OsStr::new("/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin"))
        );

        let without_path = command(&program, dir, None);
        assert_eq!(
            without_path.get_env("PATH"),
            Some(OsStr::new("/opt/homebrew/bin:/usr/local/bin"))
        );

        let empty_path = command(&program, dir, Some(OsStr::new("")));
        assert_eq!(
            empty_path.get_env("PATH"),
            Some(OsStr::new("/opt/homebrew/bin:/usr/local/bin"))
        );
    }

    #[test]
    fn start_resolves_the_program_on_the_command_path() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        write_program(&bin.join("jam-stub"), "printf found");

        let program = Program {
            name: "jam-stub".to_string(),
            args: vec![],
        };
        let command = command(&program, dir.path(), Some(bin.as_os_str()));
        let pty = Pty::start(command, 80, 24).unwrap();

        let output = String::from_utf8(drain(&pty)).unwrap();

        assert!(output.contains("found"), "{output}");
    }

    #[test]
    fn output_ends_when_the_program_exits() {
        let dir = tempfile::tempdir().unwrap();
        let pty = Pty::spawn(&sh("printf bye"), dir.path(), 80, 24).unwrap();

        let output = drain(&pty);

        assert!(output.windows(3).any(|bytes| bytes == b"bye"), "{output:?}");
    }

    #[test]
    fn dropping_kills_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let pty = Pty::spawn(&sh("echo $$; exec sleep 60"), dir.path(), 80, 24).unwrap();
        let pid: u32 = read_until(&pty, "\n").trim().parse().unwrap();

        drop(pty);

        let deadline = Instant::now() + TIMEOUT;
        while alive(pid) {
            assert!(Instant::now() < deadline, "{pid} still runs");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn missing_program_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let program = Program {
            name: "no-such-program-jam".into(),
            args: vec![],
        };

        let error = Pty::spawn(&program, dir.path(), 80, 24).err();

        let error = error.expect("no-such-program-jam started");
        assert!(error.contains("no-such-program-jam"), "{error}");
    }
}
