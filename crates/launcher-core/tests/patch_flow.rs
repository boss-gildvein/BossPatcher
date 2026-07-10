use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use launcher_core::config::{Config, PatchConfig, SecurityConfig};
use launcher_core::patch::Patcher;
use launcher_core::patch::{
    PatchChecking, PatchEmitter, PatchErrorEvent, PatchFileCompleted, PatchFileProgress,
    PatchFileStarted, PatchPlan, PatchResult, PatchWarning,
};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Spins up a tiny local HTTP server serving manifest + data and validates that
/// the Patcher downloads only expected files, protects the launcher/config,
/// and leaves extra local files untouched.
#[tokio::test]
async fn integration_patch_flow() {
    let test_files = prepare_remote_files().await;
    let manifest = build_manifest(&test_files).await;

    let server = TestServer::start(test_files, manifest).await;

    let base = tempfile::tempdir().unwrap();
    let launcher_dir = base.path().join("launcher");
    let exe_path = launcher_dir.join("BossPatcher_test.exe");
    let config_path = launcher_dir.join("BossPatcher_test.toml");

    tokio::fs::create_dir_all(&launcher_dir).await.unwrap();
    tokio::fs::write(&exe_path, b"launcher stub").await.unwrap();
    tokio::fs::write(&config_path, b"config stub")
        .await
        .unwrap();

    let local_data = launcher_dir.join("data");
    tokio::fs::create_dir_all(&local_data).await.unwrap();
    tokio::fs::write(local_data.join("client.grf"), b"old data")
        .await
        .unwrap();
    tokio::fs::write(launcher_dir.join("local_only.txt"), b"this stays")
        .await
        .unwrap();

    let config = Config {
        config_version: 1,
        title: "Integration".into(),
        launcher_url: format!("http://{}/", server.addr),
        manifest_url: format!("http://{}/manifest.toml", server.addr),
        data_url: format!("http://{}/data/", server.addr),
        calls: [("game".into(), "Game.exe".into())].into_iter().collect(),
        call_options: Default::default(),
        window: Default::default(),
        patch: PatchConfig::default(),
        security: SecurityConfig { allow_http: true },
    };

    let emitter: Arc<Mutex<CollectingEmitter>> = Arc::new(Mutex::new(CollectingEmitter::default()));
    let patcher = Patcher::new();
    let result = patcher
        .run(
            &launcher_dir,
            &exe_path,
            &config_path,
            &config,
            emitter.clone(),
        )
        .await
        .expect("patch should succeed");

    assert_eq!(result.status, "completed");
    // 7 manifest entries - 2 protected skipped = 5 checked
    assert_eq!(result.checked_files, 5);
    // client.grf changed + 3 missing Unicode files + extra_remote.txt = 5 downloads
    assert_eq!(result.files_patched, 5);

    let client_content = tokio::fs::read_to_string(local_data.join("client.grf"))
        .await
        .unwrap();
    assert_eq!(client_content, "Hello from client.grf");

    assert!(
        tokio::fs::try_exists(local_data.join("한국어").join("스프라이트.spr"))
            .await
            .unwrap()
    );
    assert!(
        tokio::fs::try_exists(launcher_dir.join("BGM").join("日本語").join("track 01.mp3"))
            .await
            .unwrap()
    );
    assert!(
        tokio::fs::try_exists(local_data.join("简体中文").join("说明.txt"))
            .await
            .unwrap()
    );
    assert!(tokio::fs::try_exists(launcher_dir.join("extra_remote.txt"))
        .await
        .unwrap());

    assert_eq!(
        tokio::fs::read_to_string(launcher_dir.join("local_only.txt"))
            .await
            .unwrap(),
        "this stays"
    );

    assert_eq!(
        tokio::fs::read_to_string(&exe_path).await.unwrap(),
        "launcher stub"
    );
    assert_eq!(
        tokio::fs::read_to_string(&config_path).await.unwrap(),
        "config stub"
    );

    let warnings = emitter.lock().await.warnings.clone();
    assert_eq!(warnings.len(), 2, "expected 2 protected-file warnings");
}

#[tokio::test]
async fn integration_patch_flow_can_be_cancelled_mid_download() {
    let test_files = vec![RemoteFile {
        path: "data/client.grf".into(),
        contents: vec![b'x'; 512 * 1024],
    }];
    let manifest = build_manifest(&test_files).await;

    let server = TestServer::start_slow(test_files, manifest, Duration::from_millis(250)).await;

    let base = tempfile::tempdir().unwrap();
    let launcher_dir = base.path().join("launcher");
    let exe_path = launcher_dir.join("BossPatcher_test.exe");
    let config_path = launcher_dir.join("BossPatcher_test.toml");

    tokio::fs::create_dir_all(&launcher_dir).await.unwrap();
    tokio::fs::write(&exe_path, b"launcher stub").await.unwrap();
    tokio::fs::write(&config_path, b"config stub").await.unwrap();

    let config = Config {
        config_version: 1,
        title: "Integration".into(),
        launcher_url: format!("http://{}/", server.addr),
        manifest_url: format!("http://{}/manifest.toml", server.addr),
        data_url: format!("http://{}/data/", server.addr),
        calls: [("game".into(), "Game.exe".into())].into_iter().collect(),
        call_options: Default::default(),
        window: Default::default(),
        patch: PatchConfig::default(),
        security: SecurityConfig { allow_http: true },
    };

    let emitter: Arc<Mutex<CollectingEmitter>> = Arc::new(Mutex::new(CollectingEmitter::default()));
    let patcher = Arc::new(Patcher::new());
    let patcher_for_task = patcher.clone();
    let emitter_for_task = emitter.clone();

    let task = tokio::spawn(async move {
        patcher_for_task
            .run(
                &launcher_dir,
                &exe_path,
                &config_path,
                &config,
                emitter_for_task,
            )
            .await
    });

    tokio::time::timeout(Duration::from_secs(1), async {
        while !patcher.is_running() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("patcher should start running");

    assert!(patcher.cancel(), "cancel should be accepted while patching");

    let error = task.await.unwrap().expect_err("patch should cancel");
    assert!(matches!(error, launcher_core::Error::DownloadCancelled));
}

#[derive(Default)]
struct CollectingEmitter {
    warnings: Vec<String>,
    errors: Vec<String>,
    completed: Option<PatchResult>,
}

impl PatchEmitter for CollectingEmitter {
    fn emit_started(&mut self) {}
    fn emit_manifest_downloaded(&mut self) {}
    fn emit_checking(&mut self, _payload: PatchChecking) {}
    fn emit_plan_ready(&mut self, _plan: PatchPlan) {}
    fn emit_file_started(&mut self, _payload: PatchFileStarted) {}
    fn emit_file_progress(&mut self, _payload: PatchFileProgress) {}
    fn emit_file_completed(&mut self, _payload: PatchFileCompleted) {}
    fn emit_warning(&mut self, warning: &PatchWarning) {
        self.warnings.push(format!("{:?}", warning));
    }
    fn emit_error(&mut self, error: PatchErrorEvent) {
        self.errors.push(error.code);
    }
    fn emit_completed(&mut self, result: PatchResult) {
        self.completed = Some(result);
    }
}

struct RemoteFile {
    path: String,
    contents: Vec<u8>,
}

async fn prepare_remote_files() -> Vec<RemoteFile> {
    vec![
        RemoteFile {
            path: "data/client.grf".into(),
            contents: b"Hello from client.grf".to_vec(),
        },
        RemoteFile {
            path: "data/한국어/스프라이트.spr".into(),
            contents: b"Korean asset".to_vec(),
        },
        RemoteFile {
            path: "BGM/日本語/track 01.mp3".into(),
            contents: b"Japanese bgm".to_vec(),
        },
        RemoteFile {
            path: "data/简体中文/说明.txt".into(),
            contents: b"Chinese text".to_vec(),
        },
        RemoteFile {
            path: "extra_remote.txt".into(),
            contents: b"Extra remote".to_vec(),
        },
        RemoteFile {
            path: "BossPatcher_test.exe".into(),
            contents: b"Protected launcher".to_vec(),
        },
        RemoteFile {
            path: "BossPatcher_test.toml".into(),
            contents: b"Protected config".to_vec(),
        },
    ]
}

async fn build_manifest(files: &[RemoteFile]) -> String {
    let entries: Vec<String> = files
        .iter()
        .map(|f| {
            format!(
                "[[files]]\npath = {:?}\nsize = {}\nmd5 = \"{}\"\n",
                f.path,
                f.contents.len(),
                md5_hex(&f.contents)
            )
        })
        .collect();
    format!(
        "manifest_version = 1\nhash_algorithm = \"md5\"\ngenerated_at = \"2026-07-09T00:00:00Z\"\n\n{}\n",
        entries.join("\n")
    )
}

fn md5_hex(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

struct TestServer {
    addr: SocketAddr,
    _files: Arc<Vec<RemoteFile>>,
    _manifest: Arc<String>,
}

impl TestServer {
    async fn start(files: Vec<RemoteFile>, manifest: String) -> Self {
        Self::start_with_chunk_delay(files, manifest, None).await
    }

    async fn start_slow(files: Vec<RemoteFile>, manifest: String, chunk_delay: Duration) -> Self {
        Self::start_with_chunk_delay(files, manifest, Some(chunk_delay)).await
    }

    async fn start_with_chunk_delay(
        files: Vec<RemoteFile>,
        manifest: String,
        chunk_delay: Option<Duration>,
    ) -> Self {
        let files = Arc::new(files);
        let manifest = Arc::new(manifest);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let files_clone = files.clone();
        let manifest_clone = manifest.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let files = files_clone.clone();
                let manifest = manifest_clone.clone();
                let chunk_delay = chunk_delay;
                tokio::spawn(async move {
                    let service =
                        service_fn(move |req| handle_request(req, files.clone(), manifest.clone(), chunk_delay));
                    let _ = http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                        .await;
                });
            }
        });

        Self {
            addr,
            _files: files,
            _manifest: manifest,
        }
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    files: Arc<Vec<RemoteFile>>,
    manifest: Arc<String>,
    chunk_delay: Option<Duration>,
) -> Result<Response<http_body_util::combinators::BoxBody<Bytes, Infallible>>, Infallible> {
    let path = urlencoding::decode(req.uri().path())
        .unwrap_or_else(|_| req.uri().path().into())
        .into_owned();
    let path = path.trim_start_matches('/');
    if path == "manifest.toml" {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/toml")
            .body(Full::new(Bytes::copy_from_slice(manifest.as_bytes())).boxed())
            .unwrap());
    }
    let prefix = "data/";
    if let Some(rel) = path.strip_prefix(prefix) {
        if let Some(file) = files.iter().find(|f| f.path == rel) {
            if let Some(delay) = chunk_delay {
                let midpoint = (file.contents.len() / 2).max(1);
                let first = Bytes::copy_from_slice(&file.contents[..midpoint]);
                let second = Bytes::copy_from_slice(&file.contents[midpoint..]);
                let stream = async_stream::stream! {
                    yield Ok::<_, Infallible>(hyper::body::Frame::data(first));
                    tokio::time::sleep(delay).await;
                    yield Ok::<_, Infallible>(hyper::body::Frame::data(second));
                };
                let body = http_body_util::StreamBody::new(stream);
                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/octet-stream")
                    .body(body.boxed())
                    .unwrap());
            }

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/octet-stream")
                .body(Full::new(Bytes::copy_from_slice(&file.contents)).boxed())
                .unwrap());
        }
    }
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::new()).boxed())
        .unwrap())
}
