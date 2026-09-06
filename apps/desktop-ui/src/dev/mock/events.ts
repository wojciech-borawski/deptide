type Callback = (payload: unknown) => void;

interface Listener {
  event: string;
  handler: number;
}

const callbacks = new Map<number, Callback>();
const listeners: Listener[] = [];

let callbackId = 0;
let listenerId = 0;

export function registerCallback(callback: Callback): number {
  callbackId += 1;
  callbacks.set(callbackId, callback);
  return callbackId;
}

export function unregisterCallback(id: number): boolean {
  return callbacks.delete(id);
}

export function registerListener(event: string, handler: number): number {
  listenerId += 1;
  listeners.push({ event, handler });
  return listenerId;
}

export function emit(event: string, payload: unknown): void {
  for (const listener of listeners) {
    if (listener.event !== event) continue;
    callbacks.get(listener.handler)?.({ event, id: listener.handler, payload });
  }
}
