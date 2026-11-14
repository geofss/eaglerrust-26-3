#!/usr/bin/env node
// External Eagler V5 smoke probe. Runs on a GitHub-hosted runner, not the build sandbox.
const endpoint = process.env.EAGLER_WS_URL || "wss://8b4t.org";
const username = "RustProbe";
const timeoutMs = 25000;

const u16 = (n) => [n >> 8 & 255, n & 255];
const ascii = (s) => [Buffer.byteLength(s), ...Buffer.from(s, "ascii")];
const frame = [
  0x01, 0x02, ...u16(1), ...u16(5), ...u16(1), ...u16(777),
  ...ascii("EaglerRust"), ...ascii("26.3"), 0, ...ascii(username)
];

function readU16(b, p) {
  if (p + 2 > b.length) throw new Error("truncated u16");
  return [b.readUInt16BE(p), p + 2];
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
      console.log(`SERVER_VERSION eagler=${eagler} java=${java} brand='${brand}' version='${version}' auth=${auth} saltBytes=${saltLen}`);
      if (eagler !== 5 || java !== 777) throw new Error(`incompatible protocol: expected Eagler 5 / Java 777, got ${eagler} / ${java}`);
      stage = "allow-login";
      send(ws, [0x04, ...ascii(username), ...ascii(""), 0, 0, 0, 0, 0], "CLIENT_REQUEST_LOGIN");
      return;
    }
    if (stage === "allow-login") {
      if (id !== 0x05) throw new Error(`expected ALLOW_LOGIN 0x05, got 0x${id.toString(16)}`);
      let p = 1, name;
      [name, p] = readAscii(data, p);
      if (p + 16 > data.length) throw new Error("ALLOW_LOGIN missing UUID");
      const uuid = data.subarray(p, p + 16).toString("hex");
      console.log(`ALLOW_LOGIN name='${name}' uuid=${uuid}`);
      stage = "server-finish";
      send(ws, [0x08], "CLIENT_FINISH_LOGIN");
      return;
    }
    if (stage === "server-finish") {
      if (id !== 0x09) throw new Error(`expected SERVER_FINISH_LOGIN 0x09, got 0x${id.toString(16)}`);
      console.log("SERVER_FINISH_LOGIN accepted; Eagler V5 login completed.");
      stage = "configuration";
      // Vanilla Login Acknowledged (packet id 0x03, VarInt) releases backend configuration.
      send(ws, [0x03], "LOGIN_ACKNOWLEDGED");
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
