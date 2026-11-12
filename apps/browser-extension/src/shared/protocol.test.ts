import { describe, expect, it } from "vitest";
import hostHello from "../../test-fixtures/protocol/host-hello.json";
import hello from "../../test-fixtures/protocol/hello.json";
import zoneEnter from "../../test-fixtures/protocol/zone-enter.json";
import helloAck from "../../test-fixtures/protocol/hello-ack.json";
import zoneApplied from "../../test-fixtures/protocol/zone-applied.json";
import error from "../../test-fixtures/protocol/error.json";
import zoneReleased from "../../test-fixtures/protocol/zone-released.json";
import bridgeStatus from "../../test-fixtures/protocol/bridge-status.json";
import { isServerMessage, PROTOCOL_VERSION, type ClientMessage, type ServerMessage } from "./protocol";

// The same fixtures the Rust tests round-trip. A field renamed on one side fails here or there.
describe("protocol fixtures", () => {
  it("client messages have the shapes the host and Desqu expect", () => {
    const client: ClientMessage[] = [
      hostHello as unknown as ClientMessage,
      hello as ClientMessage,
      zoneEnter as ClientMessage,
    ];
    expect(client.map((m) => m.type)).toEqual(["hostHello", "hello", "zoneEnter"]);
    expect(PROTOCOL_VERSION).toBe(1);
  });

  it("server messages are recognised by their type", () => {
    const server: ServerMessage[] = [
      helloAck as ServerMessage,
      zoneApplied as ServerMessage,
      error as ServerMessage,
      zoneReleased as ServerMessage,
      bridgeStatus as ServerMessage,
    ];
    for (const message of server) {
      expect(isServerMessage(message)).toBe(true);
    }
  });

  it("unknown or missing types are not server messages", () => {
    expect(isServerMessage({ type: "nope" })).toBe(false);
    expect(isServerMessage({})).toBe(false);
    expect(isServerMessage(null)).toBe(false);
  });

  it("error codes are the names Desqu sends", () => {
    const codes = ["unsupportedProtocol", "windowNotFound", "desquUnavailable"];
    expect(codes).toContain((error as { code: string }).code);
  });
});
