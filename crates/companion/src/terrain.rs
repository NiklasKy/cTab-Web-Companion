use crate::cache::{self, CacheStats, TERRAIN_CACHE_LIMIT_BYTES};
use futures_util::StreamExt;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const PRIMARY_ORIGIN: &str = "https://atlas.plan-ops.fr";
const MIRROR_ORIGIN: &str = "https://de.atlas.plan-ops.fr";
const MAX_METADATA_BYTES: usize = 256 * 1024;
const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_CATALOG_MAPS: usize = 4_096;
const MAX_TILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TILE_DIMENSION: u32 = 2_048;

#[derive(Clone, Debug)]
pub(crate) struct TerrainService {
    client: reqwest::Client,
    cache_root: PathBuf,
    origins: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct TerrainMetadata {
    pub world_name: String,
    pub catalog_name: String,
    pub display_name: String,
    pub attribution: String,
    pub size_in_meters: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    pub map_id: u32,
    pub layer_id: u32,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub default_zoom: u8,
    pub tile_size: u16,
    pub factor_x: f64,
    pub factor_y: f64,
}

#[derive(Debug, Error)]
pub(crate) enum TerrainError {
    #[error("unsupported terrain")]
    Unsupported,
    #[error("invalid tile coordinate")]
    InvalidCoordinate,
    #[error("terrain catalog unavailable")]
    CatalogUnavailable,
    #[error("terrain catalog returned invalid data")]
    InvalidCatalogData,
    #[error("terrain cache unavailable")]
    CacheUnavailable,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogMap {
    attribution: Option<String>,
    append_attribution: Option<String>,
    game_map_id: u32,
    english_title: String,
    size_in_meters: f64,
    name: String,
    aliases: Option<Vec<String>>,
    origin_x: f64,
    origin_y: f64,
    layers: Vec<CatalogLayer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogLayer {
    game_map_layer_id: u32,
    #[serde(rename = "type")]
    layer_type: String,
    format: String,
    min_zoom: u8,
    max_zoom: u8,
    default_zoom: u8,
    is_default: bool,
    tile_size: u16,
    factor_x: f64,
    factor_y: f64,
}

impl TerrainService {
    pub(crate) fn new() -> Result<Self, TerrainError> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or(TerrainError::CacheUnavailable)?;
        Self::with_origins(
            local_app_data
                .join("cTabWeb")
                .join("terrain-cache")
                .join("v1"),
            vec![PRIMARY_ORIGIN.to_owned(), MIRROR_ORIGIN.to_owned()],
            false,
            Duration::from_secs(10),
        )
    }

    fn with_origins(
        cache_root: PathBuf,
        origins: Vec<String>,
        allow_test_http: bool,
        request_timeout: Duration,
    ) -> Result<Self, TerrainError> {
        if origins.is_empty()
            || origins
                .iter()
                .any(|origin| !valid_provider_origin(origin, allow_test_http))
        {
            return Err(TerrainError::InvalidCatalogData);
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("cTab-Web-Companion/1.0")
            .build()
            .map_err(|_| TerrainError::CatalogUnavailable)?;
        Ok(Self {
            client,
            cache_root,
            origins,
        })
    }

    pub(crate) async fn metadata(
        &self,
        world_name: &str,
        expected_size: f64,
    ) -> Result<TerrainMetadata, TerrainError> {
        let request =
            terrain_request(world_name, expected_size).ok_or(TerrainError::Unsupported)?;
        let cache_path = self.metadata_cache_path(&request.cache_key);
        if let Ok(bytes) = tokio::fs::read(&cache_path).await
            && bytes.len() <= MAX_METADATA_BYTES
            && let Ok(metadata) = serde_json::from_slice::<TerrainMetadata>(&bytes)
            && validate_metadata(&metadata, &request, None).is_ok()
        {
            return Ok(metadata);
        }

        let mut last_error = TerrainError::CatalogUnavailable;
        for origin in &self.origins {
            let result = if let Some(catalog_name) = known_catalog_name(&request.cache_key) {
                let url = format!("{origin}/api/v1/games/arma3/maps/{catalog_name}");
                self.fetch_metadata(&url, &request, Some(catalog_name))
                    .await
            } else {
                let url = format!("{origin}/api/v1/games/arma3/maps");
                self.fetch_catalog_metadata(&url, &request).await
            };
            match result {
                Ok(metadata) => {
                    let bytes = serde_json::to_vec(&metadata)
                        .map_err(|_| TerrainError::InvalidCatalogData)?;
                    write_atomically(&cache_path, &bytes).await?;
                    self.maintain_cache(cache_path.clone()).await;
                    return Ok(metadata);
                }
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    pub(crate) async fn tile(
        &self,
        world_name: &str,
        expected_size: f64,
        zoom: u8,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, TerrainError> {
        let request =
            terrain_request(world_name, expected_size).ok_or(TerrainError::Unsupported)?;
        let metadata = self.metadata(world_name, expected_size).await?;
        validate_tile_coordinate(&metadata, zoom, x, y)?;
        let cache_path = self.tile_cache_path(&request.cache_key, &metadata, zoom, x, y);
        if let Ok(bytes) = tokio::fs::read(&cache_path).await
            && valid_png(&bytes, metadata.tile_size)
        {
            return Ok(bytes);
        }

        let mut last_error = TerrainError::CatalogUnavailable;
        for origin in &self.origins {
            let url = format!(
                "{origin}/data/1/maps/{}/{}/{zoom}/{x}/{y}.png",
                metadata.map_id, metadata.layer_id
            );
            match self.fetch_png(&url, metadata.tile_size).await {
                Ok(bytes) => {
                    write_atomically(&cache_path, &bytes).await?;
                    self.maintain_cache(cache_path.clone()).await;
                    return Ok(bytes);
                }
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    async fn fetch_metadata(
        &self,
        url: &str,
        request: &TerrainRequest,
        expected_catalog_name: Option<&str>,
    ) -> Result<TerrainMetadata, TerrainError> {
        let bytes = self
            .fetch_limited(url, "application/json", MAX_METADATA_BYTES)
            .await?;
        let catalog: CatalogMap =
            serde_json::from_slice(&bytes).map_err(|_| TerrainError::InvalidCatalogData)?;
        self.catalog_metadata(catalog, request, expected_catalog_name)
    }

    async fn fetch_catalog_metadata(
        &self,
        url: &str,
        request: &TerrainRequest,
    ) -> Result<TerrainMetadata, TerrainError> {
        let bytes = self
            .fetch_limited(url, "application/json", MAX_CATALOG_BYTES)
            .await?;
        let catalog: Vec<CatalogMap> =
            serde_json::from_slice(&bytes).map_err(|_| TerrainError::InvalidCatalogData)?;
        if catalog.is_empty() || catalog.len() > MAX_CATALOG_MAPS {
            return Err(TerrainError::InvalidCatalogData);
        }
        let map = select_catalog_map(catalog, request).ok_or(TerrainError::Unsupported)?;
        self.catalog_metadata(map, request, None)
    }

    fn catalog_metadata(
        &self,
        catalog: CatalogMap,
        request: &TerrainRequest,
        expected_catalog_name: Option<&str>,
    ) -> Result<TerrainMetadata, TerrainError> {
        if !valid_catalog_name(&catalog.name)
            || expected_catalog_name.is_some_and(|expected| catalog.name != expected)
        {
            return Err(TerrainError::InvalidCatalogData);
        }
        let layer = catalog
            .layers
            .into_iter()
            .find(|layer| {
                layer.is_default
                    && layer.layer_type == "Topographic"
                    && matches!(layer.format.as_str(), "PngOnly" | "PngAndWebp")
            })
            .ok_or(TerrainError::InvalidCatalogData)?;
        let attribution = catalog
            .attribution
            .or(catalog.append_attribution)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "PlanOps Atlas".to_owned());
        let metadata = TerrainMetadata {
            world_name: request.world_name.clone(),
            catalog_name: catalog.name,
            display_name: catalog.english_title,
            attribution,
            size_in_meters: catalog.size_in_meters,
            origin_x: catalog.origin_x,
            origin_y: catalog.origin_y,
            map_id: catalog.game_map_id,
            layer_id: layer.game_map_layer_id,
            min_zoom: layer.min_zoom,
            max_zoom: layer.max_zoom,
            default_zoom: layer.default_zoom,
            tile_size: layer.tile_size,
            factor_x: layer.factor_x,
            factor_y: layer.factor_y,
        };
        validate_metadata(&metadata, request, Some(&metadata.catalog_name))?;
        Ok(metadata)
    }

    async fn fetch_png(&self, url: &str, tile_size: u16) -> Result<Vec<u8>, TerrainError> {
        let bytes = self.fetch_limited(url, "image/png", MAX_TILE_BYTES).await?;
        if !valid_png(&bytes, tile_size) {
            return Err(TerrainError::InvalidCatalogData);
        }
        Ok(bytes)
    }

    async fn fetch_limited(
        &self,
        url: &str,
        expected_content_type: &str,
        limit: usize,
    ) -> Result<Vec<u8>, TerrainError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| TerrainError::CatalogUnavailable)?;
        if response.status().is_redirection() || !response.status().is_success() {
            return Err(TerrainError::CatalogUnavailable);
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        if content_type != Some(expected_content_type)
            || response
                .content_length()
                .is_some_and(|length| length > limit as u64)
        {
            return Err(TerrainError::InvalidCatalogData);
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| TerrainError::CatalogUnavailable)?;
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(TerrainError::InvalidCatalogData);
            }
            body.extend_from_slice(&chunk);
        }
        if body.is_empty() {
            return Err(TerrainError::InvalidCatalogData);
        }
        Ok(body)
    }

    fn metadata_cache_path(&self, cache_key: &str) -> PathBuf {
        self.cache_root.join(cache_key).join("metadata.json")
    }

    fn tile_cache_path(
        &self,
        cache_key: &str,
        metadata: &TerrainMetadata,
        zoom: u8,
        x: u32,
        y: u32,
    ) -> PathBuf {
        self.cache_root
            .join(cache_key)
            .join(metadata.layer_id.to_string())
            .join(zoom.to_string())
            .join(x.to_string())
            .join(format!("{y}.png"))
    }

    async fn maintain_cache(&self, protected: PathBuf) {
        let root = self.cache_root.clone();
        let _ = tokio::task::spawn_blocking(move || {
            cache::enforce_limit(&root, TERRAIN_CACHE_LIMIT_BYTES, Some(&protected));
        })
        .await;
    }

    pub(crate) async fn cache_stats(&self) -> CacheStats {
        let root = self.cache_root.clone();
        tokio::task::spawn_blocking(move || cache::stats(&root))
            .await
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
struct TerrainRequest {
    world_name: String,
    cache_key: String,
    expected_size: f64,
}

fn terrain_request(world_name: &str, expected_size: f64) -> Option<TerrainRequest> {
    if !expected_size.is_finite() || !(1.0..=1_000_000.0).contains(&expected_size) {
        return None;
    }
    let trimmed = world_name.trim();
    let cache_key = normalized_world_name(trimmed)?;
    Some(TerrainRequest {
        world_name: trimmed.to_owned(),
        cache_key,
        expected_size,
    })
}

fn normalized_world_name(world_name: &str) -> Option<String> {
    let normalized = world_name.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 128
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        None
    } else {
        Some(normalized)
    }
}

fn known_catalog_name(normalized_world_name: &str) -> Option<&'static str> {
    match normalized_world_name {
        "altis" => Some("altis"),
        "lythium" => Some("lythium"),
        "tanoa" => Some("tanoa"),
        _ => None,
    }
}

fn valid_catalog_name(value: &str) -> bool {
    normalized_world_name(value).is_some_and(|normalized| normalized == value)
}

fn select_catalog_map(catalog: Vec<CatalogMap>, request: &TerrainRequest) -> Option<CatalogMap> {
    let requested = request.cache_key.as_str();
    let matches_size = |map: &CatalogMap| {
        map.size_in_meters.is_finite() && (map.size_in_meters - request.expected_size).abs() < 0.01
    };
    if let Some(index) = catalog
        .iter()
        .position(|map| normalized_world_name(&map.name).as_deref() == Some(requested))
    {
        if !matches_size(&catalog[index]) {
            return None;
        }
        return catalog.into_iter().nth(index);
    }
    let alias_index = catalog.iter().position(|map| {
        matches_size(map)
            && map
                .aliases
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|alias| normalized_world_name(alias).as_deref() == Some(requested))
    });
    alias_index.and_then(|index| catalog.into_iter().nth(index))
}

fn validate_metadata(
    metadata: &TerrainMetadata,
    request: &TerrainRequest,
    expected_catalog_name: Option<&str>,
) -> Result<(), TerrainError> {
    let valid_text = |value: &str| !value.is_empty() && value.len() <= 512 && !value.contains('\0');
    let valid = metadata
        .world_name
        .eq_ignore_ascii_case(&request.world_name)
        && valid_catalog_name(&metadata.catalog_name)
        && expected_catalog_name.is_none_or(|expected| metadata.catalog_name == expected)
        && valid_text(&metadata.display_name)
        && valid_text(&metadata.attribution)
        && (metadata.size_in_meters - request.expected_size).abs() < 0.01
        && metadata.size_in_meters.is_finite()
        && metadata.origin_x.is_finite()
        && metadata.origin_y.is_finite()
        && metadata.factor_x.is_finite()
        && metadata.factor_x > 0.0
        && metadata.factor_y.is_finite()
        && metadata.factor_y > 0.0
        && metadata.map_id > 0
        && metadata.layer_id > 0
        && metadata.min_zoom <= metadata.default_zoom
        && metadata.default_zoom <= metadata.max_zoom
        && metadata.max_zoom <= 12
        && (64..=1_024).contains(&metadata.tile_size);
    if valid {
        Ok(())
    } else {
        Err(TerrainError::InvalidCatalogData)
    }
}

fn validate_tile_coordinate(
    metadata: &TerrainMetadata,
    zoom: u8,
    x: u32,
    y: u32,
) -> Result<(), TerrainError> {
    if zoom < metadata.min_zoom || zoom > metadata.max_zoom {
        return Err(TerrainError::InvalidCoordinate);
    }
    let scale = 2_u32
        .checked_pow(u32::from(zoom))
        .ok_or(TerrainError::InvalidCoordinate)?;
    let columns = ((metadata.size_in_meters * metadata.factor_x * f64::from(scale))
        / f64::from(metadata.tile_size))
    .ceil() as u32;
    let rows = ((metadata.size_in_meters * metadata.factor_y * f64::from(scale))
        / f64::from(metadata.tile_size))
    .ceil() as u32;
    if x < columns && y < rows {
        Ok(())
    } else {
        Err(TerrainError::InvalidCoordinate)
    }
}

fn valid_png(bytes: &[u8], expected_size: u16) -> bool {
    if bytes.len() < 24 || bytes[..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return false;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("four width bytes"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("four height bytes"));
    width == u32::from(expected_size)
        && height == u32::from(expected_size)
        && width <= MAX_TILE_DIMENSION
        && height <= MAX_TILE_DIMENSION
}

fn valid_provider_origin(origin: &str, allow_test_http: bool) -> bool {
    origin == PRIMARY_ORIGIN
        || origin == MIRROR_ORIGIN
        || (allow_test_http && origin.starts_with("http://127.0.0.1:"))
}

async fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), TerrainError> {
    let parent = path.parent().ok_or(TerrainError::CacheUnavailable)?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|_| TerrainError::CacheUnavailable)?;
    let mut suffix = [0_u8; 8];
    getrandom::fill(&mut suffix).map_err(|_| TerrainError::CacheUnavailable)?;
    let suffix = suffix
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(TerrainError::CacheUnavailable)?;
    let temporary = parent.join(format!(".{file_name}.{suffix}.tmp"));
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|_| TerrainError::CacheUnavailable)?;
    match tokio::fs::rename(&temporary, path).await {
        Ok(()) => Ok(()),
        Err(_) if path.is_file() => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Ok(())
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Err(TerrainError::CacheUnavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::header;
    use axum::response::Redirect;
    use axum::routing::get;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    fn metadata() -> TerrainMetadata {
        TerrainMetadata {
            world_name: "Altis".to_owned(),
            catalog_name: "altis".to_owned(),
            display_name: "Altis".to_owned(),
            attribution: "Copyright test".to_owned(),
            size_in_meters: 30_720.0,
            origin_x: 0.0,
            origin_y: 0.0,
            map_id: 3,
            layer_id: 3,
            min_zoom: 0,
            max_zoom: 6,
            default_zoom: 2,
            tile_size: 381,
            factor_x: 0.012375,
            factor_y: 0.012375,
        }
    }

    fn catalog_map(name: &str, aliases: &[&str], size_in_meters: f64) -> CatalogMap {
        CatalogMap {
            attribution: Some("Copyright test".to_owned()),
            append_attribution: None,
            game_map_id: 42,
            english_title: "Catalog Test".to_owned(),
            size_in_meters,
            name: name.to_owned(),
            aliases: Some(aliases.iter().map(|alias| (*alias).to_owned()).collect()),
            origin_x: 0.0,
            origin_y: 0.0,
            layers: vec![CatalogLayer {
                game_map_layer_id: 43,
                layer_type: "Topographic".to_owned(),
                format: "PngOnly".to_owned(),
                min_zoom: 0,
                max_zoom: 6,
                default_zoom: 2,
                is_default: true,
                tile_size: 256,
                factor_x: 0.025,
                factor_y: 0.025,
            }],
        }
    }

    #[test]
    fn normalizes_only_safe_arma_world_names() {
        let tanoa = terrain_request(" TANOA ", 15_360.0).expect("Tanoa request");
        assert_eq!(tanoa.world_name, "TANOA");
        assert_eq!(tanoa.cache_key, "tanoa");
        assert_eq!(tanoa.expected_size, 15_360.0);
        assert_eq!(known_catalog_name(&tanoa.cache_key), Some("tanoa"));
        assert!(terrain_request("tem_anizay", 10_240.0).is_some());
        assert!(terrain_request("../altis", 30_720.0).is_none());
        assert!(terrain_request("Altis", f64::NAN).is_none());
    }

    #[test]
    fn resolves_dynamic_catalog_names_and_aliases_with_matching_sizes() {
        let direct = terrain_request("tem_anizay", 10_240.0).expect("direct request");
        let selected = select_catalog_map(vec![catalog_map("tem_anizay", &[], 10_240.0)], &direct)
            .expect("direct catalog match");
        assert_eq!(selected.name, "tem_anizay");

        let alias = terrain_request("vtf_lybor_winter", 6_144.0).expect("alias request");
        let selected = select_catalog_map(
            vec![catalog_map("vtf_lybor", &["vtf_lybor_winter"], 6_144.0)],
            &alias,
        )
        .expect("alias catalog match");
        assert_eq!(selected.name, "vtf_lybor");

        let wrong_size = terrain_request("vtf_lybor_winter", 8_192.0).expect("request");
        assert!(
            select_catalog_map(
                vec![catalog_map("vtf_lybor", &["vtf_lybor_winter"], 6_144.0)],
                &wrong_size,
            )
            .is_none()
        );
    }

    #[test]
    fn rejects_unknown_or_non_https_provider_origins() {
        assert!(valid_provider_origin(PRIMARY_ORIGIN, false));
        assert!(valid_provider_origin(MIRROR_ORIGIN, false));
        assert!(!valid_provider_origin("http://atlas.plan-ops.fr", false));
        assert!(!valid_provider_origin("https://attacker.invalid", false));
        assert!(!valid_provider_origin(
            "https://atlas.plan-ops.fr.attacker.invalid",
            false
        ));
    }

    #[test]
    fn rejects_out_of_range_tiles() {
        let metadata = metadata();
        assert!(validate_tile_coordinate(&metadata, 2, 3, 3).is_ok());
        assert!(validate_tile_coordinate(&metadata, 2, 4, 0).is_err());
        assert!(validate_tile_coordinate(&metadata, 7, 0, 0).is_err());
    }

    #[test]
    fn validates_png_signature_and_dimensions() {
        let mut png = vec![0_u8; 24];
        png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png[16..20].copy_from_slice(&381_u32.to_be_bytes());
        png[20..24].copy_from_slice(&381_u32.to_be_bytes());
        assert!(valid_png(&png, 381));
        png[16..20].copy_from_slice(&380_u32.to_be_bytes());
        assert!(!valid_png(&png, 381));
        assert!(!valid_png(b"not an image", 381));
    }

    #[tokio::test]
    async fn rejects_redirect_wrong_type_oversize_invalid_image_and_timeout() {
        let application = Router::new()
            .route("/redirect", get(|| async { Redirect::temporary("/json") }))
            .route(
                "/wrong-type",
                get(|| async { ([(header::CONTENT_TYPE, "text/plain")], "{}") }),
            )
            .route(
                "/oversize",
                get(|| async {
                    let body = vec![b'x'; MAX_METADATA_BYTES + 1];
                    ([(header::CONTENT_TYPE, "application/json")], body)
                }),
            )
            .route(
                "/invalid-image",
                get(|| async { ([(header::CONTENT_TYPE, "image/png")], vec![0_u8; 24]) }),
            )
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    ([(header::CONTENT_TYPE, "application/json")], "{}")
                }),
            );
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind mock catalog");
        let origin = format!("http://{}", listener.local_addr().expect("mock address"));
        let server = tokio::spawn(async move {
            axum::serve(listener, application)
                .await
                .expect("serve mock catalog");
        });
        let cache = TempDir::new().expect("temporary cache");
        let service = TerrainService::with_origins(
            cache.path().to_owned(),
            vec![origin.clone()],
            true,
            Duration::from_millis(50),
        )
        .expect("test terrain service");

        assert!(matches!(
            service
                .fetch_limited(
                    &format!("{origin}/redirect"),
                    "application/json",
                    MAX_METADATA_BYTES
                )
                .await,
            Err(TerrainError::CatalogUnavailable)
        ));
        for path in ["wrong-type", "oversize"] {
            assert!(matches!(
                service
                    .fetch_limited(
                        &format!("{origin}/{path}"),
                        "application/json",
                        MAX_METADATA_BYTES
                    )
                    .await,
                Err(TerrainError::InvalidCatalogData)
            ));
        }
        assert!(matches!(
            service
                .fetch_png(&format!("{origin}/invalid-image"), 381)
                .await,
            Err(TerrainError::InvalidCatalogData)
        ));
        assert!(matches!(
            service
                .fetch_limited(
                    &format!("{origin}/slow"),
                    "application/json",
                    MAX_METADATA_BYTES
                )
                .await,
            Err(TerrainError::CatalogUnavailable)
        ));
        server.abort();
    }

    #[tokio::test]
    #[ignore = "requires the live PlanOps Atlas service"]
    async fn downloads_supported_metadata_and_tiles_then_serves_them_offline() {
        let cache = TempDir::new().expect("temporary cache");
        let online = TerrainService::with_origins(
            cache.path().to_owned(),
            vec![PRIMARY_ORIGIN.to_owned(), MIRROR_ORIGIN.to_owned()],
            false,
            Duration::from_secs(10),
        )
        .expect("online terrain service");
        let altis_metadata = online
            .metadata("Altis", 30_720.0)
            .await
            .expect("Altis metadata");
        let altis_tile = online
            .tile("Altis", 30_720.0, 2, 1, 1)
            .await
            .expect("Altis tile");
        let lythium_metadata = online
            .metadata("lythium", 20_480.0)
            .await
            .expect("Lythium metadata");
        let lythium_tile = online
            .tile("lythium", 20_480.0, 2, 1, 1)
            .await
            .expect("Lythium tile");
        let anizay_metadata = online
            .metadata("tem_anizay", 10_240.0)
            .await
            .expect("dynamic Anizay metadata");
        let anizay_tile = online
            .tile("tem_anizay", 10_240.0, 2, 1, 1)
            .await
            .expect("dynamic Anizay tile");
        let lybor_winter_metadata = online
            .metadata("vtf_lybor_winter", 6_144.0)
            .await
            .expect("aliased Lybor metadata");
        assert_eq!(altis_metadata.size_in_meters, 30_720.0);
        assert_eq!(lythium_metadata.size_in_meters, 20_480.0);
        assert_eq!(anizay_metadata.catalog_name, "tem_anizay");
        assert_eq!(lybor_winter_metadata.catalog_name, "vtf_lybor");
        assert!(valid_png(&altis_tile, altis_metadata.tile_size));
        assert!(valid_png(&lythium_tile, lythium_metadata.tile_size));
        assert!(valid_png(&anizay_tile, anizay_metadata.tile_size));

        let offline = TerrainService::with_origins(
            cache.path().to_owned(),
            vec!["http://127.0.0.1:9".to_owned()],
            true,
            Duration::from_millis(20),
        )
        .expect("offline terrain service");
        assert_eq!(
            offline
                .metadata("Altis", 30_720.0)
                .await
                .expect("cached Altis metadata"),
            altis_metadata
        );
        assert_eq!(
            offline
                .tile("Altis", 30_720.0, 2, 1, 1)
                .await
                .expect("cached Altis tile"),
            altis_tile
        );
        assert_eq!(
            offline
                .metadata("lythium", 20_480.0)
                .await
                .expect("cached Lythium metadata"),
            lythium_metadata
        );
        assert_eq!(
            offline
                .tile("lythium", 20_480.0, 2, 1, 1)
                .await
                .expect("cached Lythium tile"),
            lythium_tile
        );
        assert_eq!(
            offline
                .metadata("tem_anizay", 10_240.0)
                .await
                .expect("cached dynamic Anizay metadata"),
            anizay_metadata
        );
        assert_eq!(
            offline
                .tile("tem_anizay", 10_240.0, 2, 1, 1)
                .await
                .expect("cached dynamic Anizay tile"),
            anizay_tile
        );
        assert_eq!(
            offline
                .metadata("vtf_lybor_winter", 6_144.0)
                .await
                .expect("cached aliased Lybor metadata"),
            lybor_winter_metadata
        );
    }
}
