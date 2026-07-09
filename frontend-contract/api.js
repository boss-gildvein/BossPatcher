import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function exit() {
  return invoke("app_exit");
}

export function call(alias) {
  return invoke("call_alias", { alias });
}

export function patch() {
  return invoke("patch_files");
}

export function getStatus() {
  return invoke("get_status");
}

export function onPatchEvent(eventName, callback) {
  return listen(`patch:${eventName}`, (event) => {
    callback(event.payload);
  });
}

export function onFatalError(callback) {
  return listen("app:fatal-error", (event) => {
    callback(event.payload);
  });
}
