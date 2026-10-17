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

import { useEffect } from "react";

const DISMISS_AFTER_MS = 6000;

type Props = { message: string; onDismiss: () => void };

export default function Toast({ message, onDismiss }: Props) {
  useEffect(() => {
    const timer = setTimeout(onDismiss, DISMISS_AFTER_MS);
    return () => clearTimeout(timer);
  }, [message, onDismiss]);

  return (
    <div className="toast" role="alert" onClick={onDismiss}>
      {message}
    </div>
  );
}
