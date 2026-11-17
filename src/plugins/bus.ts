// Tiny event bus: the app emits, plugins listen (through hangar.events.on).
// Kept free of imports so store.ts can use it without import cycles.

type Handler = (payload: any) => void;
const handlers = new Map<string, Set<Handler>>();

export function on(event: string, h: Handler): () => void {
  if (!handlers.has(event)) handlers.set(event, new Set());
  handlers.get(event)!.add(h);
  return () => handlers.get(event)?.delete(h);
}

export function emit(event: string, payload: unknown) {
  for (const h of handlers.get(event) ?? []) {
    try {
      h(payload);
    } catch (e) {
      console.error(`[plugins] handler for "${event}" failed`, e);
    }
  }
}
