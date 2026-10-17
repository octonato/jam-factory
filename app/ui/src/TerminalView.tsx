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

import { useEffect, useRef } from "react";
import { ITheme, Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  OpenedTerminal,
  SessionTarget,
  TerminalKind,
  closeTerminal,
  openTerminal,
  resizeTerminal,
  writeTerminal,
} from "./api";

const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;

const LIGHT_THEME: ITheme = {
  background: "#ffffff",
  foreground: "#1f2328",
  cursor: "#1f2328",
  cursorAccent: "#ffffff",
  selectionBackground: "#b6d4fe",
  black: "#24292f",
  red: "#cf222e",
  green: "#116329",
  yellow: "#4d2d00",
  blue: "#0969da",
  magenta: "#8250df",
  cyan: "#1b7c83",
  white: "#6e7781",
  brightBlack: "#57606a",
  brightRed: "#a40e26",
  brightGreen: "#1a7f37",
  brightYellow: "#633c01",
  brightBlue: "#218bff",
  brightMagenta: "#a475f9",
  brightCyan: "#3192aa",
  brightWhite: "#8c959f",
};

type Props = {
  target: SessionTarget;
  dir: string;
  kind: TerminalKind;
  index: number | null;
  active: boolean;
  fontSize: number;
  onOpened: (opened: OpenedTerminal) => void;
  onExit: () => void;
  onError: (message: string) => void;
};

function decode(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export default function TerminalView(props: Props) {
  const container = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const callbacks = useRef(props);
  callbacks.current = props;

  useEffect(() => {
    const term = new Terminal({
      fontFamily: "Menlo, Monaco, monospace",
      fontSize: props.fontSize,
      cursorBlink: true,
      theme: LIGHT_THEME,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container.current!);
    termRef.current = term;
    fitRef.current = fit;

    const size = fit.proposeDimensions();
    const cols = size?.cols || DEFAULT_COLS;
    const rows = size?.rows || DEFAULT_ROWS;
    term.resize(cols, rows);

    let id: number | null = null;
    let disposed = false;

    openTerminal(
      { target: props.target, dir: props.dir, kind: props.kind, index: props.index, cols, rows },
      (event) => {
        if (disposed) {
          return;
        }
        if (event.kind === "output") {
          term.write(decode(event.data));
        } else {
          term.write("\r\n[session ended]\r\n");
          callbacks.current.onExit();
        }
      },
    )
      .then((opened) => {
        if (disposed) {
          closeTerminal(opened.id).catch(() => {});
          return;
        }
        id = opened.id;
        callbacks.current.onOpened(opened);
      })
      .catch((e) => callbacks.current.onError(String(e)));

    const input = term.onData((data) => {
      if (id !== null) {
        writeTerminal(id, data).catch(() => {});
      }
    });

    const observer = new ResizeObserver(() => {
      const next = fit.proposeDimensions();
      if (!next || !next.cols || !next.rows) {
        return;
      }
      fit.fit();
      if (id !== null) {
        resizeTerminal(id, next.cols, next.rows).catch(() => {});
      }
    });
    observer.observe(container.current!);

    return () => {
      disposed = true;
      observer.disconnect();
      input.dispose();
      if (id !== null) {
        closeTerminal(id).catch(() => {});
      }
      term.dispose();
    };
  }, []);

  useEffect(() => {
    if (props.active) {
      termRef.current?.focus();
    }
  }, [props.active]);

  return <div className="terminal-view" ref={container} />;
}
