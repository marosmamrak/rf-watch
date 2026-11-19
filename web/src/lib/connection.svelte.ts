import { parseMsg, type Band, type Frame } from './protocol';

export type ConnState = 'connecting' | 'open' | 'closed';

/** WebSocket URL: `?ws=` override, otherwise /ws on the host that served the page. */
export function streamUrl(loc: Pick<Location, 'protocol' | 'host' | 'search'>): string {
  const override = new URLSearchParams(loc.search).get('ws');
  if (override) return override;
  const proto = loc.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${loc.host}/ws`;
}

/** Reconnect delay: 0.5 s doubling up to 8 s. */
export function backoffMs(attempt: number): number {
  return Math.min(8000, 500 * 2 ** attempt);
}

/** Reactive connection to the rf-watch stream with automatic reconnect. */
export class Connection {
  state = $state<ConnState>('connecting');
  device = $state('');
  version = $state('');
  band = $state.raw<Band | null>(null);
  frame = $state.raw<Frame | null>(null);
  lastMessageAt = $state(0);

  #url: string;
  #ws: WebSocket | null = null;
  #attempt = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #stopped = false;

  constructor(url: string) {
    this.#url = url;
  }

  start() {
    this.#stopped = false;
    this.#connect();
  }

  stop() {
    this.#stopped = true;
    clearTimeout(this.#timer);
    this.#ws?.close();
    this.#ws = null;
  }

  #connect() {
    this.state = 'connecting';
    const ws = new WebSocket(this.#url);
    this.#ws = ws;
    ws.onopen = () => {
      this.state = 'open';
      this.#attempt = 0;
    };
    ws.onmessage = (ev) => {
      const msg = parseMsg(ev.data);
      if (!msg) return;
      this.lastMessageAt = Date.now();
      if (msg.type === 'hello') {
        this.device = msg.device;
        this.version = msg.version;
        this.band = msg.band;
      } else {
        this.frame = msg;
      }
    };
    ws.onclose = () => {
      if (this.#ws !== ws) return;
      this.state = 'closed';
      if (!this.#stopped) {
        this.#timer = setTimeout(() => this.#connect(), backoffMs(this.#attempt++));
      }
    };
  }
}
