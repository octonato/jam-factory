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

import { Checkout, Editor, Project, Repo, TerminalKind } from "./api";
import EditorMenu from "./EditorMenu";

type Props = {
  project: Project;
  editors: Editor[];
  terminalsEnabled: boolean;
  onNewTerminal: (repo: Repo, checkout: Checkout, kind: TerminalKind) => void;
  onError: (message: string) => void;
};

export default function Sidebar({
  project,
  editors,
  terminalsEnabled,
  onNewTerminal,
  onError,
}: Props) {
  return (
    <aside className="sidebar">
      <h2>{project.name}</h2>

      {project.errors.length > 0 && (
        <ul className="file-errors">
          {project.errors.map((e) => (
            <li key={e.line}>
              line {e.line}: {e.text} — {e.reason}
            </li>
          ))}
        </ul>
      )}

      {project.repos.map((repo) => (
        <section
          key={`${repo.org}/${repo.name}`}
          className={repo.cloned ? "repo" : "repo not-cloned"}
        >
          <h3>
            {repo.org}/{repo.name}
            {!repo.cloned && <span className="dim"> not cloned</span>}
          </h3>

          {repo.error && <p className="repo-error">{repo.error}</p>}

          <ul className="checkouts">
            {repo.checkouts.map((checkout) => (
              <li key={checkout.path} className="checkout">
                <div className="checkout-name">
                  {checkout.dirName}
                  <span className="dim"> {checkout.branch ?? "detached"}</span>
                </div>

                <div className="checkout-actions">
                  <button
                    disabled={!terminalsEnabled}
                    onClick={() => onNewTerminal(repo, checkout, "shell")}
                  >
                    Shell
                  </button>
                  <button
                    disabled={!terminalsEnabled}
                    onClick={() => onNewTerminal(repo, checkout, "claude")}
                  >
                    Claude
                  </button>
                  <EditorMenu
                    editors={editors}
                    checkoutPath={checkout.path}
                    onError={onError}
                  />
                </div>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </aside>
  );
}
