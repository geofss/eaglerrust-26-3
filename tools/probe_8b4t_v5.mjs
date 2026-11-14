#!/usr/bin/env node
// External Eagler V5 smoke probe. Runs on a GitHub-hosted runner, not the build sandbox.
const endpoint = process.env.EAGLER_WS_URL || "wss://8b4t.org";
const username = "RustProbe";
const timeoutMs = 25000;

const u16 = (n) => [n >> 8 & 255, n & 255];
const ascii = (s) => [Buffer.byteLength(s), ...Buffer.from(s, "ascii")];
const frame = [
  0x01, 0x02, ...u16(3), ...u16(3), ...u16(4), ...u16(5), ...u16(1), ...u16(777),
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
  send(ws, frame, "Eagler V5 CLIENT_VERSION (protocol 5 / Java 777)");
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
      if (eagler !== 5 || java !== 777) throw new Error(`incompatible protocol: expected Eagler 5 / Java 777, got ${eagler} / ${java}`);
      stage = "allow-login";
      const standardCaps = (1 << 2) | (1 << 3) | (1 << 4); // REDIRECT, NOTIFICATION, PAUSE_MENU
      send(ws, [
        0x04,
        ...(nicknameSelection ? ascii(username) : ascii("")),
        ...ascii("default"),      // HandshakerInstance requests the default backend
        0,                        // empty auth password
        0,                        // enableCookies = false
        0,                        // cookie length
        ...varInt(standardCaps),  // standard capability bitmask
        ...varInt(1),             // REDIRECT supports version 0 (bit 0)
        ...varInt(1),             // NOTIFICATION supports version 0 (bit 0)
        ...varInt(1),             // PAUSE_MENU supports version 0 (bit 0)
        0                         // zero extended capabilities
      ], "EAGLER V5 CLIENT_REQUEST_LOGIN (source-matched V5 layout)");
      return;
    }
    if (stage === "allow-login") {
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
      // Stop here deliberately: the real client sends profile-data packets (brand UUID,
      // skin, cape, update certificate) before CLIENT_FINISH_LOGIN. This probe isolates
      // whether the corrected CLIENT_REQUEST_LOGIN is accepted without faking those assets.
      finish("PASS: Eagler V5 CLIENT_REQUEST_LOGIN accepted (SERVER_ALLOW_LOGIN received). Profile-data and finish-login stages remain untested.");
      return;
    }
    if (stage === "server-finish") {
      if (id !== 0x09) throw new Error(`expected SERVER_FINISH_LOGIN 0x09, got 0x${id.toString(16)}`);
      console.log("SERVER_FINISH_LOGIN accepted; Eagler V5 login completed.");
      stage = "configuration";
      // Vanilla Login Acknowledged (packet id 0x03, VarInt) releases backend configuration.
      finish("PASS: DNS/TLS/WebSocket + Eagler V5 protocol 5 / Java 777 + Eaglercraft login request accepted + server finish login received.");
      return;
      return;
    }
    if (stage === "configuration") {
      console.log(`CONFIGURATION_TRAFFIC observed (first packet id 0x${id.toString(16)}).`);
      finish("PASS: DNS/TLS/WebSocket + Eagler V5 protocol 5 + Java protocol 777 + login completed; configuration traffic received.");
    }
  } catch (error) { fail(error); }
});
ws.addEventListener("error", () => fail(new Error("WebSocket error (browser/runtime may hide the underlying TLS or network reason)")));
ws.addEventListener("close", (event) => {
  if (!done) fail(new Error(`closed before completion: code=${event.code} reason='${event.reason}' clean=${event.wasClean}`));
});
