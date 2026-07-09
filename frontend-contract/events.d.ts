export interface PatchStarted {
  status: "started";
}

export interface PatchChecking {
  status: "checking";
  checked_files: number;
  total_files: number;
}

export interface PatchPlanReady {
  checked_files: number;
  valid_files: number;
  missing_files: number;
  changed_files: number;
  protected_skipped: number;
  files_to_download: number;
  bytes_to_download: number;
}

export interface PatchFileStarted {
  path: string;
  file_index: number;
  file_total: number;
  file_size: number;
}

export interface PatchFileProgress {
  path: string;
  file_index: number;
  file_total: number;
  file_downloaded_bytes: number;
  file_total_bytes: number;
  total_downloaded_bytes: number;
  total_bytes: number;
}

export interface PatchFileCompleted {
  path: string;
  file_index: number;
  file_total: number;
  status: "completed" | "failed";
}

export interface PatchWarning {
  code: string;
  path: string;
  message: string;
}

export interface PatchError {
  code: string;
  path: string;
  message: string;
  retryable: boolean;
}

export interface PatchCompleted {
  status: "completed";
  checked_files: number;
  files_patched: number;
  bytes_downloaded: number;
}

export type PatchEvent =
  | { "patch:started": PatchStarted }
  | { "patch:checking": PatchChecking }
  | { "patch:plan-ready": PatchPlanReady }
  | { "patch:file-started": PatchFileStarted }
  | { "patch:file-progress": PatchFileProgress }
  | { "patch:file-completed": PatchFileCompleted }
  | { "patch:warning": PatchWarning }
  | { "patch:error": PatchError }
  | { "patch:completed": PatchCompleted };

export interface Status {
  config_loaded: boolean;
  title?: string;
  launcher_url?: string;
  aliases: string[];
  patch_running: boolean;
  last_error?: string;
}
