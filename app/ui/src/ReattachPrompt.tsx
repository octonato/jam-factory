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

import { LiveSession } from "./api";

type Props = {
  sessions: LiveSession[];
  onReattach: () => void;
  onDismiss: () => void;
};

export default function ReattachPrompt({ sessions, onReattach, onDismiss }: Props) {
  return (
    <div className="banner reattach">
      <p>
        {sessions.length} running {sessions.length === 1 ? "session" : "sessions"}:
      </p>
      <ul>
        {sessions.map((s) => (
          <li key={s.name}>{s.name}</li>
        ))}
      </ul>
      <div className="reattach-actions">
        <button onClick={onReattach}>Reattach all</button>
        <button onClick={onDismiss}>Dismiss</button>
      </div>
    </div>
  );
}
