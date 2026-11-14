#!/usr/bin/env python3
"""Deterministic Eagler V5 WebSocket smoke-test endpoint (not EaglerXServer).

Install: python -m pip install -r tools/requirements-test.txt
Run:     python tools/mock_eagler_v5_server.py
Connect the browser client to: ws://127.0.0.1:8765
"""
import asyncio
import struct

from websockets.asyncio.server import serve

HOST = "127.0.0.1"
PORT = 8765


def u16(data: bytes, offset: int):
    if offset + 2 > len(data):
        raise ValueError("truncated u16")
    return int.from_bytes(data[offset:offset + 2], "big"), offset + 2


def u8_string(data: bytes, offset: int):
    if offset >= len(data):
        raise ValueError("truncated string length")
    length = data[offset]
    offset += 1
    end = offset + length
    if end > len(data):
        raise ValueError("truncated string")
    value = data[offset:end].decode("ascii")
    return value, end


def varint(data: bytes):
    value = 0
    for index in range(5):
        if index >= len(data):
            raise ValueError("truncated VarInt")
        byte = data[index]
        value |= (byte & 0x7F) << (7 * index)
        if byte & 0x80 == 0:
            return value, index + 1
    raise ValueError("VarInt too wide")


def encode_varint(value: int):
    value &= 0xFFFFFFFF
    out = bytearray()
    while value & ~0x7F:
        out.append((value & 0x7F) | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


async def receive_binary(ws, expected: str):
    message = await asyncio.wait_for(ws.recv(), timeout=20)
    if not isinstance(message, bytes):
        raise ValueError(f"{expected}: expected binary WebSocket message")
    print(f"[mock] RX {len(message)} bytes: {message[:12].hex(' ')}")
    return message


async def expect_packet(ws, packet_id: int, expected: str):
    message = await receive_binary(ws, expected)
    actual, used = varint(message)
    if actual != packet_id:
        raise ValueError(f"{expected}: expected packet 0x{packet_id:02x}, got 0x{actual:02x}")
    return message[used:]


def parse_v5_preamble(message: bytes):
    if message[:2] != b"\x01\x02":
        raise ValueError(f"expected V5 magic 01 02, got {message[:2].hex()}")
    offset = 2
    eagler_count, offset = u16(message, offset)
    eagler_versions = []
    for _ in range(eagler_count):
        version, offset = u16(message, offset)
        eagler_versions.append(version)
    minecraft_count, offset = u16(message, offset)
    minecraft_versions = []
    for _ in range(minecraft_count):
        version, offset = u16(message, offset)
        minecraft_versions.append(version)
    brand, offset = u8_string(message, offset)
    client_version, offset = u8_string(message, offset)
    if offset >= len(message):
        raise ValueError("missing password-present flag")
    password_present = message[offset]
    offset += 1
    username, offset = u8_string(message, offset)
    if offset != len(message):
        raise ValueError("unexpected bytes after V5 preamble")
    if 5 not in eagler_versions or 777 not in minecraft_versions:
        raise ValueError(f"wrong protocol offer: Eagler={eagler_versions}, Java={minecraft_versions}")
    print(f"[mock] V5 client={brand} {client_version}, user={username}, Eagler={eagler_versions}, Java={minecraft_versions}")
    print("[mock] Java world data version 5023 is intentionally not part of this handshake")
    return username, password_present


async def handler(ws):
    try:
        hello = await receive_binary(ws, "V5 preamble")
        username, password_present = parse_v5_preamble(hello)
        if password_present != 0:
            raise ValueError("test fixture expects no password")

        # SERVER_VERSION: Eagler V5, Java protocol 777, mock brand/version,
        # no authentication method and an empty salt.
        server_version = (
            b"\x02" + struct.pack(">HH", 5, 777)
            + bytes([len(b"MockEaglerXServer")]) + b"MockEaglerXServer"
            + bytes([len(b"26.3-test")]) + b"26.3-test"
            + b"\x00\x00\x00"
        )
        await ws.send(server_version)
        print("[mock] TX SERVER_VERSION Eagler=5 Java=777")

        login = await receive_binary(ws, "CLIENT_REQUEST_LOGIN")
        if not login or login[0] != 0x04:
            raise ValueError("expected Eagler CLIENT_REQUEST_LOGIN 0x04")
        requested_name, offset = u8_string(login, 1)
        backend, offset = u8_string(login, offset)
        if offset + 3 > len(login):
            raise ValueError("truncated login options")
        password_flag, cookie_flag, cookie_length = login[offset:offset + 3]
        offset += 3
        capabilities, used = varint(login[offset:])
        offset += used
        if offset >= len(login):
            raise ValueError("missing extended capabilities")
        extended_capabilities = login[offset]
        if requested_name != username or backend or password_flag or cookie_flag or cookie_length or capabilities or extended_capabilities:
            raise ValueError("unexpected login options in no-password smoke test")

        granted_uuid = bytes.fromhex("00112233445566778899aabbccddeeff")
        await ws.send(b"\x05" + bytes([len(username)]) + username.encode("ascii") + granted_uuid)
        print("[mock] TX SERVER_ALLOW_LOGIN 0x05")
        finish = await receive_binary(ws, "CLIENT_FINISH_LOGIN")
        if finish != b"\x08":
            raise ValueError(f"expected CLIENT_FINISH_LOGIN 08, got {finish.hex()}")
        await ws.send(b"\x09")
        print("[mock] TX SERVER_FINISH_LOGIN 0x09")

        ack = await expect_packet(ws, 0x03, "Java Login Acknowledged")
        if ack:
            raise ValueError("Login Acknowledged should have an empty payload")
        print("[mock] Login -> Configuration confirmed")

        # Protocol 777 Known Packs is clientbound 0x0f; the client replies
        # with Select Known Packs (serverbound 0x07), then we send an opaque
        # registry-like message and Finish Configuration (0x03).
        await ws.send(encode_varint(0x0F) + b"\x00")
        await ws.send(encode_varint(0x7F) + b"mock-registry-sync")
        await ws.send(encode_varint(0x03))
        print("[mock] TX Known Packs 0x0f, unknown config 0x7f, Finish Configuration 0x03")

        seen = set()
        for _ in range(4):
            message = await receive_binary(ws, "Configuration response")
            packet_id, used = varint(message)
            seen.add(packet_id)
        if seen != {0x00, 0x02, 0x03, 0x07}:
            raise ValueError(f"unexpected config responses: {sorted(seen)}")
        print("[mock] Configuration -> Play confirmed (client info, brand, known packs, finish ack)")

        keepalive_payload = struct.pack(">q", 123456789)
        await ws.send(encode_varint(0x2C) + keepalive_payload)
        response = await expect_packet(ws, 0x11, "Play keepalive response")
        if response != keepalive_payload:
            raise ValueError("keepalive payload was not echoed")
        print("[mock] Play keepalive round-trip passed")

        await ws.send(encode_varint(0x1A) + b"mock test complete")
        print("[mock] Test sequence complete; server disconnect sent")
    except Exception as error:
        print(f"[mock] FAILED: {error}")
        try:
            await ws.close(code=1011, reason="mock protocol test failed")
        except Exception:
            pass


async def main():
    print(f"Eagler V5 mock endpoint listening on ws://{HOST}:{PORT}")
    print("This is a deterministic protocol fixture, not a real EaglerXServer proxy.")
    async with serve(handler, HOST, PORT, compression=None, max_size=1 << 20):
        await asyncio.Future()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n[mock] stopped")
