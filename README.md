# BossPatcher

A Windows-only, Tauri v2 desktop launcher that loads a remote web-based UI from a local, filename-derived TOML config. It exposes approved Rust-backed commands (`exit`, `call`, `patch`), enforces filesystem boundaries, and patches files from a remote manifest without deleting extra local files.

## Workspace

```text
BossPatcher/
  crates/
    launcher-core/     Shared config, manifest, path validation, hashing, patching
    manifestgen/       CLI tool to generate manifest.toml from a folder
  src-tauri/           Tauri v2 app (Rust backend + window setup)
  frontend-contract/   JavaScript API examples and event definitions
```

## Prerequisites

- Windows
- Rust 1.78+ (`rustup update`)
- Optional: `cargo node` / Node only if you later bundle additional frontend assets

## Build

```powershell
cargo build -p bosspatcher-launcher --release
cargo build -p manifestgen --release
```

Outputs:

- `target/release/bosspatcher-launcher.exe` (rename as desired)
- `target/release/manifestgen.exe`

## Run tests

```powershell
cargo test --workspace
```

## Launcher config

The launcher derives its config filename from its executable filename. For `BossPatcher.exe`, create `BossPatcher.toml` in the same directory:

```toml
config_version = 1

title = "BossPatcher Launcher"
launcher_url = "https://launcher.example.com/"
manifest_url = "https://patch.example.com/manifest.toml"
data_url = "https://patch.example.com/data/"

[calls]
game = "Game.exe"
setup = "Setup.exe"

[security]
# Defaults to false. Use true only for local development fixtures.
allow_http = false

[patch]
max_concurrent_downloads = 3
verify_after_download = true
resume_downloads = false
hash_algorithm = "md5"
```

### Mandatory fields

| Field | Purpose |
|-------|---------|
| `config_version` | Must be `1` |
| `title` | Window title |
| `launcher_url` | HTTPS URL of the remote launcher UI origin |
| `manifest_url` | HTTPS URL where the patch manifest is fetched |
| `data_url` | HTTPS base URL for patch file downloads |
| `[calls]` | Alias → relative executable path map |

### Local HTTP opt-in

Production configs must use HTTPS by default. For local manual testing, set:

```toml
[security]
allow_http = true
```

When `allow_http = false` or the section is omitted, any HTTP `launcher_url`, `manifest_url`, or `data_url` fails config validation before the UI or patcher starts. The `--headless-patch` diagnostic mode uses the same TOML validation path as the Tauri app.

### Security rules enforced by Rust

- Only aliases defined in `[calls]` can be launched. `call("game")` resolves to the configured path; raw paths from JavaScript are rejected.
- Aliases must be relative, must stay inside the launcher directory, and cannot contain `..`.
- Only files listed in the remote manifest can be modified. Extra local files are never deleted.
- The running launcher executable and its config TOML are protected and skipped with a warning if they appear in the manifest.
- Absolute manifest paths, traversal, and case-insensitive duplicates are rejected.
- Remote UI command access is controlled by Tauri v2 capabilities. The app grants only the registered launcher commands (`app_exit`, `call_alias`, `get_status`, `patch_files`) to the configured main remote webview.

## Frontend JavaScript API

```javascript
import { exit, call, patch, getStatus, onPatchEvent, onFatalError } from "./api.js";

// Close the launcher (blocked while patching)
await exit();

// Launch a configured alias
await call("game");

// Apply patch from the configured remote manifest
await patch();

// Inspect launcher state
const status = await getStatus();

// Listen to patch lifecycle events
onPatchEvent("started", (payload) => { /* ... */ });
onPatchEvent("plan-ready", (payload) => { /* files_to_download, bytes_to_download */ });
onPatchEvent("file-started", (payload) => { /* file_index, file_total, file_size */ });
onPatchEvent("file-progress", (payload) => { /* file and total byte progress */ });
onPatchEvent("file-completed", (payload) => { /* ... */ });
onPatchEvent("warning", (payload) => { /* ... */ });
onPatchEvent("error", (payload) => { /* ... */ });
onPatchEvent("completed", (payload) => { /* ... */ });
```

## Patch workflow

1. Download `manifest_url`.
2. Validate `manifest_version` and `hash_algorithm`.
3. Reject invalid/duplicate paths.
4. Compare manifest entries against local files (missing → size mismatch → MD5 mismatch).
5. Emit a patch plan with counts and total bytes to download.
6. Stream-download each needed file to a temporary file.
7. Verify MD5 of the temporary file.
8. Safely rename the verified temp file over the target.
9. Emit progress, warning, error, and completion events.

## Manifest generator

```powershell
manifestgen.exe "C:\PatchRoot" --output "C:\WebRoot\manifest.toml" --verbose
```

The generator:

- Walks recursively
- Stores relative paths with forward slashes
- Preserves Unicode filenames
- Records `size` and `md5`
- Sorts entries deterministically by path
- Excludes its own output file if inside the scanned folder
- Streams large files while hashing

Example output:

```toml
manifest_version = 1
hash_algorithm = "md5"
generated_at = "2026-07-09T00:00:00Z"

[[files]]
path = "data/client.grf"
size = 4294967296
md5 = "9e107d9d372bb6826bd81d3542a419d6"
```

## License

Proprietary — all rights reserved by Boss Gildvein.
