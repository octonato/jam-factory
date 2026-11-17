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

//! Keeps programs running on pseudo-terminals while no page shows them, and answers `/api/terminals`.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::api::{json_response, same_origin, segments};
use super::http::{Request, Response};
use super::pty::{Program, Pty, command_names};

/// The most output bytes a terminal keeps for its next viewer.
const KEPT_OUTPUT: usize = 256 * 1024;

/// What a terminal sends to its viewer.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// Bytes the program wrote.
    Output(Vec<u8>),
    /// Another viewer took the terminal.
    Taken,
    /// The program ended.
    Ended,
}

/// A program on a pseudo-terminal, with its last output and at most one viewer.
pub struct Terminal {
    pty: Mutex<Option<Pty>>,
    screen: Mutex<Screen>,
    next_viewer: AtomicU64,
}

/// The kept output, the viewer and whether the program ended.
struct Screen {
    output: VecDeque<u8>,
    viewer: Option<(u64, Sender<Event>)>,
    ended: bool,
}

impl Terminal {
    /// Runs `program` in `dir` on a `cols` × `rows` terminal.
    pub fn start(
        program: &Program,
        dir: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<Terminal>, String> {
        let mut pty = Pty::spawn(program, dir, cols, rows)?;
        let output = pty.take_output();

        let terminal = Arc::new(Terminal {
            pty: Mutex::new(Some(pty)),
            screen: Mutex::new(Screen {
                output: VecDeque::new(),
                viewer: None,
                ended: false,
            }),
            next_viewer: AtomicU64::new(1),
        });

        let pump = Arc::clone(&terminal);
        thread::Builder::new()
            .name("jam-board-terminal".to_string())
            .spawn(move || pump.pump(&output))
            .map_err(|error| error.to_string())?;

        Ok(terminal)
    }

    /// Keeps and forwards each chunk of `output`, then ends the terminal when the output closes.
    fn pump(&self, output: &Receiver<Vec<u8>>) {
        for chunk in output {
            self.keep(chunk);
        }

        self.kill();

        let mut screen = lock(&self.screen);
        screen.ended = true;
        if let Some((_, viewer)) = screen.viewer.take() {
            let _ = viewer.send(Event::Ended);
        }
    }

    /// Adds `chunk` to the kept output and sends it to the viewer.
    fn keep(&self, chunk: Vec<u8>) {
        let mut screen = lock(&self.screen);

        screen.output.extend(&chunk);
        let excess = screen.output.len().saturating_sub(KEPT_OUTPUT);
        screen.output.drain(..excess);

        let gone = screen
            .viewer
            .as_ref()
            .is_some_and(|(_, viewer)| viewer.send(Event::Output(chunk)).is_err());
        if gone {
            screen.viewer = None;
        }
    }

    /// A new viewer that gets the kept output first. The previous viewer gets `Taken`.
    pub fn view(self: &Arc<Self>) -> Viewer {
        let (sender, events) = mpsc::channel();
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);

        let mut screen = lock(&self.screen);
        if let Some((_, previous)) = screen.viewer.take() {
            let _ = previous.send(Event::Taken);
        }

        if !screen.output.is_empty() {
            let kept = screen.output.iter().copied().collect();
            let _ = sender.send(Event::Output(kept));
        }

        if screen.ended {
            let _ = sender.send(Event::Ended);
        } else {
            screen.viewer = Some((id, sender));
        }
        drop(screen);

        Viewer {
            terminal: Arc::clone(self),
            id,
            events,
        }
    }

    /// Drops the viewer `id` when it is the current one.
    fn leave(&self, id: u64) {
        let mut screen = lock(&self.screen);

        if screen
            .viewer
            .as_ref()
            .is_some_and(|(viewer, _)| *viewer == id)
        {
            screen.viewer = None;
        }
    }

    /// Whether the program ended.
    pub fn ended(&self) -> bool {
        lock(&self.screen).ended
    }

    /// Writes `bytes` to the program's terminal input.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        match lock(&self.pty).as_mut() {
            Some(pty) => pty.write(bytes),
            None => Ok(()),
        }
    }

    /// Sets the terminal size to `cols` × `rows`.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        match lock(&self.pty).as_ref() {
            Some(pty) => pty.resize(cols, rows),
            None => Ok(()),
        }
    }

    /// The leader of the terminal's foreground process group, when it is not the program.
    pub fn foreground(&self) -> Option<u32> {
        lock(&self.pty).as_ref()?.foreground()
    }

    /// Kills the program and waits for it to end.
    pub fn kill(&self) {
        let pty = lock(&self.pty).take();
        drop(pty);
    }
}

/// The page that shows a terminal. It leaves the terminal when dropped.
pub struct Viewer {
    terminal: Arc<Terminal>,
    id: u64,
    events: Receiver<Event>,
}

impl Viewer {
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.terminal.leave(self.id);
    }
}

/// The terminals that run in worktrees, by id.
pub struct Terminals {
    entries: Mutex<Entries>,
}

/// The last id given and the open terminals.
struct Entries {
    last_id: u64,
    open: BTreeMap<u64, Entry>,
}

impl Default for Terminals {
    /// No terminals, with ids that start at the current time in milliseconds.
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());

        Terminals {
            entries: Mutex::new(Entries {
                last_id: u64::try_from(now).unwrap_or(0),
                open: BTreeMap::new(),
            }),
        }
    }
}

/// A terminal with its folder, its program name, its tab name and whether a user named the tab.
struct Entry {
    path: PathBuf,
    program: String,
    name: String,
    renamed: bool,
    terminal: Arc<Terminal>,
}

impl Entries {
    /// Drops the terminals whose program ended.
    fn prune(&mut self) {
        self.open.retain(|_, entry| !entry.terminal.ended());
    }
}

impl Terminals {
    /// Runs `program` in `dir` as a new terminal named `name`, and returns its id and the terminal. `kind` is the program name the list shows.
    pub fn open(
        &self,
        dir: &Path,
        kind: &str,
        program: &Program,
        name: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(u64, Arc<Terminal>), String> {
        let terminal = Terminal::start(program, dir, cols, rows)?;

        let mut entries = lock(&self.entries);
        entries.last_id += 1;
        let id = entries.last_id;

        let entry = Entry {
            path: dir.to_path_buf(),
            program: kind.to_string(),
            name: name.to_string(),
            renamed: false,
            terminal: Arc::clone(&terminal),
        };
        entries.open.insert(id, entry);

        Ok((id, terminal))
    }

    /// The running terminal `id`.
    pub fn get(&self, id: u64) -> Option<Arc<Terminal>> {
        let mut entries = lock(&self.entries);
        entries.prune();

        entries
            .open
            .get(&id)
            .map(|entry| Arc::clone(&entry.terminal))
    }

    /// The running terminals in id order, as `{"terminals": [{"id", "path", "program", "name", "renamed", "command"}]}`.
    /// `command` is the name of the foreground job, or null when the program is in the foreground.
    pub fn to_json(&self) -> Value {
        let mut entries = lock(&self.entries);
        entries.prune();

        let rows: Vec<(Value, Option<u32>)> = entries
            .open
            .iter()
            .map(|(id, entry)| {
                let row = json!({
                    "id": id,
                    "path": entry.path.to_string_lossy(),
                    "program": entry.program,
                    "name": entry.name,
                    "renamed": entry.renamed,
                });
                (row, entry.terminal.foreground())
            })
            .collect();
        drop(entries);

        let pids: Vec<u32> = rows.iter().filter_map(|(_, pid)| *pid).collect();
        let names = command_names(&pids);

        let terminals: Vec<Value> = rows
            .into_iter()
            .map(|(mut row, pid)| {
                row["command"] = json!(pid.and_then(|pid| names.get(&pid)));
                row
            })
            .collect();

        json!({ "terminals": terminals })
    }

    /// Names terminal `id` `name`, and returns whether it runs.
    pub fn rename(&self, id: u64, name: &str) -> bool {
        let mut entries = lock(&self.entries);
        entries.prune();

        match entries.open.get_mut(&id) {
            Some(entry) => {
                entry.name = name.to_string();
                entry.renamed = true;
                true
            }
            None => false,
        }
    }

    /// Kills terminal `id`, and returns whether it was open.
    pub fn kill(&self, id: u64) -> bool {
        let entry = lock(&self.entries).open.remove(&id);

        let Some(entry) = entry else {
            return false;
        };
        entry.terminal.kill();
        true
    }

    /// Kills every terminal.
    pub fn kill_all(&self) {
        let open = mem::take(&mut lock(&self.entries).open);

        for entry in open.values() {
            entry.terminal.kill();
        }
    }
}

/// Answers `POST /api/terminals/<id>/kill` and `POST /api/terminals/<id>/name` with `terminals`.
pub fn post(terminals: &Terminals, request: &Request) -> Response {
    if !same_origin(request) {
        return Response::error(403);
    }

    let segments = segments(&request.target);
    let ["", "api", "terminals", id, action] = segments.as_slice() else {
        return Response::error(404);
    };
    let Ok(id) = id.parse::<u64>() else {
        return Response::error(404);
    };

    let done = match *action {
        "kill" => terminals.kill(id),
        "name" => match tab_name(&request.body) {
            Some(name) => terminals.rename(id, &name),
            None => return Response::error(400),
        },
        _ => return Response::error(404),
    };

    if done {
        Response::new(204, "text/plain; charset=utf-8", Vec::new())
    } else {
        Response::error(404)
    }
}

/// The running terminals, as `{"terminals": [...]}`.
pub fn get(terminals: &Terminals) -> Response {
    json_response(&terminals.to_json())
}

/// The trimmed UTF-8 text of `body`, when it is not blank.
fn tab_name(body: &[u8]) -> Option<String> {
    let name = str::from_utf8(body).ok()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
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

    /// The output events of `viewer` up to the first one that makes them hold `needle`.
    fn read_until(viewer: &Viewer, needle: &str) -> String {
        let deadline = Instant::now() + TIMEOUT;
        let mut bytes = Vec::new();

        loop {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.contains(needle) {
                return text;
            }

            let left = deadline.saturating_duration_since(Instant::now());
            match viewer.events().recv_timeout(left) {
                Ok(Event::Output(chunk)) => bytes.extend(chunk),
                Ok(event) => panic!("no {needle:?} in {text:?}: {event:?}"),
                Err(error) => panic!("no {needle:?} in {text:?}: {error}"),
            }
        }
    }

    /// The first event of `viewer` that is not output.
    fn next_signal(viewer: &Viewer) -> Event {
        loop {
            match viewer.events().recv_timeout(TIMEOUT) {
                Ok(Event::Output(_)) => {}
                Ok(event) => return event,
                Err(RecvTimeoutError::Timeout) => panic!("no event"),
                Err(RecvTimeoutError::Disconnected) => panic!("the terminal dropped the viewer"),
            }
        }
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

    /// Waits up to 5 s for the process `pid` to end.
    fn assert_ends(pid: u32) {
        let deadline = Instant::now() + TIMEOUT;

        while alive(pid) {
            assert!(Instant::now() < deadline, "{pid} still runs");
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Waits up to 5 s for `terminal` to end.
    fn assert_ended(terminal: &Terminal) {
        let deadline = Instant::now() + TIMEOUT;

        while !terminal.ended() {
            assert!(Instant::now() < deadline, "the terminal still runs");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_dropped_viewer_leaves_the_program_running() {
        let dir = tempfile::tempdir().unwrap();
        let terminal = Terminal::start(&sh("echo $$; exec sleep 60"), dir.path(), 80, 24).unwrap();

        let viewer = terminal.view();
        let pid: u32 = read_until(&viewer, "\n").trim().parse().unwrap();
        drop(viewer);

        thread::sleep(Duration::from_millis(100));
        assert!(alive(pid));

        terminal.kill();
        assert_ends(pid);
    }

    #[test]
    fn a_new_viewer_gets_the_kept_output_then_new_output() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh(r#"printf first; read line; printf "got:%s" "$line"; sleep 60"#);
        let terminal = Terminal::start(&program, dir.path(), 80, 24).unwrap();

        let first = terminal.view();
        read_until(&first, "first");
        drop(first);

        let second = terminal.view();
        read_until(&second, "first");
        terminal.write(b"hi\r").unwrap();
        read_until(&second, "got:hi");

        terminal.kill();
    }

    #[test]
    fn a_new_viewer_takes_the_terminal_from_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let terminal =
            Terminal::start(&sh("printf ready; exec sleep 60"), dir.path(), 80, 24).unwrap();

        let old = terminal.view();
        read_until(&old, "ready");
        let new = terminal.view();

        assert_eq!(next_signal(&old), Event::Taken);
        drop(old);

        terminal.kill();
        assert_eq!(next_signal(&new), Event::Ended);
    }

    #[test]
    fn the_kept_output_holds_only_the_last_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let program = sh(r#"head -c 300000 /dev/zero | tr "\0" a; printf END; exec sleep 60"#);
        let terminal = Terminal::start(&program, dir.path(), 80, 24).unwrap();

        read_until(&terminal.view(), "END");

        let kept = match terminal.view().events().recv_timeout(TIMEOUT) {
            Ok(Event::Output(kept)) => kept,
            other => panic!("no kept output: {other:?}"),
        };
        assert_eq!(kept.len(), KEPT_OUTPUT);
        assert!(kept.ends_with(b"aEND"));

        terminal.kill();
    }

    #[test]
    fn program_exit_ends_the_terminal_and_tells_the_viewer() {
        let dir = tempfile::tempdir().unwrap();
        let terminal = Terminal::start(&sh("printf bye"), dir.path(), 80, 24).unwrap();
        let viewer = terminal.view();

        assert_eq!(next_signal(&viewer), Event::Ended);
        assert!(terminal.ended());

        let late = terminal.view();
        assert_eq!(next_signal(&late), Event::Ended);
    }

    #[test]
    fn the_list_shows_open_terminals_in_id_order() {
        let dir = tempfile::tempdir().unwrap();
        let terminals = Terminals::default();
        let program = sh("exec sleep 60");

        let (first, _) = terminals
            .open(dir.path(), "shell", &program, "shell 1", 80, 24)
            .unwrap();
        let (second, _) = terminals
            .open(dir.path(), "claude", &program, "claude", 80, 24)
            .unwrap();
        assert!(terminals.rename(first, "build"));

        let path = dir.path().to_string_lossy();
        assert_eq!(
            terminals.to_json(),
            json!({ "terminals": [
                { "id": first, "path": path, "program": "shell", "name": "build", "renamed": true, "command": null },
                { "id": second, "path": path, "program": "claude", "name": "claude", "renamed": false, "command": null },
            ]})
        );

        terminals.kill_all();
        assert_eq!(terminals.to_json(), json!({ "terminals": [] }));
    }

    #[test]
    fn the_list_shows_the_foreground_command() {
        let dir = tempfile::tempdir().unwrap();
        let terminals = Terminals::default();
        let program = Program {
            name: "sh".to_string(),
            args: vec!["-i".to_string()],
        };

        let (_, terminal) = terminals
            .open(dir.path(), "shell", &program, "shell 1", 80, 24)
            .unwrap();
        terminal.write(b"sleep 60\r").unwrap();

        let deadline = Instant::now() + TIMEOUT;
        while terminals.to_json()["terminals"][0]["command"] != "sleep" {
            assert!(Instant::now() < deadline, "{}", terminals.to_json());
            thread::sleep(Duration::from_millis(20));
        }

        terminals.kill_all();
    }

    #[test]
    fn kill_ends_the_program_and_drops_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let terminals = Terminals::default();
        let program = sh("echo $$; exec sleep 60");

        let (id, terminal) = terminals
            .open(dir.path(), "shell", &program, "shell 1", 80, 24)
            .unwrap();
        let pid: u32 = read_until(&terminal.view(), "\n").trim().parse().unwrap();

        assert!(terminals.kill(id));
        assert_ends(pid);
        assert!(terminals.get(id).is_none());
        assert!(!terminals.kill(id));
        assert!(!terminals.rename(id, "gone"));
    }

    #[test]
    fn an_ended_program_leaves_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let terminals = Terminals::default();

        let (id, terminal) = terminals
            .open(dir.path(), "shell", &sh("true"), "shell 1", 80, 24)
            .unwrap();
        assert_ended(&terminal);

        assert!(terminals.get(id).is_none());
        assert_eq!(terminals.to_json(), json!({ "terminals": [] }));
    }

    #[test]
    fn tab_names_are_trimmed_and_not_blank() {
        assert_eq!(tab_name(b"  build "), Some("build".to_string()));
        assert_eq!(tab_name(b"   "), None);
        assert_eq!(tab_name(&[0xFF]), None);
    }
}
