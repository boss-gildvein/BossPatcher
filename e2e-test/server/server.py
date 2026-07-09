#!/usr/bin/env python3
"""Local fake launcher/patch server for BossPatcher end-to-end tests."""

import os
import sys
from pathlib import Path
from flask import Flask, send_from_directory, Response

ROOT = Path(__file__).resolve().parent
DATA = ROOT / "data"

app = Flask(__name__)

UI_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>BossPatcher Test UI</title>
</head>
<body>
    <h1>BossPatcher E2E UI</h1>
    <p id="status">Idle</p>
    <button id="patchBtn">Patch</button>
    <button id="playBtn">Play</button>
    <button id="exitBtn">Exit</button>
    <pre id="log"></pre>
    <script type="module">
        // Tauri injects __TAURI_INTERNALS__ into every webview.
        const { invoke } = window.__TAURI_INTERNALS__;
        const log = (msg) => document.getElementById('log').textContent += msg + '\n';
        document.getElementById('patchBtn').onclick = async () => {
            log('Patching...');
            try {
                const result = await invoke('patch_files');
                log('Patch result: ' + JSON.stringify(result, null, 2));
            } catch (e) {
                log('Patch error: ' + JSON.stringify(e));
            }
        };
        document.getElementById('playBtn').onclick = async () => {
            log('Launching game...');
            try {
                const result = await invoke('call_alias', { alias: 'game' });
                log('Launch result: ' + JSON.stringify(result));
            } catch (e) {
                log('Launch error: ' + JSON.stringify(e));
            }
        };
        document.getElementById('exitBtn').onclick = async () => {
            log('Exiting...');
            await invoke('app_exit');
        };
    </script>
</body>
</html>
"""


@app.get("/")
def index():
    return Response(UI_HTML, mimetype="text/html")


@app.get("/manifest.toml")
def manifest():
    return send_from_directory(ROOT, "manifest.toml", mimetype="text/toml")


@app.get("/data/<path:filename>")
def data(filename: str):
    return send_from_directory(DATA, filename)


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "5000"))
    print(f"Serving BossPatcher test server on http://127.0.0.1:{port}")
    # Use threaded mode but no reloader so it behaves predictably in a test.
    app.run(host="127.0.0.1", port=port, threaded=True, use_reloader=False)
