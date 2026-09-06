use crate::cache::{self, CacheStats, MARKER_CACHE_LIMIT_BYTES};
use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use texpresso::Format;
use thiserror::Error;

const MAX_PBO_FILES: usize = 2_048;
const MAX_PBO_ENTRIES: usize = 100_000;
const MAX_PBO_HEADER_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 4 * 1024 * 1024;
const MAX_PAA_DIMENSION: usize = 1_024;
const MAX_TAG_BYTES: usize = 1024 * 1024;
const MAX_MOD_INDICES: usize = 128;
const MOD_ICON_DESCRIPTOR: &str = "ctab_mod_icon_v1";

#[derive(Clone, Debug)]
pub(crate) struct MarkerIconService {
    arma_root: Option<PathBuf>,
    cache_root: PathBuf,
    pbo_index: Arc<OnceLock<Vec<PboLocation>>>,
    mod_indices: Arc<Mutex<HashMap<String, Arc<Vec<PboLocation>>>>>,
}

#[derive(Clone, Debug)]
struct PboLocation {
    path: PathBuf,
    prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModIconReference {
    mod_directory: String,
    workshop_id: String,
    mod_hash: String,
    virtual_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum IconReference {
    BaseGame(String),
    Mod(ModIconReference),
}

#[derive(Debug, Error)]
pub(crate) enum MarkerIconError {
    #[error("marker icons are unavailable")]
    Unavailable,
    #[error("invalid marker icon request")]
    InvalidRequest,
    #[error("unsupported marker texture")]
    Unsupported,
    #[error("marker icon cache is unavailable")]
    Cache,
}

impl MarkerIconService {
    pub(crate) fn new(arma_root: Option<PathBuf>) -> Result<Self, MarkerIconError> {
        let arma_root = arma_root.and_then(validate_arma_root);
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or(MarkerIconError::Cache)?;
        Ok(Self {
            arma_root,
            cache_root: local_app_data
                .join("cTabWeb")
                .join("marker-cache")
                .join("v1"),
            pbo_index: Arc::new(OnceLock::new()),
            mod_indices: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub(crate) async fn icon(
        &self,
        marker_type: &str,
        icon_path: &str,
    ) -> Result<Vec<u8>, MarkerIconError> {
        if !valid_marker_type(marker_type) || parse_icon_reference(icon_path).is_none() {
            return Err(MarkerIconError::InvalidRequest);
        }
        let service = self.clone();
        let marker_type = marker_type.to_owned();
        let icon_path = icon_path.to_owned();
        tokio::task::spawn_blocking(move || service.icon_blocking(&marker_type, &icon_path))
            .await
            .map_err(|_| MarkerIconError::Unavailable)?
    }

    fn icon_blocking(
        &self,
        marker_type: &str,
        icon_path: &str,
    ) -> Result<Vec<u8>, MarkerIconError> {
        let reference = parse_icon_reference(icon_path).ok_or(MarkerIconError::InvalidRequest)?;
        let mut reference_hasher = std::collections::hash_map::DefaultHasher::new();
        icon_path.hash(&mut reference_hasher);
        let cache_path = self.cache_root.join(format!(
            "{marker_type}_{:016x}.png",
            reference_hasher.finish()
        ));
        if let Ok(cached) = std::fs::read(&cache_path)
            && valid_cached_png(&cached)
        {
            return Ok(cached);
        }
        let arma_root = self
            .arma_root
            .as_ref()
            .ok_or(MarkerIconError::Unavailable)?;
        let paa = match reference {
            IconReference::BaseGame(virtual_path) => {
                let index = self.pbo_index.get_or_init(|| build_pbo_index(arma_root));
                extract_virtual_file(index, &virtual_path)?
            }
            IconReference::Mod(reference) => {
                let index = self.mod_index(arma_root, &reference)?;
                extract_virtual_file(&index, &reference.virtual_path)?
            }
        };
        let png = decode_paa_to_png(&paa)?;
        write_cache_file(&cache_path, &png)?;
        cache::enforce_limit(
            &self.cache_root,
            MARKER_CACHE_LIMIT_BYTES,
            Some(&cache_path),
        );
        Ok(png)
    }

    pub(crate) async fn cache_stats(&self) -> CacheStats {
        let root = self.cache_root.clone();
        tokio::task::spawn_blocking(move || cache::stats(&root))
            .await
            .unwrap_or_default()
    }

    fn mod_index(
        &self,
        arma_root: &Path,
        reference: &ModIconReference,
    ) -> Result<Arc<Vec<PboLocation>>, MarkerIconError> {
        let key = format!(
            "{}:{}:{}",
            reference.mod_directory.to_ascii_lowercase(),
            reference.workshop_id,
            reference.mod_hash.to_ascii_lowercase()
        );
        let mut indices = self
            .mod_indices
            .lock()
            .map_err(|_| MarkerIconError::Unavailable)?;
        if let Some(index) = indices.get(&key) {
            return Ok(Arc::clone(index));
        }
        if indices.len() >= MAX_MOD_INDICES {
            return Err(MarkerIconError::Unavailable);
        }
        let addon_directories = mod_addon_directories(arma_root, reference);
        if addon_directories.is_empty() {
            return Err(MarkerIconError::Unavailable);
        }
        let index = Arc::new(build_pbo_index_from_directories(addon_directories));
        if index.is_empty() {
            return Err(MarkerIconError::Unavailable);
        }
        indices.insert(key, Arc::clone(&index));
        Ok(index)
    }
}

fn validate_arma_root(path: PathBuf) -> Option<PathBuf> {
    if !path.is_absolute() || !path.join("arma3_x64.exe").is_file() || !path.join("Addons").is_dir()
    {
        return None;
    }
    path.canonicalize().ok()
}

fn valid_marker_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn parse_icon_reference(value: &str) -> Option<IconReference> {
    if value.is_empty() || value.len() > 512 || value.contains('\0') || value.contains(':') {
        return None;
    }
    if value.starts_with('[') {
        let descriptor = serde_json::from_str::<Vec<String>>(value).ok()?;
        if descriptor.len() != 5
            || descriptor.first().map(String::as_str) != Some(MOD_ICON_DESCRIPTOR)
        {
            return None;
        }
        let mod_directory = descriptor[1].clone();
        let workshop_id = descriptor[2].clone();
        let mod_hash = descriptor[3].clone();
        let virtual_path = normalize_virtual_path(&descriptor[4], false)?;
        if !valid_mod_directory(&mod_directory)
            || !valid_workshop_id(&workshop_id)
            || !valid_mod_hash(&mod_hash)
        {
            return None;
        }
        return Some(IconReference::Mod(ModIconReference {
            mod_directory,
            workshop_id,
            mod_hash,
            virtual_path,
        }));
    }
    normalize_virtual_path(value, true).map(IconReference::BaseGame)
}

fn normalize_virtual_path(value: &str, require_a3: bool) -> Option<String> {
    if value.is_empty() || value.len() > 512 || value.contains('\0') || value.contains(':') {
        return None;
    }
    let normalized = value
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\")
        .to_ascii_lowercase();
    let components: Vec<_> = normalized.split('\\').collect();
    if (require_a3 && components.first() != Some(&"a3"))
        || components
            .iter()
            .any(|part| part.is_empty() || matches!(*part, "." | ".."))
        || !normalized.ends_with(".paa")
    {
        return None;
    }
    Some(normalized)
}

fn valid_mod_directory(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.ends_with([' ', '.'])
        && value.chars().all(|character| {
            !character.is_control()
                && !matches!(
                    character,
                    '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
        && !matches!(value, "." | "..")
}

fn valid_workshop_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 20 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_mod_hash(value: &str) -> bool {
    value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn mod_addon_directories(arma_root: &Path, reference: &ModIconReference) -> Vec<PathBuf> {
    let mut mod_roots = Vec::new();
    if reference.workshop_id != "0"
        && let Some(steamapps) = arma_root.parent().and_then(Path::parent)
    {
        mod_roots.push(
            steamapps
                .join("workshop")
                .join("content")
                .join("107410")
                .join(&reference.workshop_id),
        );
    }
    mod_roots.push(arma_root.join(&reference.mod_directory));
    mod_roots.push(arma_root.join("!Workshop").join(&reference.mod_directory));

    let mut addon_directories = Vec::new();
    for mod_root in mod_roots {
        for directory_name in ["Addons", "addons"] {
            let candidate = mod_root.join(directory_name);
            if candidate.is_dir()
                && let Ok(canonical) = candidate.canonicalize()
                && !addon_directories.contains(&canonical)
            {
                addon_directories.push(canonical);
            }
        }
    }
    addon_directories
}

fn build_pbo_index(arma_root: &Path) -> Vec<PboLocation> {
    let mut addon_directories = vec![arma_root.join("Addons")];
    if let Ok(children) = std::fs::read_dir(arma_root) {
        for child in children.flatten().take(256) {
            let addons = child.path().join("Addons");
            if addons.is_dir() {
                addon_directories.push(addons);
            }
        }
    }
    build_pbo_index_from_directories(addon_directories)
}

fn build_pbo_index_from_directories(addon_directories: Vec<PathBuf>) -> Vec<PboLocation> {
    let mut locations = Vec::new();
    for directory in addon_directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if locations.len() >= MAX_PBO_FILES {
                break;
            }
            let path = entry.path();
            let is_pbo = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pbo"));
            if is_pbo && let Ok(prefix) = read_pbo_prefix(&path) {
                locations.push(PboLocation { path, prefix });
            }
        }
    }
    locations.sort_by_key(|location| std::cmp::Reverse(location.prefix.len()));
    locations
}

fn read_pbo_prefix(path: &Path) -> Result<String, MarkerIconError> {
    let file = File::open(path).map_err(|_| MarkerIconError::Unavailable)?;
    let mut reader = BufReader::new(file);
    let first_name = read_c_string(&mut reader, 1024)?;
    let (mime, _, _, _, _) = read_entry_fields(&mut reader)?;
    if !first_name.is_empty() || &mime != b"sreV" {
        return path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase())
            .ok_or(MarkerIconError::Unsupported);
    }
    let mut prefix = None;
    loop {
        let key = read_c_string(&mut reader, 1024)?;
        if key.is_empty() {
            break;
        }
        let value = read_c_string(&mut reader, 2048)?;
        if key.eq_ignore_ascii_case("prefix") {
            prefix = normalize_pbo_component(&value);
        }
        if reader
            .stream_position()
            .map_err(|_| MarkerIconError::Unsupported)?
            > MAX_PBO_HEADER_BYTES
        {
            return Err(MarkerIconError::Unsupported);
        }
    }
    prefix.ok_or(MarkerIconError::Unsupported)
}

fn normalize_pbo_component(value: &str) -> Option<String> {
    let normalized = value
        .trim_matches(['\\', '/'])
        .replace('/', "\\")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.contains(':')
        || normalized
            .split('\\')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        None
    } else {
        Some(normalized)
    }
}

fn extract_virtual_file(
    index: &[PboLocation],
    virtual_path: &str,
) -> Result<Vec<u8>, MarkerIconError> {
    for location in index {
        let Some(relative) = virtual_path
            .strip_prefix(&location.prefix)
            .and_then(|suffix| suffix.strip_prefix('\\'))
        else {
            continue;
        };
        if let Ok(bytes) = extract_pbo_entry(&location.path, relative) {
            return Ok(bytes);
        }
    }
    Err(MarkerIconError::Unavailable)
}

fn extract_pbo_entry(path: &Path, requested: &str) -> Result<Vec<u8>, MarkerIconError> {
    let file = File::open(path).map_err(|_| MarkerIconError::Unavailable)?;
    let file_length = file
        .metadata()
        .map_err(|_| MarkerIconError::Unavailable)?
        .len();
    let mut reader = BufReader::new(file);
    let first_name = read_c_string(&mut reader, 1024)?;
    let (first_mime, first_original, _, _, first_size) = read_entry_fields(&mut reader)?;
    if first_name.is_empty() && &first_mime == b"sreV" {
        loop {
            let key = read_c_string(&mut reader, 1024)?;
            if key.is_empty() {
                break;
            }
            let _ = read_c_string(&mut reader, 2048)?;
        }
    } else if !first_name.is_empty() || first_original != 0 || first_size != 0 {
        return Err(MarkerIconError::Unsupported);
    }

    let mut entries = Vec::new();
    for _ in 0..MAX_PBO_ENTRIES {
        let name = read_c_string(&mut reader, 4096)?;
        let (mime, original_size, _, _, data_size) = read_entry_fields(&mut reader)?;
        if name.is_empty() {
            break;
        }
        entries.push((
            name.replace('/', "\\").to_ascii_lowercase(),
            mime,
            original_size,
            data_size,
        ));
        if reader
            .stream_position()
            .map_err(|_| MarkerIconError::Unsupported)?
            > MAX_PBO_HEADER_BYTES
        {
            return Err(MarkerIconError::Unsupported);
        }
    }
    let data_start = reader
        .stream_position()
        .map_err(|_| MarkerIconError::Unsupported)?;
    let mut offset = data_start;
    for (name, mime, original_size, data_size) in entries {
        let size = usize::try_from(data_size).map_err(|_| MarkerIconError::Unsupported)?;
        if name.eq_ignore_ascii_case(requested) {
            if size == 0 || size > MAX_ENTRY_BYTES || &mime == b"srpC" || original_size > data_size
            {
                return Err(MarkerIconError::Unsupported);
            }
            let end = offset
                .checked_add(u64::from(data_size))
                .ok_or(MarkerIconError::Unsupported)?;
            if end > file_length {
                return Err(MarkerIconError::Unsupported);
            }
            reader
                .seek(SeekFrom::Start(offset))
                .map_err(|_| MarkerIconError::Unavailable)?;
            let mut output = vec![0_u8; size];
            reader
                .read_exact(&mut output)
                .map_err(|_| MarkerIconError::Unavailable)?;
            return Ok(output);
        }
        offset = offset
            .checked_add(u64::from(data_size))
            .ok_or(MarkerIconError::Unsupported)?;
    }
    Err(MarkerIconError::Unavailable)
}

fn read_entry_fields(
    reader: &mut impl Read,
) -> Result<([u8; 4], u32, u32, u32, u32), MarkerIconError> {
    let mut fields = [0_u8; 20];
    reader
        .read_exact(&mut fields)
        .map_err(|_| MarkerIconError::Unsupported)?;
    Ok((
        fields[0..4].try_into().expect("four MIME bytes"),
        u32::from_le_bytes(fields[4..8].try_into().expect("four original-size bytes")),
        u32::from_le_bytes(fields[8..12].try_into().expect("four offset bytes")),
        u32::from_le_bytes(fields[12..16].try_into().expect("four timestamp bytes")),
        u32::from_le_bytes(fields[16..20].try_into().expect("four data-size bytes")),
    ))
}

fn read_c_string(reader: &mut impl Read, limit: usize) -> Result<String, MarkerIconError> {
    let mut bytes = Vec::new();
    for _ in 0..=limit {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|_| MarkerIconError::Unsupported)?;
        if byte[0] == 0 {
            return String::from_utf8(bytes).map_err(|_| MarkerIconError::Unsupported);
        }
        bytes.push(byte[0]);
    }
    Err(MarkerIconError::Unsupported)
}

pub(crate) fn decode_paa_to_png(bytes: &[u8]) -> Result<Vec<u8>, MarkerIconError> {
    if bytes.len() < 16 || bytes.len() > MAX_ENTRY_BYTES {
        return Err(MarkerIconError::Unsupported);
    }
    let format = match u16::from_le_bytes([bytes[0], bytes[1]]) {
        0xff01 => Format::Bc1,
        0xff05 => Format::Bc3,
        _ => return Err(MarkerIconError::Unsupported),
    };
    let mut cursor = 2_usize;
    while bytes.get(cursor..cursor + 4) == Some(b"GGAT") {
        cursor = cursor.checked_add(8).ok_or(MarkerIconError::Unsupported)?;
        let length = read_u32(bytes, cursor)? as usize;
        if length > MAX_TAG_BYTES {
            return Err(MarkerIconError::Unsupported);
        }
        cursor = cursor
            .checked_add(4 + length)
            .ok_or(MarkerIconError::Unsupported)?;
        if cursor > bytes.len() {
            return Err(MarkerIconError::Unsupported);
        }
    }
    let palette_length = usize::from(read_u16(bytes, cursor)?);
    cursor = cursor
        .checked_add(2 + palette_length)
        .ok_or(MarkerIconError::Unsupported)?;
    let raw_width = read_u16(bytes, cursor)?;
    let width = usize::from(raw_width & 0x7fff);
    let height = usize::from(read_u16(bytes, cursor + 2)?);
    let data_size = read_u24(bytes, cursor + 4)?;
    cursor = cursor.checked_add(7).ok_or(MarkerIconError::Unsupported)?;
    if width == 0 || height == 0 || width > MAX_PAA_DIMENSION || height > MAX_PAA_DIMENSION {
        return Err(MarkerIconError::Unsupported);
    }
    let block_bytes = match format {
        Format::Bc1 => 8,
        Format::Bc3 => 16,
        _ => unreachable!(),
    };
    let expected = width
        .div_ceil(4)
        .checked_mul(height.div_ceil(4))
        .and_then(|blocks| blocks.checked_mul(block_bytes))
        .ok_or(MarkerIconError::Unsupported)?;
    let end = cursor
        .checked_add(data_size)
        .ok_or(MarkerIconError::Unsupported)?;
    let stored = bytes.get(cursor..end).ok_or(MarkerIconError::Unsupported)?;
    let decoded_storage;
    let texture = if raw_width & 0x8000 != 0 {
        decoded_storage =
            lzo::decompress(stored, expected).map_err(|_| MarkerIconError::Unsupported)?;
        decoded_storage.as_slice()
    } else {
        stored
    };
    if texture.len() != expected {
        return Err(MarkerIconError::Unsupported);
    }
    let pixel_bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(MarkerIconError::Unsupported)?;
    let mut rgba = vec![0_u8; pixel_bytes];
    format.decompress(texture, width, height, &mut rgba);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&rgba, width as u32, height as u32, ExtendedColorType::Rgba8)
        .map_err(|_| MarkerIconError::Unsupported)?;
    Ok(png)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, MarkerIconError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(MarkerIconError::Unsupported)?;
    Ok(u16::from_le_bytes(value.try_into().expect("two bytes")))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MarkerIconError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(MarkerIconError::Unsupported)?;
    Ok(u32::from_le_bytes(value.try_into().expect("four bytes")))
}
fn read_u24(bytes: &[u8], offset: usize) -> Result<usize, MarkerIconError> {
    let value = bytes
        .get(offset..offset + 3)
        .ok_or(MarkerIconError::Unsupported)?;
    Ok(usize::from(value[0]) | (usize::from(value[1]) << 8) | (usize::from(value[2]) << 16))
}

fn valid_cached_png(bytes: &[u8]) -> bool {
    if bytes.len() < 24 || bytes[..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return false;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("four width bytes"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("four height bytes"));
    width > 0
        && height > 0
        && width <= MAX_PAA_DIMENSION as u32
        && height <= MAX_PAA_DIMENSION as u32
}

fn write_cache_file(path: &Path, bytes: &[u8]) -> Result<(), MarkerIconError> {
    let parent = path.parent().ok_or(MarkerIconError::Cache)?;
    std::fs::create_dir_all(parent).map_err(|_| MarkerIconError::Cache)?;
    let mut suffix = [0_u8; 8];
    getrandom::fill(&mut suffix).map_err(|_| MarkerIconError::Cache)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or(MarkerIconError::Cache)?,
        suffix
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ));
    std::fs::write(&temporary, bytes).map_err(|_| MarkerIconError::Cache)?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) if path.is_file() => {
            let _ = std::fs::remove_file(temporary);
            Ok(())
        }
        Err(_) => {
            let _ = std::fs::remove_file(temporary);
            Err(MarkerIconError::Cache)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_virtual_paths_and_marker_types() {
        assert!(normalize_virtual_path(r"\A3\ui_f\data\marker.paa", true).is_some());
        assert!(normalize_virtual_path(r"custom_mod\data\marker.paa", true).is_none());
        assert!(normalize_virtual_path(r"..\secret.paa", false).is_none());
        assert!(normalize_virtual_path(r"C:\secret.paa", false).is_none());
        assert!(valid_marker_type("mil_unknown"));
        assert!(!valid_marker_type("../unknown"));
    }

    #[test]
    fn accepts_only_bounded_mod_icon_descriptors() {
        let descriptor = serde_json::to_string(&[
            MOD_ICON_DESCRIPTOR,
            "@marker_pack",
            "3123456789",
            "7de4bd5c",
            r"marker_pack\data\artillery_ca.paa",
        ])
        .expect("serialize descriptor");
        assert_eq!(
            parse_icon_reference(&descriptor),
            Some(IconReference::Mod(ModIconReference {
                mod_directory: "@marker_pack".to_owned(),
                workshop_id: "3123456789".to_owned(),
                mod_hash: "7de4bd5c".to_owned(),
                virtual_path: r"marker_pack\data\artillery_ca.paa".to_owned(),
            }))
        );

        let traversal = serde_json::to_string(&[
            MOD_ICON_DESCRIPTOR,
            "..",
            "3123456789",
            "7de4bd5c",
            r"marker_pack\data\artillery_ca.paa",
        ])
        .expect("serialize traversal descriptor");
        assert!(parse_icon_reference(&traversal).is_none());
        assert!(parse_icon_reference(r"marker_pack\data\artillery_ca.paa").is_none());
    }

    #[test]
    fn accepts_safe_workshop_directory_punctuation_without_allowing_paths() {
        assert!(valid_mod_directory("@O&T Expansion Eden"));
        assert!(valid_mod_directory("@3AS (Beta Test)"));
        assert!(valid_mod_directory("@WebKnight's Zombies and Creatures"));
        assert!(!valid_mod_directory(r"..\secret"));
        assert!(!valid_mod_directory("../secret"));
        assert!(!valid_mod_directory("C:secret"));
        assert!(!valid_mod_directory("trailing."));
        assert!(!valid_mod_directory("trailing "));
    }

    #[test]
    fn resolves_only_the_reported_workshop_mod_addons_directory() {
        let temporary = tempfile::tempdir().expect("create temporary Steam library");
        let steamapps = temporary.path().join("steamapps");
        let arma_root = steamapps.join("common").join("Arma 3");
        let workshop_addons = steamapps
            .join("workshop")
            .join("content")
            .join("107410")
            .join("3123456789")
            .join("Addons");
        std::fs::create_dir_all(&arma_root).expect("create Arma root");
        std::fs::create_dir_all(&workshop_addons).expect("create Workshop Addons");
        let reference = ModIconReference {
            mod_directory: "@marker_pack".to_owned(),
            workshop_id: "3123456789".to_owned(),
            mod_hash: "7de4bd5c".to_owned(),
            virtual_path: r"marker_pack\data\artillery_ca.paa".to_owned(),
        };

        let directories = mod_addon_directories(&arma_root, &reference);

        assert_eq!(
            directories,
            vec![
                workshop_addons
                    .canonicalize()
                    .expect("canonical Workshop path")
            ]
        );
    }

    #[test]
    fn extracts_a_marker_from_the_reported_workshop_mod_pbo() {
        let temporary = tempfile::tempdir().expect("create temporary Steam library");
        let steamapps = temporary.path().join("steamapps");
        let arma_root = steamapps.join("common").join("Arma 3");
        let workshop_addons = steamapps
            .join("workshop")
            .join("content")
            .join("107410")
            .join("3123456789")
            .join("Addons");
        std::fs::create_dir_all(arma_root.join("Addons")).expect("create Arma Addons");
        std::fs::create_dir_all(&workshop_addons).expect("create Workshop Addons");
        let pbo_path = workshop_addons.join("marker_pack.pbo");
        write_test_pbo(
            &pbo_path,
            "marker_pack",
            r"data\artillery_ca.paa",
            &test_bc1_paa(),
        );
        let service = MarkerIconService {
            arma_root: Some(arma_root),
            cache_root: temporary.path().join("cache"),
            pbo_index: Arc::new(OnceLock::new()),
            mod_indices: Arc::new(Mutex::new(HashMap::new())),
        };
        let descriptor = serde_json::to_string(&[
            MOD_ICON_DESCRIPTOR,
            "@marker_pack",
            "3123456789",
            "7de4bd5c",
            r"marker_pack\data\artillery_ca.paa",
        ])
        .expect("serialize descriptor");

        let png = service
            .icon_blocking("mod_artillery", &descriptor)
            .expect("extract and convert Workshop marker");

        assert!(valid_cached_png(&png));
    }

    fn write_test_pbo(path: &Path, prefix: &str, entry_name: &str, entry_data: &[u8]) {
        let mut pbo = Vec::new();
        pbo.push(0);
        pbo.extend_from_slice(b"sreV");
        pbo.extend_from_slice(&[0_u8; 16]);
        pbo.extend_from_slice(b"prefix\0");
        pbo.extend_from_slice(prefix.as_bytes());
        pbo.push(0);
        pbo.push(0);
        pbo.extend_from_slice(entry_name.as_bytes());
        pbo.push(0);
        pbo.extend_from_slice(&[0_u8; 4]);
        pbo.extend_from_slice(&(entry_data.len() as u32).to_le_bytes());
        pbo.extend_from_slice(&[0_u8; 8]);
        pbo.extend_from_slice(&(entry_data.len() as u32).to_le_bytes());
        pbo.push(0);
        pbo.extend_from_slice(&[0_u8; 20]);
        pbo.extend_from_slice(entry_data);
        std::fs::write(path, pbo).expect("write test PBO");
    }

    fn test_bc1_paa() -> Vec<u8> {
        let mut paa = vec![0x01, 0xff, 0, 0, 4, 0, 4, 0, 8, 0, 0];
        paa.extend_from_slice(&[0xff, 0xff, 0, 0, 0, 0, 0, 0]);
        paa
    }

    #[test]
    #[ignore = "requires CTAB_ARMA_ROOT to point to a local Arma 3 installation"]
    fn converts_a_local_base_game_marker_without_staging_game_content() {
        let arma_root = std::env::var_os("CTAB_ARMA_ROOT")
            .map(PathBuf::from)
            .expect("CTAB_ARMA_ROOT is required");
        let ui_data = arma_root.join("Addons").join("ui_f_data.pbo");
        assert_eq!(
            read_pbo_prefix(&ui_data).expect("read ui_f_data prefix"),
            r"a3\ui_f\data"
        );
        let service = MarkerIconService::new(Some(arma_root)).expect("create marker icon service");
        let png = service
            .icon_blocking(
                "mil_unknown",
                r"\A3\ui_f\data\map\markers\military\unknown_CA.paa",
            )
            .expect("convert the base-game marker");
        let nato_png = service
            .icon_blocking("b_inf", r"\A3\ui_f\data\map\markers\nato\b_inf.paa")
            .expect("convert a multitone NATO marker");
        let reused_task_icons = [
            (
                "loc_Rifle",
                r"\A3\ui_f\data\igui\cfg\simpletasks\types\rifle_ca.paa",
            ),
            (
                "loc_car",
                r"\A3\ui_f\data\igui\cfg\simpletasks\types\car_ca.paa",
            ),
            (
                "loc_LZ",
                r"\A3\ui_f\data\igui\cfg\simpletasks\types\land_ca.paa",
            ),
        ];
        for (marker_type, icon_path) in reused_task_icons {
            let task_png = service
                .icon_blocking(marker_type, icon_path)
                .expect("convert a base-game task icon reused by a mod marker");
            assert!(valid_cached_png(&task_png));
        }

        assert!(valid_cached_png(&png));
        assert!(valid_cached_png(&nato_png));
    }
}
