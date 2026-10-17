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
import { Editor, openInEditor } from "./api";

type Props = {
  editors: Editor[];
  checkoutPath: string;
  onError: (message: string) => void;
};

export default function EditorMenu({ editors, checkoutPath, onError }: Props) {
  const [open, setOpen] = useState(false);

  const pick = (editor: Editor) => {
    setOpen(false);
    openInEditor(editor.name, checkoutPath).catch((e) => onError(String(e)));
  };

  return (
    <div className="editor-menu">
      <button onClick={() => setOpen(!open)}>Open in…</button>

      {open && (
        <ul className="menu" onMouseLeave={() => setOpen(false)}>
          {editors.length === 0 && (
            <li className="menu-empty">No editors in ~/.config/odeck/config.toml</li>
          )}
          {editors.map((editor) => (
            <li key={editor.name}>
              <button onClick={() => pick(editor)}>{editor.name}</button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
