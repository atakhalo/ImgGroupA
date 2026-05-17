use std::fs;
use std::sync::mpsc;
use std::path::Path;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize, Clone)]
struct ImageInfo {
    name: String,
    path: String,
    data_url: String,
    relative_folder: String,
    file_size: u64,
    modified_date: u64,
}

fn get_modified_time(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Serialize)]
struct LoadImagesResult {
    folder_path: String,
    images: Vec<ImageInfo>,
}

fn mime_from_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        _ => None,
    }
}

/// 递归扫描目录，收集所有图片文件
fn scan_directory(root: &Path, current: &Path, images: &mut Vec<ImageInfo>) {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if path.is_dir() {
            scan_directory(root, &path, images);
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let mime = match mime_from_ext(&ext) {
            Some(m) => m,
            None => continue,
        };

        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let encoded = STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime, encoded);

        // 计算相对路径（相对于根目录，使用 / 分隔）
        let parent = path.parent().unwrap();
        let relative = if parent == root {
            String::new()
        } else {
            parent
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default()
        };

        let modified_date = get_modified_time(&path);

        images.push(ImageInfo {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: path.to_string_lossy().to_string(),
            data_url,
            relative_folder: relative,
            file_size: bytes.len() as u64,
            modified_date,
        });
    }
}

/// 选择文件夹并递归加载所有图片
#[tauri::command]
async fn load_images(app: tauri::AppHandle) -> Result<LoadImagesResult, String> {
    let (tx, rx) = mpsc::channel();
    app.dialog()
        .file()
        .pick_folder(move |path| {
            let _ = tx.send(path);
        });

    let folder_path = match rx.recv() {
        Ok(Some(path)) => path,
        Ok(None) => return Err("CANCELLED".to_string()),
        Err(_) => return Err("Dialog error".to_string()),
    };

    let folder_path_str = folder_path.to_string();
    let folder_path = Path::new(&folder_path_str);

    let mut images = Vec::new();
    scan_directory(folder_path, folder_path, &mut images);

    Ok(LoadImagesResult {
        folder_path: folder_path_str,
        images,
    })
}

/// 多选图片文件加载
#[tauri::command]
async fn load_image_files(app: tauri::AppHandle) -> Result<LoadImagesResult, String> {
    let (tx, rx) = mpsc::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["jpg", "jpeg", "png", "gif", "bmp", "webp", "svg", "ico"])
        .pick_files(move |paths| {
            let _ = tx.send(paths);
        });

    let files = match rx.recv() {
        Ok(Some(paths)) => paths,
        Ok(None) => return Err("CANCELLED".to_string()),
        Err(_) => return Err("Dialog error".to_string()),
    };

    let mut images = Vec::new();
    for file_path in files {
        let path_str = file_path.to_string();
        let path = Path::new(&path_str);

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let mime = match mime_from_ext(&ext) {
            Some(m) => m,
            None => continue,
        };

        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let encoded = STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime, encoded);

        let modified_date = get_modified_time(path);

        images.push(ImageInfo {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: path_str,
            data_url,
            relative_folder: String::new(),
            file_size: bytes.len() as u64,
            modified_date,
        });
    }

    Ok(LoadImagesResult {
        folder_path: "__temp__".to_string(),
        images,
    })
}

/// 加载单个图片文件
fn load_single_image(path: &Path) -> Option<ImageInfo> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let mime = mime_from_ext(&ext)?;

    let bytes = fs::read(path).ok()?;
    let encoded = STANDARD.encode(&bytes);
    let data_url = format!("data:{};base64,{}", mime, encoded);
    let modified_date = get_modified_time(path);

    Some(ImageInfo {
        name: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        path: path.to_string_lossy().to_string(),
        data_url,
        relative_folder: String::new(),
        file_size: bytes.len() as u64,
        modified_date,
    })
}

/// 处理拖入的文件和文件夹
#[tauri::command]
fn load_dropped_files(paths: Vec<String>) -> Result<LoadImagesResult, String> {
    let mut images = Vec::new();

    for path_str in &paths {
        let path = Path::new(path_str);
        if path.is_dir() {
            scan_directory(path, path, &mut images);
        } else if path.is_file() {
            if let Some(info) = load_single_image(path) {
                images.push(info);
            }
        }
    }

    if images.is_empty() {
        return Err("No images found in dropped content".to_string());
    }

    // 单个文件夹 → 使用文件夹路径；文件 → temp
    let folder_name = if paths.len() == 1 && Path::new(&paths[0]).is_dir() {
        paths[0].clone()
    } else {
        "__temp__".to_string()
    };

    Ok(LoadImagesResult {
        folder_path: folder_name,
        images,
    })
}

/// 使用系统默认程序打开文件
#[tauri::command]
async fn open_file_default(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Windows: 使用 cmd /c start "" "filePath"
        let result = std::process::Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn();
        if let Err(e) = result {
            return Err(format!("打开文件失败: {}", e));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let result = std::process::Command::new("open")
            .arg(&path)
            .spawn();
        if let Err(e) = result {
            return Err(format!("打开文件失败: {}", e));
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct MetadataField {
    name: String,
    value: String,
    source: String, // "IPTC" or "EXIF"
}

#[derive(Serialize)]
struct MetadataResult {
    fields: Vec<MetadataField>,
}

// ---- 设置系统 ----

#[derive(Serialize, Deserialize, Clone)]
struct GridSettings {
    #[serde(default = "default_border_radius")]
    border_radius: u32,
    #[serde(default = "default_gap")]
    gap: u32,
    #[serde(default = "default_min_width")]
    min_width: u32,
    #[serde(default = "default_background_color")]
    background_color: String,
    #[serde(default = "default_root_title_color")]
    root_title_color: String,
    #[serde(default = "default_root_title_bg")]
    root_title_bg: String,
    #[serde(default = "default_child_title_color")]
    child_title_color: String,
    #[serde(default = "default_child_title_bg")]
    child_title_bg: String,
    #[serde(default = "default_group_bg")]
    group_bg: String,
}

fn default_border_radius() -> u32 { 8 }
fn default_gap() -> u32 { 12 }
fn default_min_width() -> u32 { 200 }
fn default_background_color() -> String { "#0f1a30".to_string() }
fn default_root_title_color() -> String { "#cccccc".to_string() }
fn default_root_title_bg() -> String { "#1a2a4a".to_string() }
fn default_child_title_color() -> String { "#cccccc".to_string() }
fn default_child_title_bg() -> String { "#141e33".to_string() }
fn default_group_bg() -> String { "#141e33".to_string() }

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            border_radius: default_border_radius(),
            gap: default_gap(),
            min_width: default_min_width(),
            background_color: default_background_color(),
            root_title_color: default_root_title_color(),
            root_title_bg: default_root_title_bg(),
            child_title_color: default_child_title_color(),
            child_title_bg: default_child_title_bg(),
            group_bg: default_group_bg(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct ExternalProgram {
    name: String,
    path: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct FilterPreset {
    name: String,
    pattern: String,
    mode: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppSettings {
    #[serde(default)]
    grid: GridSettings,
    #[serde(default)]
    filter_presets: Vec<FilterPreset>,
    #[serde(default)]
    external_programs: Vec<ExternalProgram>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            grid: GridSettings::default(),
            filter_presets: Vec::new(),
            external_programs: Vec::new(),
        }
    }
}

fn config_path() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent().unwrap_or(Path::new(".")).join("config-user.yaml")
}

#[tauri::command]
fn read_settings() -> Result<AppSettings, String> {
    let path = config_path();
    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
    } else {
        Ok(AppSettings::default())
    }
}

#[tauri::command]
fn write_settings(settings: AppSettings) -> Result<(), String> {
    let path = config_path();
    let content = serde_yaml::to_string(&settings).map_err(|e| format!("Failed to serialize settings: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
async fn browse_file(app: tauri::AppHandle) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    app.dialog()
        .file()
        .pick_file(move |path| {
            let _ = tx.send(path);
        });
    match rx.recv() {
        Ok(Some(path)) => Ok(path.to_string()),
        Ok(None) => Err("CANCELLED".to_string()),
        Err(_) => Err("Dialog error".to_string()),
    }
}

#[tauri::command]
fn open_with_program(program_path: String, file_path: String) -> Result<(), String> {
    let result = std::process::Command::new(&program_path)
        .arg(&file_path)
        .spawn();
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to open: {}", e)),
    }
}

/// 读取图片所有 IPTC 和 EXIF 元数据
#[tauri::command]
fn read_image_metadata(path: String) -> Result<MetadataResult, String> {
    let p = std::path::Path::new(&path);
    let mut fields = Vec::new();

    // ---- IPTC ----
    if let Ok(iptc) = iptc::IPTC::read_from_path(p) {
        let all = iptc.get_all();
        for (tag, values) in all {
            for v in values {
                if !v.is_empty() {
                    fields.push(MetadataField {
                        name: format!("IPTC:{}", tag),
                        value: v,
                        source: "IPTC".to_string(),
                    });
                }
            }
        }
    }

    // ---- EXIF ----
    if let Ok(file) = std::fs::File::open(p) {
        let mut reader = std::io::BufReader::new(file);
        let exif_reader = exif::Reader::new();
        if let Ok(exif) = exif_reader.read_from_container(&mut reader) {
            for f in exif.fields() {
                let tag_display = format!("{}", f.tag);

                // UserComment 在 kamadak-exif 中 display_value 返回 hex dump，
                // 需要从 raw Undefined bytes 中手动解码
                let val = if is_user_comment_tag(&f.tag) {
                    extract_user_comment(&f.value)
                } else {
                    let v = f.display_value().to_string();
                    if v.is_empty() {
                        extract_from_undefined(&f.value)
                    } else {
                        v
                    }
                };

                if !val.is_empty() {
                    fields.push(MetadataField {
                        name: tag_display,
                        value: val,
                        source: "EXIF".to_string(),
                    });
                }
            }
        }
    }

    Ok(MetadataResult { fields })
}

/// 判断是否为 UserComment 标签 (0x9286, ExifIFD)
fn is_user_comment_tag(tag: &exif::Tag) -> bool {
    // Tag 是一个 struct: Tag(pub Context, pub u16)
    // UserComment = Tag(Context::Exif, 0x9286)
    tag.1 == 0x9286
}

/// 从 UserComment 的 Undefined 值中提取文本
/// 格式: 前 8 字节为编码标识，之后为实际文本
fn extract_user_comment(value: &exif::Value) -> String {
    use exif::Value;
    match value {
        Value::Undefined(buf, _) if buf.len() > 8 => {
            let encoding = &buf[..8];
            let text = &buf[8..];

            match encoding {
                // ASCII
                [0x41, 0x53, 0x43, 0x49, 0x49, 0x00, 0x00, 0x00] => {
                    if let Ok(s) = std::str::from_utf8(text) {
                        return s.trim_end_matches('\0').to_string();
                    }
                    String::from_utf8_lossy(text).trim_end_matches('\0').to_string()
                }
                // UNICODE (UCS-2/UTF-16, no BOM, byte order matches II vs MM not enforced here)
                [0x55, 0x4E, 0x49, 0x43, 0x4F, 0x44, 0x45, 0x00] => {
                    if text.len() >= 2 {
                        let utf16: Vec<u16> = text
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .take_while(|&c| c != 0)
                            .collect();
                        if let Ok(s) = String::from_utf16(&utf16) {
                            if !s.is_empty() {
                                return s;
                            }
                        }
                    }
                    // 回退
                    String::from_utf8_lossy(text).trim_end_matches('\0').to_string()
                }
                // JIS 或未定义编码，回退
                _ => {
                    if let Ok(s) = std::str::from_utf8(text) {
                        return s.trim_end_matches('\0').to_string();
                    }
                    String::from_utf8_lossy(text).trim_end_matches('\0').to_string()
                }
            }
        }
        _ => String::new(),
    }
}

/// 从 Exif Undefined 值中提取可读文本（通用回退）
fn extract_from_undefined(value: &exif::Value) -> String {
    use exif::Value;
    match value {
        Value::Undefined(buf, _) if buf.len() > 8 => {
            let text = &buf[8..]; // 前 8 字节是编码标识
            // 先尝试 UTF-8
            if let Ok(s) = std::str::from_utf8(text) {
                return s.trim_end_matches('\0').to_string();
            }
            // 尝试 UTF-16LE
            if text.len() >= 2 && text.len() % 2 == 0 {
                let utf16: Vec<u16> = text
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|&c| c != 0)
                    .collect();
                if let Ok(s) = String::from_utf16(&utf16) {
                    return s;
                }
            }
            // 回退：显示可打印 ASCII
            let printable: String = text.iter().take(200).map(|&c| if c.is_ascii_graphic() || c == b' ' { c as char } else { '.' }).collect();
            printable
        }
        _ => String::new(),
    }
}

// ---- CLI arg support ----

struct StartupPaths(Vec<String>);

/// Get all paths passed as command-line arguments
#[tauri::command]
fn get_startup_paths(state: tauri::State<StartupPaths>) -> Vec<String> {
    state.0.clone()
}

/// Load images from a specific path (file or directory, no dialog)
#[tauri::command]
fn load_images_from_path(path: String) -> Result<LoadImagesResult, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("Path does not exist: {}", path));
    }

    let mut images = Vec::new();
    if p.is_dir() {
        scan_directory(p, p, &mut images);
    } else if p.is_file() {
        if let Some(info) = load_single_image(p) {
            images.push(info);
        }
    }

    if images.is_empty() {
        return Err("No valid image files found".to_string());
    }

    Ok(LoadImagesResult {
        folder_path: path,
        images,
    })
}

/// 删除指定的文件列表（移入回收站）
#[tauri::command]
fn delete_files(paths: Vec<String>) -> Result<(), String> {
    let paths: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    trash::delete_all(&paths).map_err(|e| format!("{}", e))
}

/// 切换全屏状态
#[tauri::command]
async fn toggle_fullscreen(window: tauri::Window) -> Result<bool, String> {
    let is_full = window.is_fullscreen().map_err(|e| e.to_string())?;
    window.set_fullscreen(!is_full).map_err(|e| e.to_string())?;
    Ok(!is_full)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    // Collect all non-flag args after the executable path
    let startup_paths: Vec<String> = args.iter()
        .skip(1)
        .filter(|p| !p.starts_with("--"))
        .cloned()
        .collect();

    tauri::Builder::default()
        .manage(StartupPaths(startup_paths))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_images,
            load_image_files,
            load_dropped_files,
            load_images_from_path,
            get_startup_paths,
            open_file_default,
            read_image_metadata,
            toggle_fullscreen,
            read_settings,
            write_settings,
            browse_file,
            open_with_program,
            delete_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
