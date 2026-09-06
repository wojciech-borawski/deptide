import { registerCallback, unregisterCallback } from "./mock/events";
import { handleMockCommand } from "./mock/handlers";

export function installMockBackend(): void {
  const internals = {
    invoke: (command: string, args: Record<string, unknown> = {}) => handleMockCommand(command, args),
    transformCallback: registerCallback,
    unregisterCallback,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
  };

  Object.assign(window, { __TAURI_INTERNALS__: internals });
}
