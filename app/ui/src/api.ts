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

import { Channel, invoke } from "@tauri-apps/api/core";

export type AppInfo = {
  tmuxAvailable: boolean;
  githubSrc: string | null;
  configError: string | null;
  fontSize: number;
};

export type LineError = { line: number; text: string; reason: string };

export type Checkout = { path: string; dirName: string; branch: string | null };

export type Repo = {
  org: string;
  name: string;
  path: string | null;
  cloned: boolean;
  error: string | null;
  checkouts: Checkout[];
};

export type Project = {
  name: string;
  root: string;
  errors: LineError[];
  repos: Repo[];
};

export type Editor = { name: string; command: string };

export type TerminalKind = "shell" | "claude";

export type SessionTarget = { org: string; repo: string; checkout: string };

export type OpenedTerminal = { id: number; name: string; index: number };

export type LiveSession = {
  name: string;
  target: SessionTarget;
  kind: TerminalKind;
  index: number;
  dir: string;
};

export type TerminalEvent =
  | { kind: "output"; data: string }
  | { kind: "exit"; code: number | null };

export type OpenTerminalArgs = {
  target: SessionTarget;
  dir: string;
  kind: TerminalKind;
  index: number | null;
  cols: number;
  rows: number;
};

export const appInfo = () => invoke<AppInfo>("app_info");

export const openProject = (path: string) => invoke<Project>("open_project", { path });

export const listEditors = () => invoke<Editor[]>("list_editors");

export const openInEditor = (editor: string, checkout: string) =>
  invoke<void>("open_in_editor", { editor, checkout });

export function openTerminal(
  args: OpenTerminalArgs,
  onEvent: (event: TerminalEvent) => void,
): Promise<OpenedTerminal> {
  const events = new Channel<TerminalEvent>();
  events.onmessage = onEvent;
  return invoke<OpenedTerminal>("open_terminal", { ...args, events });
}

export const writeTerminal = (id: number, data: string) =>
  invoke<void>("write_terminal", { id, data });

export const resizeTerminal = (id: number, cols: number, rows: number) =>
  invoke<void>("resize_terminal", { id, cols, rows });

export const closeTerminal = (id: number) => invoke<void>("close_terminal", { id });

export const killSession = (name: string) => invoke<void>("kill_session", { name });

export const listLiveSessions = (path: string) =>
  invoke<LiveSession[]>("list_live_sessions", { path });
