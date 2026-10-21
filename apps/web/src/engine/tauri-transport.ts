import type { Command } from './protocol/Command';
import type { Event } from './protocol/Event';
import type { Reply, Transport } from './transport';

/**
 * Mac transport: `invoke('engine', …)` into native ra-api, progress over a Tauri Channel.
 * Filled in when apps/desktop starts; the interface is fixed now so the UI never changes.
 */
export class TauriTransport implements Transport {
  send(_cmd: Command, _bufs?: ArrayBuffer[], _onEvent?: (e: Event) => void): Promise<Reply> {
    return Promise.reject(new Error('The Mac app is not built yet'));
  }

  dispose(): void {}
}

export const inTauri = (): boolean => '__TAURI_INTERNALS__' in window;
