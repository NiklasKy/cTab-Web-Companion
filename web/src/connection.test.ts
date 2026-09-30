import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { connectTacticalStream } from "./connection";
import { applyEnvelope, type LiveState } from "./live-state";

class FakeSocket extends EventTarget {
  static instances: FakeSocket[] = [];
  sent: string[] = [];
  closed = false;
  constructor(readonly url: string) { super(); FakeSocket.instances.push(this); }
  send(value: string): void { this.sent.push(value); }
  close(): void { this.closed = true; }
  receive(value: unknown): void { this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(value) })); }
}

const snapshot = {
  protocol_version: 1, session_id: "session-a", sequence: 100, type: "session_snapshot",
  payload: {
    mission_name: "Recovery", ctab_edition: "original",
    capabilities: { map: true, own_position: true, bft: true },
    terrain: { world_name: "Altis", display_name: "Altis", world_size: 30720 },
    entities: [], markers: []
  }
};

beforeEach(() => { vi.useFakeTimers(); FakeSocket.instances = []; vi.stubGlobal("WebSocket", FakeSocket); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("local stream recovery", () => {
  it("reconnects, authenticates again, and replaces state even if the new sequence is lower", () => {
    let state: LiveState | null = null;
    const receive = vi.fn((envelope, firstSnapshot) => {
      state = applyEnvelope(firstSnapshot ? null : state, envelope);
    });
    const stop = connectTacticalStream("a".repeat(64), { onEnvelope: receive, onStatus: vi.fn() });
    const first = FakeSocket.instances[0]!;
    first.dispatchEvent(new Event("open"));
    expect(JSON.parse(first.sent[0]!)).toEqual({ type: "authenticate", token: "a".repeat(64) });
    first.receive(snapshot);
    first.dispatchEvent(new CloseEvent("close", { code: 1006 }));
    vi.advanceTimersByTime(1000);
    const second = FakeSocket.instances[1]!;
    second.dispatchEvent(new Event("open"));
    expect(second.sent).toEqual(first.sent);
    second.receive({ ...snapshot, sequence: 2 });
    expect(receive.mock.calls.map((call) => call[1])).toEqual([true, true]);
    expect((state as LiveState | null)?.sequence).toBe(2);
    first.receive({ ...snapshot, sequence: 200 });
    expect(receive).toHaveBeenCalledTimes(2);
    stop();
    vi.advanceTimersByTime(60000);
    expect(FakeSocket.instances).toHaveLength(2);
  });

  it("recovers a silent connection but keeps an idle mission alive through validated heartbeats", () => {
    const stop = connectTacticalStream("a".repeat(64), { onEnvelope: vi.fn(), onStatus: vi.fn() });
    const socket = FakeSocket.instances[0]!;
    socket.receive(snapshot);
    for (let index = 0; index < 8; index++) {
      vi.advanceTimersByTime(5000);
      socket.receive({ protocol_version: 1, session_id: "session-a", sequence: 101 + index,
        type: "heartbeat", payload: { uptime_ms: 1000 * index } });
    }
    expect(FakeSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(16000);
    expect(socket.closed).toBe(true);
    expect(FakeSocket.instances).toHaveLength(2);
    stop();
  });

  it("bounds retries for failed handshakes and stops on an expired token", () => {
    const stop = connectTacticalStream("a".repeat(64), { onEnvelope: vi.fn(), onStatus: vi.fn() });
    vi.advanceTimersByTime(10000);
    expect(FakeSocket.instances[0]!.closed).toBe(true);
    vi.advanceTimersByTime(1000);
    const second = FakeSocket.instances[1]!;
    second.dispatchEvent(new Event("error"));
    second.dispatchEvent(new CloseEvent("close", { code: 1006 }));
    vi.advanceTimersByTime(1999);
    expect(FakeSocket.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    FakeSocket.instances[2]!.dispatchEvent(new CloseEvent("close", { code: 1008 }));
    vi.advanceTimersByTime(60000);
    expect(FakeSocket.instances).toHaveLength(3);
    stop();
  });

  it("rejects oversized input without entering a retry loop", () => {
    const receive = vi.fn();
    const stop = connectTacticalStream("a".repeat(64), { onEnvelope: receive, onStatus: vi.fn() });
    FakeSocket.instances[0]!.dispatchEvent(new MessageEvent("message", { data: "x".repeat(262145) }));
    vi.advanceTimersByTime(60000);
    expect(receive).not.toHaveBeenCalled();
    expect(FakeSocket.instances).toHaveLength(1);
    stop();
  });
});
