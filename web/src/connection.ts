import { parseTacticalEnvelope, PROTOCOL_VERSION, type TacticalEnvelope } from "./protocol";

interface StreamHandlers {
  onEnvelope: (envelope: TacticalEnvelope, firstSnapshot: boolean) => void;
  onStatus: (message: string, kind: "waiting" | "connected" | "error") => void;
}

const MAX_MESSAGE_LENGTH = 262_144;
const CONNECT_TIMEOUT_MS = 10_000;
const IDLE_TIMEOUT_MS = 15_000;

/** Maintain one authenticated, same-origin connection with bounded retry frequency. */
export function connectTacticalStream(token: string, handlers: StreamHandlers): () => void {
  let active: WebSocket | null = null;
  let retryTimer: number | null = null;
  let timeoutTimer: number | null = null;
  let attempts = 0;
  let stopped = false;

  const clearTimeoutTimer = (): void => {
    if (timeoutTimer !== null) window.clearTimeout(timeoutTimer);
    timeoutTimer = null;
  };
  const retry = (): void => {
    if (stopped || retryTimer !== null) return;
    handlers.onStatus("Reconnecting", "waiting");
    const delay = Math.min(10_000, 1_000 * 2 ** Math.min(attempts++, 4));
    retryTimer = window.setTimeout(() => {
      retryTimer = null;
      connect();
    }, delay);
  };
  const connect = (): void => {
    if (stopped) return;
    handlers.onStatus("Connecting", "waiting");
    let socket: WebSocket;
    try {
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      socket = new WebSocket(`${protocol}//${window.location.host}/ws`);
    } catch {
      retry();
      return;
    }
    active = socket;
    let awaitingSnapshot = true;
    const disconnect = (reconnect: boolean, message = "Disconnected"): void => {
      if (active !== socket) return;
      active = null;
      clearTimeoutTimer();
      socket.close();
      if (reconnect) retry();
      else handlers.onStatus(message, "error");
    };
    const armTimeout = (delay: number): void => {
      clearTimeoutTimer();
      timeoutTimer = window.setTimeout(() => disconnect(true), delay);
    };
    armTimeout(CONNECT_TIMEOUT_MS);
    socket.addEventListener("open", () => {
      if (active !== socket) return;
      socket.send(JSON.stringify({ type: "authenticate", token }));
      handlers.onStatus("Authenticating", "waiting");
    });
    socket.addEventListener("message", (event: MessageEvent<unknown>) => {
      if (active !== socket) return;
      if (typeof event.data !== "string" || event.data.length > MAX_MESSAGE_LENGTH) {
        disconnect(false, "Rejected invalid data");
        return;
      }
      let value: unknown;
      try { value = JSON.parse(event.data) as unknown; }
      catch { disconnect(false, "Rejected malformed data"); return; }
      if (isHeartbeat(value)) {
        if (!awaitingSnapshot) armTimeout(IDLE_TIMEOUT_MS);
        return;
      }
      const envelope = parseTacticalEnvelope(value);
      if (envelope === null) { disconnect(false, "Rejected invalid data"); return; }
      if (awaitingSnapshot && envelope.type !== "session_snapshot") return;
      const firstSnapshot = awaitingSnapshot;
      awaitingSnapshot = false;
      attempts = 0;
      armTimeout(IDLE_TIMEOUT_MS);
      handlers.onEnvelope(envelope, firstSnapshot);
      handlers.onStatus("Live · localhost", "connected");
    });
    socket.addEventListener("close", (event: CloseEvent) => {
      disconnect(event.code !== 1008, "Session expired - reopen cWEB from Arma");
    });
    socket.addEventListener("error", () => disconnect(true));
  };
  connect();
  return () => {
    stopped = true;
    if (retryTimer !== null) window.clearTimeout(retryTimer);
    clearTimeoutTimer();
    const socket = active;
    active = null;
    socket?.close();
  };
}

function isHeartbeat(value: unknown): boolean {
  if (value === null || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  if (record.type !== "heartbeat" || record.protocol_version !== PROTOCOL_VERSION
    || typeof record.session_id !== "string" || record.session_id.length === 0
    || record.session_id.length > 96 || !Number.isSafeInteger(record.sequence)
    || typeof record.sequence !== "number" || record.sequence < 0
    || record.payload === null || typeof record.payload !== "object") return false;
  const payload = record.payload as Record<string, unknown>;
  return typeof payload.uptime_ms === "number" && Number.isFinite(payload.uptime_ms) && payload.uptime_ms >= 0;
}
