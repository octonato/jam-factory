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

import { useCallback, useEffect, useRef, useState } from "react";
import {
  AppInfo,
  Checkout,
  Editor,
  LiveSession,
  Project,
  Repo,
  TerminalKind,
  appInfo,
  listEditors,
  listLiveSessions,
  openProject,
} from "./api";
import ReattachPrompt from "./ReattachPrompt";
import Sidebar from "./Sidebar";
import TerminalTabs, { Tab } from "./TerminalTabs";
import Toast from "./Toast";

type Props = { path: string };

type NewTab = Pick<Tab, "target" | "dir" | "kind" | "index" | "name">;

export default function ProjectView({ path }: Props) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [editors, setEditors] = useState<Editor[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [live, setLive] = useState<LiveSession[]>([]);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeKey, setActiveKey] = useState<number | null>(null);
  const nextKey = useRef(1);

  useEffect(() => {
    appInfo().then((i) => {
      setInfo(i);
      if (i.tmuxAvailable) {
        listLiveSessions(path)
          .then(setLive)
          .catch(() => {});
      }
    });
    listEditors().then(setEditors);
    openProject(path)
      .then(setProject)
      .catch((e) => setLoadError(String(e)));
  }, [path]);

  const dismissToast = useCallback(() => setToast(null), []);

  const addTabs = (added: NewTab[]) => {
    if (added.length === 0) {
      return;
    }

    const created = added.map((t) => ({
      ...t,
      key: nextKey.current++,
      generation: 0,
      ended: false,
    }));
    setTabs((current) => [...current, ...created]);
    setActiveKey(created[created.length - 1].key);
  };

  const newTerminal = (repo: Repo, checkout: Checkout, kind: TerminalKind) => {
    addTabs([
      {
        target: { org: repo.org, repo: repo.name, checkout: checkout.dirName },
        dir: checkout.path,
        kind,
        index: null,
        name: null,
      },
    ]);
  };

  const reattachAll = () => {
    const open = new Set(tabs.map((t) => t.name));
    addTabs(
      live
        .filter((s) => !open.has(s.name))
        .map((s) => ({
          target: s.target,
          dir: s.dir,
          kind: s.kind,
          index: s.index,
          name: s.name,
        })),
    );
    setLive([]);
  };

  const updateTab = (key: number, change: Partial<Tab>) =>
    setTabs((current) => current.map((t) => (t.key === key ? { ...t, ...change } : t)));

  const closeTab = (key: number) => {
    const remaining = tabs.filter((t) => t.key !== key);
    setTabs(remaining);
    if (activeKey === key) {
      setActiveKey(remaining.length > 0 ? remaining[remaining.length - 1].key : null);
    }
  };

  if (loadError) {
    return <p className="banner error">{loadError}</p>;
  }

  if (!info || !project) {
    return null;
  }

  return (
    <div className="project">
      <div className="banners">
        {info.githubSrc === null && (
          <p className="banner error">
            GITHUB_SRC is not set. Set it in your shell or in ~/.config/odeck/config.toml.
          </p>
        )}
        {!info.tmuxAvailable && (
          <p className="banner error">tmux not found on PATH. Terminals are disabled.</p>
        )}
        {info.configError && <p className="banner error">{info.configError}</p>}
        {live.length > 0 && (
          <ReattachPrompt sessions={live} onReattach={reattachAll} onDismiss={() => setLive([])} />
        )}
      </div>

      <div className="layout">
        <Sidebar
          project={project}
          editors={editors}
          terminalsEnabled={info.tmuxAvailable}
          onNewTerminal={newTerminal}
          onError={setToast}
        />
        <main className="terminals">
          <TerminalTabs
            tabs={tabs}
            activeKey={activeKey}
            fontSize={info.fontSize}
            onSelect={setActiveKey}
            onUpdate={updateTab}
            onClose={closeTab}
            onError={setToast}
          />
        </main>
      </div>

      {toast && <Toast message={toast} onDismiss={dismissToast} />}
    </div>
  );
}
