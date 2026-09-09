#![deny(unsafe_op_in_unsafe_fn)]

mod cache;
mod marker_icons;
mod terrain;

use axum::Router;
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ORIGIN, REFERRER_POLICY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use cache::{MARKER_CACHE_LIMIT_BYTES, TERRAIN_CACHE_LIMIT_BYTES};
use ctab_web_protocol::{
    BridgeFrame, BridgeMessage, BrowserCommand, Envelope, MAX_FRAME_BYTES, TacticalMessage,
};
use futures_util::StreamExt;
use marker_icons::{MarkerIconError, MarkerIconService};
use rust_embed::RustEmbed;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;
use terrain::{TerrainError, TerrainService};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::net::windows::named_pipe::ServerOptions;
use tokio::sync::{RwLock, broadcast};
use tokio::task::JoinHandle;

const AUTH_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_AUTH_MESSAGE_BYTES: usize = 1_024;
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);
const STARTUP_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const HEARTBEAT_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const MAX_DIAGNOSTIC_EVENTS: usize = 32;

#[derive(Debug, Clone)]
pub struct CompanionOptions {
    pub pipe_name: String,
    pub pipe_token: String,
    pub open_browser: bool,
    pub arma_root: Option<std::path::PathBuf>,
}

#[derive(Debug)]
pub struct RunningCompanion {
    pub address: SocketAddr,
    pub browser_token: String,
    server_task: JoinHandle<Result<(), std::io::Error>>,
    pipe_task: JoinHandle<Result<(), CompanionError>>,
    watchdog_task: JoinHandle<()>,
}

impl RunningCompanion {
    pub async fn wait(mut self) -> Result<(), CompanionError> {
        let result = tokio::select! {
            server = &mut self.server_task => {
                server
                    .map_err(|error| CompanionError::Task(error.to_string()))?
                    .map_err(CompanionError::Io)
            }
            pipe = &mut self.pipe_task => {
                pipe.map_err(|error| CompanionError::Task(error.to_string()))?
            }
            watchdog = &mut self.watchdog_task => {
                watchdog.map_err(|error| CompanionError::Task(error.to_string()))?;
                Ok(())
            }
        };
        self.server_task.abort();
        self.pipe_task.abort();
        self.watchdog_task.abort();
        result
    }

    pub fn stop(self) {
        self.server_task.abort();
        self.pipe_task.abort();
        self.watchdog_task.abort();
    }
}

#[derive(Debug, Error)]
pub enum CompanionError {
    #[error("invalid companion startup arguments")]
    InvalidOptions,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("background task error: {0}")]
    Task(String),
    #[error("the Windows cryptographic random source is unavailable: {0}")]
    RandomSource(String),
    #[error("terrain service startup failed")]
    Terrain,
    #[error("marker icon service startup failed")]
    MarkerIcons,
}

#[derive(Debug)]
struct AppState {
    expected_origin: String,
    browser_token: String,
    pipe_token: String,
    current_snapshot: RwLock<Option<Envelope>>,
    updates: broadcast::Sender<Envelope>,
    browser_opened: AtomicBool,
    open_browser: bool,
    browser_url: String,
    terrain: TerrainService,
    marker_icons: MarkerIconService,
    last_heartbeat: Mutex<Option<Instant>>,
    diagnostics: Mutex<VecDeque<&'static str>>,
    started_at: Instant,
}

#[derive(Debug, Serialize)]
struct StatusPayload {
    connection: &'static str,
    edition: &'static str,
    terrain: Option<String>,
    update_age_ms: Option<u64>,
    heartbeat_timeout_ms: u64,
    terrain_cache_bytes: u64,
    terrain_cache_limit_bytes: u64,
    marker_cache_bytes: u64,
    marker_cache_limit_bytes: u64,
    diagnostics: Vec<&'static str>,
}

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebAssets;

pub async fn start(options: CompanionOptions) -> Result<RunningCompanion, CompanionError> {
    start_with_browser_opener(options, open_browser).await
}

async fn start_with_browser_opener(
    options: CompanionOptions,
    browser_opener: impl Fn(&str) -> std::io::Result<()> + Send + Sync + 'static,
) -> Result<RunningCompanion, CompanionError> {
    validate_options(&options)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    if !address.ip().is_loopback() {
        return Err(CompanionError::InvalidOptions);
    }

    let browser_token = random_hex(32)?;
    let expected_origin = format!("http://{address}");
    let browser_url = format!("{expected_origin}/#token={browser_token}");
    let (updates, _) = broadcast::channel(256);
    let terrain = TerrainService::new().map_err(|_| CompanionError::Terrain)?;
    let marker_icons = MarkerIconService::new(options.arma_root.clone())
        .map_err(|_| CompanionError::MarkerIcons)?;
    let state = Arc::new(AppState {
        expected_origin,
        browser_token: browser_token.clone(),
        pipe_token: options.pipe_token,
        current_snapshot: RwLock::new(None),
        updates,
        browser_opened: AtomicBool::new(false),
        open_browser: options.open_browser,
        browser_url,
        terrain,
        marker_icons,
        last_heartbeat: Mutex::new(None),
        diagnostics: Mutex::new(VecDeque::from(["companion_started"])),
        started_at: Instant::now(),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/status/{token}", get(companion_status))
        .route("/ws", get(websocket_upgrade))
        .route(
            "/terrain/{token}/{world_name}/metadata",
            get(terrain_metadata),
        )
        .route(
            "/terrain/{token}/{world_name}/{zoom}/{x}/{y}",
            get(terrain_tile),
        )
        .route("/marker-icon/{token}/{marker_type}", get(marker_icon))
        .route(
            "/marker-icon/{token}/{marker_type}/overlay",
            get(marker_overlay_icon),
        )
        .route("/entity-icon/{token}/{slot}/{entity_id}", get(entity_icon))
        .fallback(static_asset)
        .layer(middleware::from_fn(security_headers))
        .with_state(Arc::clone(&state));
    let server_task = tokio::spawn(async move { axum::serve(listener, app).await });
    let pipe_name = options.pipe_name;
    let pipe_state = Arc::clone(&state);
    let pipe_task =
        tokio::spawn(async move { pipe_server_loop(&pipe_name, pipe_state, browser_opener).await });
    let watchdog_task = tokio::spawn(async move { heartbeat_watchdog(state).await });

    Ok(RunningCompanion {
        address,
        browser_token,
        server_task,
        pipe_task,
        watchdog_task,
    })
}

async fn companion_status(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Response {
    if !constant_time_equal(&token, &state.browser_token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (edition, terrain) = {
        let current = state.current_snapshot.read().await;
        current
            .as_ref()
            .and_then(|envelope| {
                let TacticalMessage::SessionSnapshot(snapshot) = &envelope.message else {
                    return None;
                };
                let edition = match snapshot.ctab_edition {
                    ctab_web_protocol::CtabEdition::None => "none",
                    ctab_web_protocol::CtabEdition::Original => "original",
                    ctab_web_protocol::CtabEdition::Devastator => "devastator",
                    ctab_web_protocol::CtabEdition::Unsupported => "unsupported",
                };
                Some((edition, snapshot.terrain.display_name.clone()))
            })
            .unwrap_or(("none", String::new()))
    };
    let age = state.last_heartbeat.lock().ok().and_then(|last| {
        last.map(|instant| instant.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
    });
    let diagnostics = state
        .diagnostics
        .lock()
        .map(|events| events.iter().copied().collect())
        .unwrap_or_default();
    let (terrain_cache, marker_cache) = tokio::join!(
        state.terrain.cache_stats(),
        state.marker_icons.cache_stats()
    );
    let payload = StatusPayload {
        connection: if age.is_some_and(|value| value < HEARTBEAT_TIMEOUT.as_millis() as u64) {
            "live"
        } else {
            "waiting"
        },
        edition,
        terrain: (!terrain.is_empty()).then_some(terrain),
        update_age_ms: age,
        heartbeat_timeout_ms: HEARTBEAT_TIMEOUT.as_millis() as u64,
        terrain_cache_bytes: terrain_cache.bytes,
        terrain_cache_limit_bytes: TERRAIN_CACHE_LIMIT_BYTES,
        marker_cache_bytes: marker_cache.bytes,
        marker_cache_limit_bytes: MARKER_CACHE_LIMIT_BYTES,
        diagnostics,
    };
    match serde_json::to_vec(&payload) {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .header(CACHE_CONTROL, "no-store")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn heartbeat_watchdog(state: Arc<AppState>) {
    loop {
        tokio::time::sleep(HEARTBEAT_CHECK_INTERVAL).await;
        let last_heartbeat = state.last_heartbeat.lock().ok().and_then(|last| *last);
        let now = Instant::now();
        let expired = last_heartbeat.is_some_and(|last| heartbeat_expired(last, now));
        let startup_idle = last_heartbeat.is_none()
            && now.saturating_duration_since(state.started_at) >= STARTUP_IDLE_TIMEOUT;
        if expired || startup_idle {
            record_diagnostic(
                &state,
                if expired {
                    "heartbeat_timeout_shutdown"
                } else {
                    "startup_idle_shutdown"
                },
            );
            return;
        }
    }
}

fn heartbeat_expired(last: Instant, now: Instant) -> bool {
    now.saturating_duration_since(last) >= HEARTBEAT_TIMEOUT
}

fn record_diagnostic(state: &AppState, event: &'static str) {
    if let Ok(mut events) = state.diagnostics.lock() {
        if events.len() == MAX_DIAGNOSTIC_EVENTS {
            events.pop_front();
        }
        events.push_back(event);
    }
}

async fn entity_icon(
    State(state): State<Arc<AppState>>,
    Path((token, slot, entity_id)): Path<(String, String, String)>,
) -> Response {
    if !constant_time_equal(&token, &state.browser_token)
        || !matches!(slot.as_str(), "primary" | "overlay")
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let icon_path = {
        let current = state.current_snapshot.read().await;
        let Some(envelope) = current.as_ref() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let TacticalMessage::SessionSnapshot(snapshot) = &envelope.message else {
            return StatusCode::NOT_FOUND.into_response();
        };
        snapshot
            .entities
            .iter()
            .find(|entity| entity.id == entity_id)
            .and_then(|entity| match slot.as_str() {
                "primary" if !entity.icon_path.is_empty() => Some(entity.icon_path.clone()),
                "overlay" if !entity.overlay_icon_path.is_empty() => {
                    Some(entity.overlay_icon_path.clone())
                }
                _ => None,
            })
    };
    let Some(icon_path) = icon_path else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    icon_path.hash(&mut hasher);
    let cache_key = format!("entity_{:016x}", hasher.finish());
    icon_response(state.marker_icons.icon(&cache_key, &icon_path).await)
}

fn validate_options(options: &CompanionOptions) -> Result<(), CompanionError> {
    let valid_pipe_name = options.pipe_name.starts_with(r"\\.\pipe\ctab-web-")
        && options.pipe_name.len() <= 180
        && options
            .pipe_name
            .chars()
            .skip(r"\\.\pipe\".len())
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    let valid_token = (32..=128).contains(&options.pipe_token.len())
        && options
            .pipe_token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric());
    let valid_arma_root = options.arma_root.as_ref().is_none_or(|root| {
        root.is_absolute() && root.join("arma3_x64.exe").is_file() && root.join("Addons").is_dir()
    });
    if valid_pipe_name && valid_token && valid_arma_root {
        Ok(())
    } else {
        Err(CompanionError::InvalidOptions)
    }
}

async fn marker_icon(
    State(state): State<Arc<AppState>>,
    Path((token, marker_type)): Path<(String, String)>,
) -> Response {
    if !constant_time_equal(&token, &state.browser_token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let icon_path = {
        let current = state.current_snapshot.read().await;
        let Some(envelope) = current.as_ref() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let TacticalMessage::SessionSnapshot(snapshot) = &envelope.message else {
            return StatusCode::NOT_FOUND.into_response();
        };
        snapshot
            .markers
            .iter()
            .find(|marker| marker.marker_type == marker_type && !marker.icon_path.is_empty())
            .map(|marker| marker.icon_path.clone())
    };
    let Some(icon_path) = icon_path else {
        return StatusCode::NOT_FOUND.into_response();
    };
    icon_response(state.marker_icons.icon(&marker_type, &icon_path).await)
}

async fn marker_overlay_icon(
    State(state): State<Arc<AppState>>,
    Path((token, marker_type)): Path<(String, String)>,
) -> Response {
    if !constant_time_equal(&token, &state.browser_token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let icon_path = {
        let current = state.current_snapshot.read().await;
        let Some(envelope) = current.as_ref() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let TacticalMessage::SessionSnapshot(snapshot) = &envelope.message else {
            return StatusCode::NOT_FOUND.into_response();
        };
        snapshot
            .markers
            .iter()
            .find(|marker| {
                marker.marker_type == marker_type && !marker.overlay_icon_path.is_empty()
            })
            .map(|marker| marker.overlay_icon_path.clone())
    };
    let Some(icon_path) = icon_path else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    icon_path.hash(&mut hasher);
    let cache_key = format!("overlay_{:016x}", hasher.finish());
    icon_response(state.marker_icons.icon(&cache_key, &icon_path).await)
}

fn icon_response(result: Result<Vec<u8>, MarkerIconError>) -> Response {
    match result {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "image/png")
            .header(CACHE_CONTROL, "private, max-age=86400")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(MarkerIconError::Cache) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Err(
            MarkerIconError::Unavailable
            | MarkerIconError::InvalidRequest
            | MarkerIconError::Unsupported,
        ) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn terrain_metadata(
    State(state): State<Arc<AppState>>,
    Path((token, world_name)): Path<(String, String)>,
) -> Response {
    if !constant_time_equal(&token, &state.browser_token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(terrain) = active_terrain(&state, &world_name).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state
        .terrain
        .metadata(&world_name, terrain.world_size)
        .await
    {
        Ok(metadata) => match serde_json::to_vec(&metadata) {
            Ok(body) => Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "application/json; charset=utf-8")
                .header(CACHE_CONTROL, "private, max-age=300")
                .body(Body::from(body))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Err(error) => terrain_error_response(error),
    }
}

async fn terrain_tile(
    State(state): State<Arc<AppState>>,
    Path((token, world_name, zoom, x, y)): Path<(String, String, u8, u32, u32)>,
) -> Response {
    if !constant_time_equal(&token, &state.browser_token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(terrain) = active_terrain(&state, &world_name).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state
        .terrain
        .tile(&world_name, terrain.world_size, zoom, x, y)
        .await
    {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "image/png")
            .header(CACHE_CONTROL, "private, max-age=86400")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(error) => terrain_error_response(error),
    }
}

async fn active_terrain(
    state: &AppState,
    requested_world_name: &str,
) -> Option<ctab_web_protocol::TerrainInfo> {
    let current = state.current_snapshot.read().await;
    let envelope = current.as_ref()?;
    let TacticalMessage::SessionSnapshot(snapshot) = &envelope.message else {
        return None;
    };
    if !snapshot.capabilities.map {
        return None;
    }
    snapshot
        .terrain
        .world_name
        .eq_ignore_ascii_case(requested_world_name)
        .then_some(snapshot.terrain.clone())
}

fn terrain_error_response(error: TerrainError) -> Response {
    match error {
        TerrainError::Unsupported | TerrainError::InvalidCoordinate => {
            StatusCode::NOT_FOUND.into_response()
        }
        TerrainError::CatalogUnavailable => StatusCode::BAD_GATEWAY.into_response(),
        TerrainError::InvalidCatalogData | TerrainError::CacheUnavailable => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

async fn websocket_upgrade(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    let valid_origin = headers
        .get(ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .is_some_and(|origin| origin == state.expected_origin);
    if !valid_origin {
        return StatusCode::FORBIDDEN.into_response();
    }
    websocket
        .max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| websocket_session(socket, state))
}

async fn websocket_session(mut socket: WebSocket, state: Arc<AppState>) {
    let authentication = tokio::time::timeout(AUTH_TIMEOUT, socket.next()).await;
    let authenticated = match authentication {
        Ok(Some(Ok(Message::Text(text)))) if text.len() <= MAX_AUTH_MESSAGE_BYTES => {
            serde_json::from_str::<BrowserCommand>(text.as_str())
                .ok()
                .is_some_and(|command| match command {
                    BrowserCommand::Authenticate { token } => {
                        constant_time_equal(&token, &state.browser_token)
                    }
                })
        }
        _ => false,
    };
    if !authenticated {
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1008,
                reason: "authentication required".into(),
            })))
            .await;
        return;
    }

    let mut updates = state.updates.subscribe();
    if let Some(snapshot) = state.current_snapshot.read().await.clone()
        && send_envelope(&mut socket, &snapshot).await.is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            update = updates.recv() => {
                match update {
                    Ok(envelope) if send_envelope(&mut socket, &envelope).await.is_err() => return,
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        record_diagnostic(&state, "websocket_resynchronized");
                        let snapshot = state.current_snapshot.read().await.clone();
                        if let Some(snapshot) = snapshot
                            && send_envelope(&mut socket, &snapshot).await.is_err()
                        {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() { return; }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}

async fn send_envelope(socket: &mut WebSocket, envelope: &Envelope) -> Result<(), ()> {
    let encoded = serde_json::to_string(envelope).map_err(|_| ())?;
    socket
        .send(Message::Text(encoded.into()))
        .await
        .map_err(|_| ())
}

async fn pipe_server_loop(
    pipe_name: &str,
    state: Arc<AppState>,
    browser_opener: impl Fn(&str) -> std::io::Result<()> + Send + Sync,
) -> Result<(), CompanionError> {
    let mut first_instance = true;
    loop {
        let mut options = ServerOptions::new();
        options.first_pipe_instance(first_instance);
        let mut pipe = options.create(pipe_name)?;
        first_instance = false;
        pipe.connect().await?;
        loop {
            let length = match pipe.read_u32_le().await {
                Ok(length) => length as usize,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(CompanionError::Io(error)),
            };
            if length == 0 || length > MAX_FRAME_BYTES {
                record_diagnostic(&state, "invalid_pipe_frame_rejected");
                break;
            }
            let mut payload = vec![0_u8; length];
            pipe.read_exact(&mut payload).await?;
            let Ok(frame) = serde_json::from_slice::<BridgeFrame>(&payload) else {
                continue;
            };
            if !constant_time_equal(&frame.pipe_token, &state.pipe_token) {
                continue;
            }

            match frame.message {
                BridgeMessage::OpenBrowser {} => open_browser_now(&state, &browser_opener),
                BridgeMessage::Publish { mut envelope } => {
                    if envelope.validate().is_err() {
                        continue;
                    }
                    canonicalize_envelope(&mut envelope);

                    if accept_envelope(&state, &envelope).await {
                        if matches!(&envelope.message, TacticalMessage::Heartbeat(_))
                            && let Ok(mut last) = state.last_heartbeat.lock()
                        {
                            *last = Some(Instant::now());
                        }
                        if matches!(&envelope.message, TacticalMessage::SessionSnapshot(_)) {
                            open_browser_once(&state, &browser_opener);
                        }
                        if !matches!(&envelope.message, TacticalMessage::Heartbeat(_)) {
                            let _ = state.updates.send(envelope);
                        }
                    }
                }
            }
        }
    }
}

fn canonicalize_envelope(envelope: &mut Envelope) {
    match &mut envelope.message {
        TacticalMessage::SessionSnapshot(snapshot) => {
            deduplicate_by_id(&mut snapshot.entities, |entity| entity.id.as_str());
            deduplicate_by_id(&mut snapshot.markers, |marker| marker.id.as_str());
        }
        TacticalMessage::EntityDelta(delta) => {
            deduplicate_by_id(&mut delta.updated, |entity| entity.id.as_str());
            deduplicate_by_id(&mut delta.removed, String::as_str);
        }
        TacticalMessage::PositionDelta(delta) => {
            deduplicate_by_id(&mut delta.updated, |update| update.id.as_str());
            deduplicate_by_id(&mut delta.removed, String::as_str);
        }
        TacticalMessage::MarkerDelta(delta) => {
            deduplicate_by_id(&mut delta.updated, |marker| marker.id.as_str());
            deduplicate_by_id(&mut delta.removed, String::as_str);
        }
        TacticalMessage::Heartbeat(_) | TacticalMessage::Error(_) => {}
    }
}

fn deduplicate_by_id<T>(items: &mut Vec<T>, id: impl for<'a> Fn(&'a T) -> &'a str) {
    let mut indices = HashMap::<String, usize>::new();
    let mut unique = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        let key = id(&item).to_owned();
        if let Some(index) = indices.get(&key).copied() {
            unique[index] = item;
        } else {
            indices.insert(key, unique.len());
            unique.push(item);
        }
    }
    *items = unique;
}

async fn accept_envelope(state: &AppState, envelope: &Envelope) -> bool {
    let mut current = state.current_snapshot.write().await;
    match &envelope.message {
        TacticalMessage::SessionSnapshot(_) => {
            if current.as_ref().is_some_and(|snapshot| {
                snapshot.session_id == envelope.session_id && snapshot.sequence >= envelope.sequence
            }) {
                return false;
            }
            let is_new_session = current
                .as_ref()
                .is_none_or(|snapshot| snapshot.session_id != envelope.session_id);
            *current = Some(envelope.clone());
            if is_new_session {
                record_diagnostic(state, "mission_ready");
            }
            true
        }
        TacticalMessage::EntityDelta(delta) => {
            let Some(snapshot_envelope) = current.as_mut() else {
                return false;
            };
            if snapshot_envelope.session_id != envelope.session_id
                || snapshot_envelope.sequence >= envelope.sequence
            {
                return false;
            }
            let TacticalMessage::SessionSnapshot(snapshot) = &mut snapshot_envelope.message else {
                return false;
            };
            let mut resulting_ids: HashSet<&str> = snapshot
                .entities
                .iter()
                .filter(|entity| !delta.removed.iter().any(|id| id == &entity.id))
                .map(|entity| entity.id.as_str())
                .collect();
            resulting_ids.extend(delta.updated.iter().map(|entity| entity.id.as_str()));
            if resulting_ids.len() > ctab_web_protocol::MAX_ENTITIES {
                return false;
            }
            snapshot
                .entities
                .retain(|entity| !delta.removed.iter().any(|id| id == &entity.id));
            for update in &delta.updated {
                if let Some(entity) = snapshot
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == update.id)
                {
                    *entity = update.clone();
                } else {
                    snapshot.entities.push(update.clone());
                }
            }
            snapshot_envelope.sequence = envelope.sequence;
            true
        }
        TacticalMessage::PositionDelta(delta) => {
            let Some(snapshot_envelope) = current.as_mut() else {
                return false;
            };
            if snapshot_envelope.session_id != envelope.session_id
                || snapshot_envelope.sequence >= envelope.sequence
            {
                return false;
            }
            let TacticalMessage::SessionSnapshot(snapshot) = &mut snapshot_envelope.message else {
                return false;
            };
            snapshot
                .entities
                .retain(|entity| !delta.removed.iter().any(|id| id == &entity.id));
            for update in &delta.updated {
                if let Some(entity) = snapshot
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == update.id)
                {
                    entity.position = update.position;
                    entity.direction = update.direction;
                }
            }
            snapshot_envelope.sequence = envelope.sequence;
            true
        }
        TacticalMessage::MarkerDelta(delta) => {
            let Some(snapshot_envelope) = current.as_mut() else {
                return false;
            };
            if snapshot_envelope.session_id != envelope.session_id
                || snapshot_envelope.sequence >= envelope.sequence
            {
                return false;
            }
            let TacticalMessage::SessionSnapshot(snapshot) = &mut snapshot_envelope.message else {
                return false;
            };
            let mut resulting_ids: HashSet<&str> = snapshot
                .markers
                .iter()
                .filter(|marker| !delta.removed.iter().any(|id| id == &marker.id))
                .map(|marker| marker.id.as_str())
                .collect();
            resulting_ids.extend(delta.updated.iter().map(|marker| marker.id.as_str()));
            if resulting_ids.len() > ctab_web_protocol::MAX_MARKERS {
                return false;
            }
            snapshot
                .markers
                .retain(|marker| !delta.removed.iter().any(|id| id == &marker.id));
            for update in &delta.updated {
                if let Some(marker) = snapshot
                    .markers
                    .iter_mut()
                    .find(|marker| marker.id == update.id)
                {
                    *marker = update.clone();
                } else {
                    snapshot.markers.push(update.clone());
                }
            }
            snapshot_envelope.sequence = envelope.sequence;
            true
        }
        TacticalMessage::Heartbeat(_) => {
            let Some(snapshot_envelope) = current.as_mut() else {
                return false;
            };
            if snapshot_envelope.session_id != envelope.session_id
                || snapshot_envelope.sequence >= envelope.sequence
            {
                return false;
            }
            snapshot_envelope.sequence = envelope.sequence;
            true
        }
        TacticalMessage::Error(_) => true,
    }
}

fn open_browser_once(state: &AppState, browser_opener: &impl Fn(&str) -> std::io::Result<()>) {
    if !state.open_browser || state.browser_opened.swap(true, Ordering::AcqRel) {
        return;
    }

    open_browser_now(state, browser_opener);
}

fn open_browser_now(state: &AppState, browser_opener: &impl Fn(&str) -> std::io::Result<()>) {
    state.browser_opened.store(true, Ordering::Release);
    if let Err(error) = browser_opener(&state.browser_url) {
        record_diagnostic(state, "browser_open_failed");
        eprintln!("cTab Web Companion could not open the browser: {error}");
    } else {
        record_diagnostic(state, "browser_opened");
    }
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    left.len() == right.len() && bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn random_hex(bytes: usize) -> Result<String, CompanionError> {
    let mut random = vec![0_u8; bytes];
    getrandom::fill(&mut random)
        .map_err(|error| CompanionError::RandomSource(error.to_string()))?;
    let mut output = String::with_capacity(bytes * 2);
    for byte in random {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    Ok(output)
}

async fn static_asset(uri: Uri) -> Response {
    let requested = uri.path().trim_start_matches('/');
    let path = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    if path.contains("..") || path.contains('\\') || path.contains('\0') {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(asset) = WebAssets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = content_type_for(path);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .header(CACHE_CONTROL, "no-store")
        .body(Body::from(asset.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn content_type_for(path: &str) -> &'static str {
    match PathExtension::of(path) {
        PathExtension::Html => "text/html; charset=utf-8",
        PathExtension::JavaScript => "text/javascript; charset=utf-8",
        PathExtension::Css => "text/css; charset=utf-8",
        PathExtension::Png => "image/png",
        PathExtension::Svg => "image/svg+xml",
        PathExtension::Other => "application/octet-stream",
    }
}

enum PathExtension {
    Html,
    JavaScript,
    Css,
    Png,
    Svg,
    Other,
}

impl PathExtension {
    fn of(path: &str) -> Self {
        if path.ends_with(".html") {
            Self::Html
        } else if path.ends_with(".js") {
            Self::JavaScript
        } else if path.ends_with(".css") {
            Self::Css
        } else if path.ends_with(".png") {
            Self::Png
        } else if path.ends_with(".svg") {
            Self::Svg
        } else {
            Self::Other
        }
    }
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self' ws://127.0.0.1:*; font-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
        ),
    );
    response
}

#[cfg(windows)]
fn open_browser(url: &str) -> Result<(), std::io::Error> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = OsStr::new("open").encode_wide().chain(once(0)).collect();
    let target: Vec<u16> = OsStr::new(url).encode_wide().chain(once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "ShellExecuteW returned {}",
            result as isize
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctab_web_bridge::send_frame_to_pipe;
    use ctab_web_protocol::{
        EntityDelta, Heartbeat, MarkerDelta, Point2, PositionDelta, PositionUpdate,
        synthetic_snapshot,
    };
    use futures_util::{SinkExt, StreamExt};
    use http::HeaderValue;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::protocol::Message as ClientMessage;

    fn test_options(suffix: &str) -> CompanionOptions {
        CompanionOptions {
            pipe_name: format!(r"\\.\pipe\ctab-web-test-{suffix}-{}", std::process::id()),
            pipe_token: "a".repeat(64),
            open_browser: false,
            arma_root: None,
        }
    }

    #[test]
    fn heartbeat_timeout_has_a_grace_period() {
        let started = Instant::now();
        assert!(!heartbeat_expired(
            started,
            started + HEARTBEAT_TIMEOUT - Duration::from_millis(1)
        ));
        assert!(heartbeat_expired(started, started + HEARTBEAT_TIMEOUT));
        assert!(started + STARTUP_IDLE_TIMEOUT > started + HEARTBEAT_TIMEOUT);
    }

    #[test]
    fn canonicalizes_duplicate_tactical_identifiers() {
        let mut envelope = synthetic_snapshot();
        let TacticalMessage::SessionSnapshot(snapshot) = &mut envelope.message else {
            panic!("expected snapshot");
        };
        let mut duplicate_entity = snapshot.entities[1].clone();
        duplicate_entity.label = "Updated BFT".to_owned();
        snapshot.entities.push(duplicate_entity);
        let mut duplicate_marker = snapshot.markers[0].clone();
        duplicate_marker.label = "Updated marker".to_owned();
        snapshot.markers.push(duplicate_marker);

        canonicalize_envelope(&mut envelope);

        let TacticalMessage::SessionSnapshot(snapshot) = envelope.message else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.entities.len(), 2);
        assert_eq!(snapshot.entities[1].label, "Updated BFT");
        assert_eq!(snapshot.markers.len(), 1);
        assert_eq!(snapshot.markers[0].label, "Updated marker");
    }

    #[test]
    fn startup_options_reject_non_ctab_pipe_names() {
        let mut options = test_options("invalid");
        options.pipe_name = r"\\.\pipe\foreign".to_owned();
        assert!(matches!(
            validate_options(&options),
            Err(CompanionError::InvalidOptions)
        ));
    }

    #[test]
    fn token_comparison_checks_length_and_content() {
        assert!(constant_time_equal("abc", "abc"));
        assert!(!constant_time_equal("abc", "abd"));
        assert!(!constant_time_equal("abc", "ab"));
    }

    #[test]
    fn serves_the_embedded_brand_logo_as_png() {
        assert_eq!(content_type_for("grp9-logo.png"), "image/png");
    }

    #[tokio::test]
    async fn browser_control_requires_pipe_auth_and_uses_only_the_current_local_url() {
        let options = test_options("browser-control");
        let pipe_name = options.pipe_name.clone();
        let pipe_token = options.pipe_token.clone();
        let (opened, mut requests) = tokio::sync::mpsc::unbounded_channel();
        let runtime = start_with_browser_opener(options, move |url| {
            opened.send(url.to_owned()).map_err(std::io::Error::other)
        })
        .await
        .expect("start companion with a recording browser opener");
        let expected_url = format!(
            "http://{}/#token={}",
            runtime.address, runtime.browser_token
        );
        let mut pipe = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_name) {
                    Ok(pipe) => break pipe,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(error) => panic!("connect to test pipe: {error}"),
                }
            }
        })
        .await
        .expect("test pipe becomes available");
        let valid = serde_json::json!({
            "pipe_token": pipe_token,
            "message": { "type": "open_browser" }
        });
        let mut wrong_token = valid.clone();
        wrong_token["pipe_token"] = serde_json::json!("b".repeat(64));
        let mut browser_token = valid.clone();
        browser_token["pipe_token"] = serde_json::json!(runtime.browser_token);
        let mut supplied_url = valid.clone();
        supplied_url["message"]["url"] = serde_json::json!("https://example.invalid/");

        // All frames share one ordered connection.
        for frame in [
            wrong_token,
            browser_token,
            supplied_url,
            valid.clone(),
            valid,
        ] {
            let payload = serde_json::to_vec(&frame).expect("encode test frame");
            pipe.write_u32_le(payload.len() as u32)
                .await
                .expect("write frame length");
            pipe.write_all(&payload).await.expect("write frame payload");
        }
        // This diagnostic is a barrier proving that every earlier frame was processed.
        pipe.write_u32_le(0).await.expect("write end-of-test frame");
        for _ in 0..2 {
            let actual_url = tokio::time::timeout(Duration::from_secs(3), requests.recv())
                .await
                .expect("browser request arrives")
                .expect("browser opener remains available");
            assert_eq!(actual_url, expected_url);
        }
        let status = tokio::time::timeout(Duration::from_secs(3), async {
            let client = reqwest::Client::new();
            loop {
                let response = client
                    .get(format!(
                        "http://{}/status/{}",
                        runtime.address, runtime.browser_token
                    ))
                    .send()
                    .await
                    .expect("fetch diagnostics")
                    .bytes()
                    .await
                    .expect("read diagnostics");
                let status: serde_json::Value =
                    serde_json::from_slice(&response).expect("decode diagnostics");
                if status["diagnostics"].as_array().is_some_and(|events| {
                    events.last() == Some(&serde_json::json!("invalid_pipe_frame_rejected"))
                }) {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("all test frames are processed");
        assert!(requests.try_recv().is_err());
        assert_eq!(
            status["diagnostics"],
            serde_json::json!([
                "companion_started",
                "browser_opened",
                "browser_opened",
                "invalid_pipe_frame_rejected"
            ])
        );
        assert!(status["update_age_ms"].is_null());
        assert_eq!(status["edition"], "none");
        runtime.stop();
    }

    #[tokio::test]
    async fn serves_embedded_assets_with_strict_security_headers() {
        let runtime = start(test_options("headers"))
            .await
            .expect("start companion");
        let mut stream = TcpStream::connect(runtime.address)
            .await
            .expect("connect loopback HTTP");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .await
            .expect("write request");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("read response");
        let response = String::from_utf8(response).expect("UTF-8 response");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("content-security-policy: default-src 'self'"));
        assert!(response.contains("x-content-type-options: nosniff"));
        assert!(response.contains("<title>cTab Web Companion</title>"));
        assert!(!response.contains(&runtime.browser_token));
        runtime.stop();
    }

    #[tokio::test]
    async fn denies_terrain_metadata_without_map_capability() {
        let options = test_options("terrain-access");
        let pipe_name = options.pipe_name.clone();
        let pipe_token = options.pipe_token.clone();
        let runtime = start(options).await.expect("start companion");
        let mut snapshot = synthetic_snapshot();
        let TacticalMessage::SessionSnapshot(payload) = &mut snapshot.message else {
            panic!("expected a synthetic snapshot");
        };
        payload.capabilities.map = false;

        tokio::task::spawn_blocking(move || {
            send_frame_to_pipe(&pipe_name, &pipe_token, snapshot, Duration::from_secs(3))
        })
        .await
        .expect("writer task")
        .expect("send bridge frame");
        tokio::time::sleep(Duration::from_millis(50)).await;

        let response = reqwest::get(format!(
            "http://{}/terrain/{}/Synthetic_Altis/metadata",
            runtime.address, runtime.browser_token
        ))
        .await
        .expect("request terrain metadata");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        runtime.stop();
    }

    #[tokio::test]
    async fn concurrent_companions_use_distinct_dynamic_loopback_ports() {
        let first = start(test_options("port-a"))
            .await
            .expect("first companion");
        let second = start(test_options("port-b"))
            .await
            .expect("second companion");
        assert!(first.address.ip().is_loopback());
        assert!(second.address.ip().is_loopback());
        assert_ne!(first.address.port(), second.address.port());
        first.stop();
        second.stop();
    }

    #[tokio::test]
    async fn rejects_an_invalid_websocket_origin() {
        let runtime = start(test_options("origin"))
            .await
            .expect("start companion");
        let mut request = format!("ws://{}/ws", runtime.address)
            .into_client_request()
            .expect("websocket request");
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_static("http://attacker.invalid"));
        let result = connect_async(request).await;
        assert!(result.is_err());
        runtime.stop();
    }

    #[tokio::test]
    async fn rejects_an_invalid_session_token() {
        let runtime = start(test_options("token")).await.expect("start companion");
        assert!(runtime.address.ip().is_loopback());
        let address = runtime.address;
        let mut request = format!("ws://{address}/ws")
            .into_client_request()
            .expect("websocket request");
        request.headers_mut().insert(
            ORIGIN,
            HeaderValue::from_str(&format!("http://{address}")).expect("valid origin"),
        );
        let (mut socket, _) = connect_async(request).await.expect("connect websocket");
        socket
            .send(ClientMessage::Text(
                serde_json::json!({
                    "type": "authenticate",
                    "token": "invalid-invalid-invalid-invalid-invalid-invalid",
                })
                .to_string()
                .into(),
            ))
            .await
            .expect("send invalid authentication");
        let response = socket
            .next()
            .await
            .expect("close response")
            .expect("close frame");
        assert!(matches!(response, ClientMessage::Close(_)));
        runtime.stop();
    }

    #[tokio::test]
    async fn snapshot_and_deltas_are_reconciled_for_refresh() {
        let options = test_options("vertical");
        let pipe_name = options.pipe_name.clone();
        let pipe_token = options.pipe_token.clone();
        let runtime = start(options).await.expect("start companion");
        let address = runtime.address;
        let browser_token = runtime.browser_token.clone();

        let snapshot = synthetic_snapshot();
        let mut entity_delta = synthetic_snapshot();
        entity_delta.sequence = 2;
        let TacticalMessage::SessionSnapshot(snapshot_payload) = &entity_delta.message else {
            panic!("expected a synthetic snapshot");
        };
        let mut added_entity = snapshot_payload.entities[1].clone();
        added_entity.id = "ctab-og-unit:1:2".to_owned();
        added_entity.label = "Rifleman".to_owned();
        entity_delta.message = TacticalMessage::EntityDelta(EntityDelta {
            updated: vec![added_entity],
            removed: vec!["bft-alpha-1".to_owned()],
        });
        let mut position_delta = synthetic_snapshot();
        position_delta.sequence = 3;
        position_delta.message = TacticalMessage::PositionDelta(PositionDelta {
            updated: vec![
                PositionUpdate {
                    id: "player-local".to_owned(),
                    position: Point2 { x: 900.0, y: 800.0 },
                    direction: 180.0,
                },
                PositionUpdate {
                    id: "ctab-og-unit:1:2".to_owned(),
                    position: Point2 { x: 700.0, y: 600.0 },
                    direction: 45.0,
                },
            ],
            removed: Vec::new(),
        });
        let mut marker_delta = synthetic_snapshot();
        marker_delta.sequence = 4;
        marker_delta.message = TacticalMessage::MarkerDelta(MarkerDelta {
            updated: Vec::new(),
            removed: vec!["marker-objective".to_owned()],
        });
        let mut heartbeat = synthetic_snapshot();
        heartbeat.sequence = 5;
        heartbeat.message = TacticalMessage::Heartbeat(Heartbeat { uptime_ms: 42.0 });

        for envelope in [
            snapshot,
            entity_delta,
            position_delta,
            marker_delta,
            heartbeat,
        ] {
            let pipe_name = pipe_name.clone();
            let pipe_token = pipe_token.clone();
            tokio::task::spawn_blocking(move || {
                send_frame_to_pipe(&pipe_name, &pipe_token, envelope, Duration::from_secs(3))
            })
            .await
            .expect("writer task")
            .expect("send bridge frame");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!runtime.pipe_task.is_finished());

        for _ in 0..2 {
            let mut request = format!("ws://{address}/ws")
                .into_client_request()
                .expect("websocket request");
            request.headers_mut().insert(
                ORIGIN,
                HeaderValue::from_str(&format!("http://{address}")).expect("valid origin"),
            );
            let (mut socket, _) = connect_async(request).await.expect("connect websocket");
            socket
                .send(ClientMessage::Text(
                    serde_json::json!({
                        "type": "authenticate",
                        "token": browser_token,
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("authenticate");
            let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .expect("snapshot timeout")
                .expect("snapshot message")
                .expect("valid snapshot message");
            let ClientMessage::Text(text) = message else {
                panic!("expected a text snapshot");
            };
            let envelope: Envelope = serde_json::from_str(&text).expect("snapshot envelope");
            assert_eq!(envelope.sequence, 5);
            let TacticalMessage::SessionSnapshot(snapshot) = envelope.message else {
                panic!("expected a reconciled session snapshot");
            };
            assert_eq!(snapshot.entities[0].position, Point2 { x: 900.0, y: 800.0 });
            assert_eq!(snapshot.entities[1].id, "ctab-og-unit:1:2");
            assert_eq!(snapshot.entities[1].position, Point2 { x: 700.0, y: 600.0 });
            assert!(snapshot.markers.is_empty());
        }
        let status_text = reqwest::get(format!("http://{address}/status/{browser_token}"))
            .await
            .expect("request diagnostic status")
            .text()
            .await
            .expect("read diagnostic status");
        assert!(status_text.contains(r#""connection":"live""#));
        assert!(status_text.contains(r#""edition":"original""#));
        assert!(status_text.contains("mission_ready"));
        assert!(!status_text.contains("Phase 1 <img"));
        assert!(!status_text.contains(&browser_token));
        runtime.stop();
    }

    #[tokio::test]
    #[ignore = "requires CTAB_ARMA_ROOT to point to a local Arma 3 installation"]
    async fn serves_a_token_scoped_marker_icon_from_the_local_game_installation() {
        let mut options = test_options("marker-icon-live");
        options.arma_root = Some(
            std::env::var_os("CTAB_ARMA_ROOT")
                .map(std::path::PathBuf::from)
                .expect("CTAB_ARMA_ROOT is required"),
        );
        let pipe_name = options.pipe_name.clone();
        let pipe_token = options.pipe_token.clone();
        let runtime = start(options).await.expect("start companion");
        let mut snapshot = synthetic_snapshot();
        let TacticalMessage::SessionSnapshot(payload) = &mut snapshot.message else {
            panic!("expected a synthetic snapshot");
        };
        payload.markers[0].marker_type = "mil_unknown".to_owned();
        payload.markers[0].icon_path =
            r"\A3\ui_f\data\map\markers\military\unknown_CA.paa".to_owned();

        tokio::task::spawn_blocking(move || {
            send_frame_to_pipe(&pipe_name, &pipe_token, snapshot, Duration::from_secs(3))
        })
        .await
        .expect("writer task")
        .expect("send bridge frame");
        tokio::time::sleep(Duration::from_millis(50)).await;

        let response = reqwest::get(format!(
            "http://{}/marker-icon/{}/mil_unknown",
            runtime.address, runtime.browser_token
        ))
        .await
        .expect("request marker icon");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("image/png"))
        );
        let bytes = response.bytes().await.expect("read marker icon");
        assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        runtime.stop();
    }
}
