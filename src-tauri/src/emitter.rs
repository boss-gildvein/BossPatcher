use launcher_core::patch::{
    PatchChecking, PatchEmitter, PatchErrorEvent, PatchFileCompleted, PatchFileProgress,
    PatchFileStarted, PatchResult, PatchWarning,
};
use launcher_core::PatchPlan;
use tauri::{Emitter, Window};

pub struct TauriPatchEmitter {
    window: Window,
}

impl TauriPatchEmitter {
    pub fn new(window: Window) -> Self {
        Self { window }
    }
}

impl PatchEmitter for TauriPatchEmitter {
    fn emit_started(&mut self) {
        let _ = self.window.emit("patch:started", serde_json::json!({"status": "started"}));
    }

    fn emit_manifest_downloaded(&mut self) {
        let _ = self
            .window
            .emit("patch:manifest-downloaded", serde_json::json!({"status": "manifest-downloaded"}));
    }

    fn emit_checking(&mut self, payload: PatchChecking) {
        let _ = self.window.emit("patch:checking", payload);
    }

    fn emit_plan_ready(&mut self, plan: PatchPlan) {
        let _ = self.window.emit("patch:plan-ready", plan);
    }

    fn emit_file_started(&mut self, payload: PatchFileStarted) {
        let _ = self.window.emit("patch:file-started", payload);
    }

    fn emit_file_progress(&mut self, payload: PatchFileProgress) {
        let _ = self.window.emit("patch:file-progress", payload);
    }

    fn emit_file_completed(&mut self, payload: PatchFileCompleted) {
        let _ = self.window.emit("patch:file-completed", payload);
    }

    fn emit_warning(&mut self, warning: &PatchWarning) {
        let _ = self.window.emit("patch:warning", warning);
    }

    fn emit_error(&mut self, error: PatchErrorEvent) {
        let _ = self.window.emit("patch:error", error);
    }

    fn emit_completed(&mut self, result: PatchResult) {
        let _ = self.window.emit("patch:completed", result);
    }
}
