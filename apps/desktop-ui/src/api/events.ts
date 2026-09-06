import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { RunEvent } from "./types";

export const runEventName = "run-event";

export function listenRunEvents(handler: (event: RunEvent) => void): Promise<UnlistenFn> {
  return listen<RunEvent>(runEventName, (event) => handler(event.payload));
}
