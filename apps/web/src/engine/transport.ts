import type { Command } from './protocol/Command';
import type { Event } from './protocol/Event';

export interface Reply {
  event: Event;
  bufs: ArrayBuffer[];
}

/** The only thing each platform implements: move a command and its buffers to the engine. */
export interface Transport {
  send(cmd: Command, bufs?: ArrayBuffer[], onEvent?: (e: Event) => void): Promise<Reply>;
  dispose(): void;
}
