/// <reference lib="webworker" />
// Hosts one WasmEngine. Commands arrive as JSON plus transferable buffers.
import init, { WasmEngine } from './wasm/ra_wasm.js';

export interface Request {
  id: number;
  cmd: string;
  bufs: ArrayBuffer[];
}

export type Response =
  | { id: number; kind: 'progress'; event: string }
  | { id: number; kind: 'reply'; event: string; bufs: ArrayBuffer[] };

const ready = init().then(() => new WasmEngine());

self.onmessage = async (msg: MessageEvent<Request>) => {
  const { id, cmd, bufs } = msg.data;
  const engine = await ready;
  const onEvent = (event: string) => self.postMessage({ id, kind: 'progress', event } satisfies Response);
  const reply = engine.handle(cmd, bufs.map((b) => new Uint8Array(b)), onEvent) as {
    event: string;
    bufs: Uint8Array[];
  };
  const out = reply.bufs.map((b) => b.buffer as ArrayBuffer);
  self.postMessage({ id, kind: 'reply', event: reply.event, bufs: out } satisfies Response, out);
};
