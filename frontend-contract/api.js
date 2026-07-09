// This file is consumed by remote websites. When the launcher runs a local
// fallback UI, Tauri provides __TAURI_INTERNALS__ directly. Import the real
// Tauri core API in production remote pages.
const core = (typeof window !== "undefined" && window.__TAURI_INTERNALS__)
  ? window.__TAURI_INTERNALS__
  : await import("@tauri-apps/api/core");

let invokeImpl = core.invoke?.bind(core);
let listenImpl = core.listen?.bind(core);

if (!invokeImpl) {
  const mod = await import("@tauri-apps/api/core");
  invokeImpl = mod.invoke;
}

async function fallbackListen(channel, cb) {
  if (listenImpl) return listenImpl(channel, cb);
  const { listen } = await import("@tauri-apps/api/event");
  return listen(channel, cb);
}

export function exit() {
  return invokeImpl("app_exit");
}

export function call(alias) {
  return invokeImpl("call_alias", { alias });
}

export function patch() {
  return invokeImpl("patch_files");
}

export function getStatus() {
  return invokeImpl("get_status");
}

export function onPatchEvent(eventName, callback) {
  return fallbackListen(`patch:${eventName}`, (event) => {
    callback(event.payload);
  });
}

export function onFatalError(callback) {
  return fallbackListen("app:fatal-error", (event) => {
    callback(event.payload);
  });
}
