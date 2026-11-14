#!/usr/bin/env node
// External Eagler V5 smoke probe. Runs on a GitHub-hosted runner, not the build sandbox.
const endpoint = process.env.EAGLER_WS_URL || "wss://8b4t.org";
const username = "EaglerRustCheck";
const minecraftProtocol = Number(process.env.EAGLER_MC_PROTOCOL || "774");
if (![769, 773, 774, 777].includes(minecraftProtocol)) throw new Error(`unsupported EAGLER_MC_PROTOCOL: ${minecraftProtocol}`);
const timeoutMs = 60000;

const u16 = (n) => [n >> 8 & 255, n & 255];
const ascii = (s) => [Buffer.byteLength(s), ...Buffer.from(s, "ascii")];
const frame = [
  0x01, 0x02, ...u16(3), ...u16(3), ...u16(4), ...u16(5), ...u16(1), ...u16(minecraftProtocol),
  ...ascii("EaglerRust"), ...ascii("26.3"), 0, ...ascii(username)
];

function readU16(b, p) {
  if (p + 2 > b.length) throw new Error("truncated u16");
  return [b.readUInt16BE(p), p + 2];
}
function readVarInt(b, p) {
  let value = 0, shift = 0;
  while (shift < 35) {
    if (p >= b.length) throw new Error("truncated VarInt");
    const byte = b[p++];
    value |= (byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) return [value >>> 0, p];
    shift += 7;
  }
  throw new Error("VarInt too wide");
}
function varInt(n) {
  const out = [];
  n >>>= 0;
  while ((n & ~0x7f) !== 0) { out.push((n & 0x7f) | 0x80); n >>>= 7; }
  out.push(n);
  return out;
}
function readAscii(b, p) {
  if (p >= b.length) throw new Error("truncated ASCII length");
  const n = b[p++];
  if (p + n > b.length) throw new Error("truncated ASCII value");
  return [b.subarray(p, p + n).toString("ascii"), p + n];
}
function packetId(data) {
  if (!data.length) throw new Error("empty server message");
  return data[0];
}
function send(ws, bytes, label) {
  console.log(`TX ${label}: ${Buffer.from(bytes).toString("hex")}`);
  ws.send(Buffer.from(bytes));
}
function javaPacket(ws, id, payload, label) {
  send(ws, [...varInt(id), ...payload], label);
}
function javaString(value) {
  const bytes = Buffer.from(value, "utf8");
  return [...varInt(bytes.length), ...bytes];
}
function clientInformationPayload() {
  return [
    ...javaString("en_us"),
    8,             // view distance
    ...varInt(0),  // chat mode: enabled
    1,             // chat colors
    0x7f,          // all skin layers
    ...varInt(1),  // main hand: right
    0,             // text filtering disabled (offline profile)
    1,             // allow server listings
    ...varInt(0)   // particle status: all
  ];
}

const ws = new WebSocket(endpoint);
ws.binaryType = "arraybuffer";
let stage = "open";
let done = false;
const timer = setTimeout(() => fail(new Error(`timeout during ${stage}`)), timeoutMs);

function finish(message) {
  if (done) return;
  done = true;
  clearTimeout(timer);
  console.log(message);
  try { ws.close(1000, "probe complete"); } catch {}
}
function fail(error) {
  if (done) return;
  done = true;
  clearTimeout(timer);
  console.error(`FAIL [${stage}]: ${error?.stack || error}`);
  try { ws.close(); } catch {}
  process.exitCode = 1;
}

ws.addEventListener("open", () => {
  console.log(`OPEN endpoint=${endpoint} protocol='${ws.protocol}' extensions='${ws.extensions}'`);
  stage = "server-version";
  send(ws, frame, `Eagler V5 CLIENT_VERSION (protocol 5 / Java ${minecraftProtocol})`);
});
ws.addEventListener("message", (event) => {
  try {
    const data = Buffer.from(event.data);
    console.log(`RX stage=${stage} bytes=${data.length} head=${data.subarray(0, 16).toString("hex")}`);
    const id = packetId(data);
    if (stage === "server-version") {
      if (id !== 0x02) throw new Error(`expected SERVER_VERSION 0x02, got 0x${id.toString(16)}`);
      let p = 1, eagler, java, brand, version, auth, saltLen;
      [eagler, p] = readU16(data, p);
      [java, p] = readU16(data, p);
      [brand, p] = readAscii(data, p);
      [version, p] = readAscii(data, p);
      if (p >= data.length) throw new Error("missing auth method");
      auth = data[p++];
      [saltLen, p] = readU16(data, p);
      if (p + saltLen > data.length) throw new Error("truncated salt");
      p += saltLen;
      if (p >= data.length) throw new Error("missing V5 nickname-selection flag");
      const nicknameSelection = data[p++] !== 0;
      if (p !== data.length) throw new Error(`unexpected SERVER_VERSION trailing bytes: ${data.length - p}`);
      console.log(`SERVER_VERSION eagler=${eagler} java=${java} brand='${brand}' version='${version}' auth=${auth} saltBytes=${saltLen} nicknameSelection=${nicknameSelection}`);
      if (eagler !== 5 || java !== minecraftProtocol) throw new Error(`incompatible protocol: requested Eagler 5 / Java ${minecraftProtocol}, server selected Eagler ${eagler} / Java ${java}`);
      stage = "backend-allow-login";
      const standardCaps = (1 << 2) | (1 << 3) | (1 << 4) | (1 << 6); // REDIRECT, NOTIFICATION, PAUSE_MENU, COOKIE
      send(ws, [
        0x04,
        ...(nicknameSelection ? ascii(username) : ascii("")),
        ...ascii("default"),      // HandshakerInstance requests the default backend
        0,                        // empty auth password
        1,                        // enableCookies = true; no prior cookie is available
        0,                        // cookie length = 0 (request cookie support without presenting a token)
        ...varInt(standardCaps),  // standard capability bitmask
        ...varInt(1),             // REDIRECT supports version 0 (bit 0)
        ...varInt(1),             // NOTIFICATION supports version 0 (bit 0)
        ...varInt(1),             // PAUSE_MENU supports version 0 (bit 0)
        ...varInt(1),             // COOKIE supports version 0 (bit 0)
        0                         // zero extended capabilities
      ], `EAGLER V5 CLIENT_REQUEST_LOGIN (username=${username}, cookies=enabled, cookieBytes=0)`);
      return;
    }
    if (stage === "backend-allow-login") {
      if (id === 0xff) {
        const code = data.length >= 2 ? data[1] : -1;
        const size = data.length >= 4 ? data.readUInt16BE(2) : 0;
        const reason = data.subarray(4, 4 + size).toString("utf8");
        let detail = reason;
        try {
          const parsed = JSON.parse(reason);
          const collect = (node) => {
            if (!node || typeof node !== "object") return "";
            return (typeof node.text === "string" ? node.text : "") +
              (Array.isArray(node.extra) ? node.extra.map(collect).join("") : "");
          };
          detail = collect(parsed) || reason;
        } catch {}
        throw new Error(`Eagler server error 0x${code.toString(16)}: ${detail}`);
      }
      if (id === 0x06) throw new Error(`Eagler login denied: ${data.subarray(2).toString("utf8")}`);
      if (id !== 0x05) throw new Error(`expected Eagler SERVER_ALLOW_LOGIN 0x05, got 0x${id.toString(16)}`);
      let p = 1, name;
      [name, p] = readAscii(data, p);
      if (p + 16 > data.length) throw new Error("ALLOW_LOGIN missing UUID");
      const uuid = data.subarray(p, p + 16).toString("hex");
      p += 16;
      const [standardCaps, afterCaps] = readVarInt(data, p);
      p = afterCaps;
      const versionCount = standardCaps.toString(2).replace(/0/g, "").length;
      if (p + versionCount > data.length) throw new Error("ALLOW_LOGIN truncated standard capability versions");
      const standardCapVersions = [...data.subarray(p, p + versionCount)];
      p += versionCount;
      if (p >= data.length) throw new Error("ALLOW_LOGIN missing extended capability count");
      const extendedCaps = data[p++];
      for (let i = 0; i < extendedCaps; i++) {
        if (p + 17 > data.length) throw new Error("ALLOW_LOGIN truncated extended capability entry");
        p += 16;
        const [, next] = readVarInt(data, p);
        p = next;
      }
      if (p !== data.length) throw new Error(`ALLOW_LOGIN unexpected trailing bytes: ${data.length - p}`);
      console.log(`ALLOW_LOGIN name='${name}' uuid=${uuid} standardCaps=0x${standardCaps.toString(16)} versions=${standardCapVersions.join(",")} extendedCaps=${extendedCaps}`);
      // EaglerXServer emits SERVER_ALLOW_LOGIN only after its backend login succeeds.
      // Exercise the V5 profile-data envelope and finish-login transition too.
      const profileEntries = [
        ["brand_uuid_v1", [0xfa,0xf2,0x3e,0x85,0x71,0x56,0x31,0xbd,0x97,0xee,0xb4,0x0e,0x59,0x9e,0xf7,0xb6]],
        ["skin_v2", [0x01,0,0,0,0]],
        ["cape_v1", [0x01,0,0,0,0]],
        ["update_cert_v1", []]
      ];
      const profile = [0x07, profileEntries.length];
      for (const [key, value] of profileEntries) profile.push(...ascii(key), ...u16(value.length), ...value);
      stage = "server-finish";
      send(ws, profile, "EAGLER V5 CLIENT_PROFILE_DATA");
      send(ws, [0x08], "EAGLER V5 CLIENT_FINISH_LOGIN");
      return;
    }
    if (stage === "server-finish") {
      if (id === 0xff) {
        const code = data.length >= 2 ? data[1] : -1;
        const size = data.length >= 4 ? data.readUInt16BE(2) : 0;
        throw new Error("Eagler server error 0x" + code.toString(16) + ": " + data.subarray(4, 4 + size).toString("utf8"));
      }
      if (id !== 0x09) throw new Error("expected Eagler SERVER_FINISH_LOGIN 0x09, got 0x" + id.toString(16));
      console.log("SERVER_FINISH_LOGIN received; proxy has completed the Eagler handshake.");
      // The proxy sends Java Login Acknowledged to its backend internally.
      // The Eagler client must NOT send packet 0x03 here.
      stage = "configuration";
      javaPacket(ws, 0x00, clientInformationPayload(), "Java CONFIGURATION client_information");
      javaPacket(ws, 0x02, [...javaString("minecraft:brand"), ...javaString("EaglerRust-26.3")], "Java CONFIGURATION brand");
      return;
    }
    if (stage === "configuration") {
      // EaglerXServer injects V5 extension frames with 0xEE before/alongside
      // vanilla Java packets. They are not Java VarInt packet IDs.
      if (id === 0xee) {
        const extensionId = data.length > 1 ? data[1] : -1;
        if (extensionId === 0x21 && data.length >= 19) {
          const uuid = data.subarray(2, 18).toString("hex");
          const [flagState, end] = readVarInt(data, 18);
          console.log(`EAGLER_EXTENSION id=0x21 ClientStateFlag uuid=${uuid} state=${flagState} trailingBytes=${data.length - end}`);
        } else {
          console.log(`EAGLER_EXTENSION id=0x${extensionId < 0 ? "??" : extensionId.toString(16)} bytes=${data.length} hex=${data.toString("hex")}`);
        }
        return;
      }
      const [javaPacketId, packetIdBytes] = readVarInt(data, 0);
      const payload = data.subarray(packetIdBytes);
      console.log(`JAVA_CONFIGURATION_PACKET id=0x${javaPacketId.toString(16)} payloadBytes=${payload.length} payloadHex=${payload.toString("hex")}`);
      if (javaPacketId === 0x02) {
        throw new Error("server sent Configuration Disconnect: " + payload.toString("hex"));
      }
      if (javaPacketId === 0x04) {
        if (payload.length !== 8) throw new Error("invalid Configuration Keep Alive length: " + payload.length);
        javaPacket(ws, 0x04, [...payload], "Java CONFIGURATION keep_alive response");
      } else if (javaPacketId === 0x05) {
        if (payload.length !== 4) throw new Error("invalid Configuration Ping length: " + payload.length);
        javaPacket(ws, 0x05, [...payload], "Java CONFIGURATION pong");
      } else if (javaPacketId === 0x0f) {
        // This probe advertises no locally-known packs, so its valid subset is empty.
        javaPacket(ws, 0x07, [0], "Java CONFIGURATION select_known_packs (empty supported subset)");
      } else if (javaPacketId === 0x03) {
        if (payload.length !== 0) throw new Error("Finish Configuration unexpectedly had a payload");
        javaPacket(ws, 0x03, [], "Java CONFIGURATION acknowledge_finish");
        stage = "play";
        console.log("CONFIGURATION_COMPLETE: acknowledged server Finish Configuration; waiting for first Play packet.");
      }
      return;
    }
    if (stage === "play") {
      if (id === 0xee) {
        console.log(`EAGLER_EXTENSION_PLAY bytes=${data.length} hex=${data.toString("hex")}`);
        return;
      }
      const [javaPacketId, packetIdBytes] = readVarInt(data, 0);
      const payload = data.subarray(packetIdBytes);
      console.log(`JAVA_PLAY_PACKET id=0x${javaPacketId.toString(16)} payloadBytes=${payload.length} payloadHex=${payload.toString("hex")}`);
      finish("PASS: Eagler V5 login, Java configuration responses, Finish Configuration acknowledgement, and first Play packet received.");
    }
  } catch (error) { fail(error); }
});
ws.addEventListener("error", () => fail(new Error("WebSocket error (browser/runtime may hide the underlying TLS or network reason)")));
ws.addEventListener("close", (event) => {
  if (!done) {
    const context = stage === "backend-allow-login"
      ? " (SERVER_ALLOW_LOGIN was never reached; EaglerXServer performs its backend login before sending it)"
      : "";
    fail(new Error("closed before completion" + context + ": code=" + event.code + " reason='" + event.reason + "' clean=" + event.wasClean));
  }
});
