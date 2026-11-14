import init, {
  worker_server_init,
  worker_server_load,
  worker_server_tick,
  worker_server_set_block,
  worker_server_break_block,
  worker_server_set_game_mode,
  worker_server_set_paused,
  worker_server_save,
  worker_server_take_section_updates,
  worker_server_loaded_sections,
} from "./pkg/eaglerrust_client.js";

const TICK_INTERVAL_MS = 50;
const VIEW_DISTANCE = 1;
let initialized = false;
let paused = false;
let hasPlayerState = false;
let timer = null;
let player = { x: 0.5, y: 80, z: 0.5, yaw: 0, pitch: 0, onGround: false };
let knownSections = new Set();
function sectionSet(values) {
  const result = new Set();
  for (let i = 0; i + 2 < values.length; i += 3) result.add([values[i], values[i + 1], values[i + 2]].join(","));
  return result;
}
function publishSectionDeltas(requestId = undefined) {
  const updates = worker_server_take_section_updates();
  const current = sectionSet(worker_server_loaded_sections());
  const removed = [];
  for (const key of knownSections) {
    if (!current.has(key)) removed.push(...key.split(",").map(Number));
  }
  knownSections = current;
  if (updates.length || removed.length) {
    const removedCoordinates = Int32Array.from(removed);
    send("section-deltas", { requestId, updates, removed: removedCoordinates },
      [updates.buffer, removedCoordinates.buffer]);
  }
}

function send(type, payload = {}, transfer = []) {
  self.postMessage({ type, ...payload }, transfer);
}

function sendSnapshot(type, requestId = undefined) {
  const snapshot = worker_server_save();
  send(type, { requestId, snapshot }, [snapshot.buffer]);
}

function fail(requestId, error) {
  send("error", {
    requestId,
    message: error instanceof Error ? error.message : String(error),
  });
}

async function initialize(message) {
  if (initialized) throw new Error("Worker server is already initialized");
  await init();
  if (message.save instanceof Uint8Array || message.save instanceof ArrayBuffer) {
    const bytes = message.save instanceof Uint8Array
      ? message.save
      : new Uint8Array(message.save);
    worker_server_load(bytes, VIEW_DISTANCE);
  } else {
    const seed = Number.isInteger(message.seed) ? message.seed >>> 0 : 0xEA612026;
    worker_server_init(seed, VIEW_DISTANCE);
  }
  initialized = true;
  knownSections = sectionSet(worker_server_loaded_sections());
  worker_server_take_section_updates(); // Initial world is sent once in the ready snapshot.
  sendSnapshot("ready", message.requestId);
  timer = setInterval(runTick, TICK_INTERVAL_MS);
}

function runTick() {
  if (!initialized || paused || !hasPlayerState) return;
  try {
    const result = worker_server_tick(
      player.x, player.y, player.z, player.yaw, player.pitch, player.onGround,
    );
    if (result.worldChanged) publishSectionDeltas();
    if (result.autosaveDue) {
      const snapshot = worker_server_save();
      send("world-snapshot", { snapshot, tickCount: result.tickCount,
        worldTime: result.worldTime, dayTime: result.dayTime, autosaveDue: true }, [snapshot.buffer]);
    }
  } catch (error) {
    paused = true;
    try { worker_server_set_paused(true); } catch (_) {}
    fail(undefined, error);
  }
}

self.addEventListener("message", async (event) => {
  const message = event.data;
  if (!message || typeof message !== "object" || typeof message.type !== "string") {
    fail(undefined, "Invalid worker message");
    return;
  }

  const requestId = message.requestId;
  try {
    switch (message.type) {
      case "init":
        await initialize(message);
        break;
      case "player-state":
        if (!initialized) throw new Error("Worker server is not initialized");
        for (const key of ["x", "y", "z", "yaw", "pitch"]) {
          if (!Number.isFinite(message.player?.[key])) {
            throw new Error("Invalid player state");
          }
        }
        player = {
          x: message.player.x,
          y: message.player.y,
          z: message.player.z,
          yaw: message.player.yaw,
          pitch: message.player.pitch,
          onGround: Boolean(message.player.onGround),
        };
        hasPlayerState = true;
        break;
      case "block-edit": {
        if (!initialized) throw new Error("Worker server is not initialized");
        const { x, y, z, state } = message;
        if (![x, y, z, state].every(Number.isInteger)) throw new Error("Invalid block edit");
        let changed;
        try {
          changed = worker_server_set_block(x, y, z, state);
        } catch (error) {
          sendSnapshot("edit-result", requestId); // Roll back the renderer's optimistic edit.
          throw error;
        }
        if (changed) {
          publishSectionDeltas(requestId);
          send("edit-result", { requestId, changed: true });
        } else sendSnapshot("edit-result", requestId);
        break;
      }
      case "block-break": {
        if (!initialized) throw new Error("Worker server is not initialized");
        const { x, y, z } = message;
        if (![x, y, z].every(Number.isInteger)) throw new Error("Invalid block break");
        let changed;
        try {
          changed = worker_server_break_block(x, y, z);
        } catch (error) {
          sendSnapshot("edit-result", requestId); // Restore the authoritative section after rejection.
          throw error;
        }
        if (changed) {
          publishSectionDeltas(requestId);
          send("edit-result", { requestId, changed: true });
        } else sendSnapshot("edit-result", requestId);
        break;
      }
      case "game-mode":
        if (!initialized) throw new Error("Worker server is not initialized");
        worker_server_set_game_mode(message.mode);
        send("game-mode-result", { requestId, mode: message.mode });
        break;
      case "pause":
        if (!initialized) throw new Error("Worker server is not initialized");
        paused = Boolean(message.paused);
        worker_server_set_paused(paused);
        send("pause-result", { requestId, paused });
        break;
      case "save":
        if (!initialized) throw new Error("Worker server is not initialized");
        sendSnapshot("save-result", requestId);
        break;
      case "shutdown":
        if (timer !== null) clearInterval(timer);
        timer = null;
        initialized = false;
        send("shutdown-result", { requestId });
        self.close();
        break;
      default:
        throw new Error("Unknown worker message type: " + message.type);
    }
  } catch (error) {
    fail(requestId, error);
  }
});
