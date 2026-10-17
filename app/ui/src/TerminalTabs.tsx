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

import { useState } from "react";
import { OpenedTerminal, SessionTarget, TerminalKind, killSession } from "./api";
import TerminalView from "./TerminalView";

export type Tab = {
  key: number;
  target: SessionTarget;
  dir: string;
  kind: TerminalKind;
  index: number | null;
  name: string | null;
  generation: number;
  ended: boolean;
};

type Props = {
  tabs: Tab[];
  activeKey: number | null;
  fontSize: number;
  onSelect: (key: number) => void;
  onUpdate: (key: number, change: Partial<Tab>) => void;
  onClose: (key: number) => void;
  onError: (message: string) => void;
};

export function tabTitle(tab: Tab): string {
  const suffix = tab.index !== null && tab.index > 1 ? `-${tab.index}` : "";
  return `${tab.target.repo}/${tab.target.checkout} · ${tab.kind}${suffix}`;
}

export default function TerminalTabs({
  tabs,
  activeKey,
  fontSize,
  onSelect,
  onUpdate,
  onClose,
  onError,
}: Props) {
  const [menuFor, setMenuFor] = useState<number | null>(null);

  const kill = (tab: Tab) => {
    setMenuFor(null);
    if (tab.name === null) {
      return;
    }
    killSession(tab.name)
      .then(() => onClose(tab.key))
      .catch((e) => onError(String(e)));
  };

  if (tabs.length === 0) {
    return <p className="terminals-empty dim">Open a shell or Claude from a checkout.</p>;
  }

  return (
    <>
      <nav className="tabs">
        {tabs.map((tab) => (
          <div
            key={tab.key}
            className={tab.key === activeKey ? "tab active" : "tab"}
            onClick={() => onSelect(tab.key)}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenuFor(tab.key);
            }}
          >
            <span>{tabTitle(tab)}</span>
            <button
              className="tab-close"
              title="Close tab (the session keeps running)"
              onClick={(e) => {
                e.stopPropagation();
                onClose(tab.key);
              }}
            >
              ×
            </button>

            {menuFor === tab.key && (
              <ul className="menu" onMouseLeave={() => setMenuFor(null)}>
                <li>
                  <button onClick={() => kill(tab)}>Kill session</button>
                </li>
              </ul>
            )}
          </div>
        ))}
      </nav>

      {tabs.map((tab) => (
        <div
          key={`${tab.key}-${tab.generation}`}
          className="tab-body"
          style={{ display: tab.key === activeKey ? "flex" : "none" }}
        >
          <TerminalView
            target={tab.target}
            dir={tab.dir}
            kind={tab.kind}
            index={tab.index}
            active={tab.key === activeKey}
            fontSize={fontSize}
            onOpened={(opened: OpenedTerminal) =>
              onUpdate(tab.key, { name: opened.name, index: opened.index })
            }
            onExit={() => onUpdate(tab.key, { ended: true })}
            onError={onError}
          />

          {tab.ended && (
            <div className="tab-ended">
              <button
                onClick={() =>
                  onUpdate(tab.key, { ended: false, generation: tab.generation + 1 })
                }
              >
                Reopen
              </button>
            </div>
          )}
        </div>
      ))}
    </>
  );
}
