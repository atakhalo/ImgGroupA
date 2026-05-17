# Rust 后端文档

## 依赖

| Crate                     | 用途                                 |
| ------------------------- | ------------------------------------ |
| `tauri`                   | 桌面框架核心                         |
| `tauri-plugin-dialog`     | 原生文件夹/文件选择对话框            |
| `tauri-plugin-opener`     | 系统默认打开方式                     |
| `serde` / `serde_json`    | 序列化                               |
| `serde_yaml`              | YAML 配置文件读写                    |
| `base64`                  | 图片文件 base64 编码                 |
| `iptc`                    | IPTC 元数据读取                      |
| `kamadak-exif`            | EXIF 元数据读取                      |

## 数据结构

```rust
struct ImageInfo {
    name: String,            // 文件名（例如 "photo.jpg"）
    path: String,            // 完整文件路径
    data_url: String,        // data:image/png;base64,...
    relative_folder: String,  // 相对于根目录的路径（空字符串表示根目录）
    file_size: u64,          // 文件大小（字节）
}

struct LoadImagesResult {
    folder_path: String,  // 用户选择的文件夹路径 / "__temp__" 表示临时分组
    images: Vec<ImageInfo>,
}

struct StartupPaths(Vec<String>);  // CLI 参数传入的所有路径
```

## Tauri 命令

### `load_images` — 加载文件夹

```
异步命令，接收 AppHandle，返回 Result<LoadImagesResult, String>
```

**流程：**

1. 通过 `dialog().file().pick_folder()` 打开原生文件夹选择器
2. 使用 `mpsc::channel` 等待用户选择结果（回调转异步）
3. 用户取消时返回 `"CANCELLED"` 错误（前端静默处理）
4. 调用 `scan_directory()` 递归扫描所有子目录
5. 对每个文件：
   - 检查扩展名是否属于支持格式
   - 用 `fs::read()` 读取二进制
   - 用 `base64::STANDARD.encode()` 编码
   - 拼接 `data:{mime};base64,{encoded}` 格式的 Data URL
   - 计算 `relative_folder`（相对于根目录的路径）
   - 记录 `file_size` 和 `modified_date`

### `load_image_files` — 多选图片文件

```
异步命令，接收 AppHandle，返回 Result<LoadImagesResult, String>
```

**流程：**

1. 通过 `dialog().file().add_filter().pick_files()` 打开多选对话框
2. 文件过滤器限定为图片格式
3. 对每个选中文件执行 **读取 → 编码 → 组装**
4. `folder_path` 固定为 `"__temp__"`（前端识别后替换为翻译后的临时分组名）

### `load_dropped_files` — 加载拖放文件 / CLI 参数

```
同步命令，接收 paths: Vec<String>，返回 Result<LoadImagesResult, String>
```

**流程：**

1. 遍历所有传入路径，判断是否为目录
2. 目录 → `scan_directory()` 递归扫描
3. 文件 → `load_single_image()` 加载单个图片
4. 单文件夹 → `folder_path` 为文件夹路径；多路径/文件 → `"__temp__"`
5. 找不到图片时返回错误

### `load_images_from_path` — 从指定路径加载（无对话框）

```
同步命令，接收 path: String，返回 Result<LoadImagesResult, String>
```

支持文件或目录：
- 目录 → `scan_directory()` 递归扫描
- 文件 → `load_single_image()` 加载

### `get_startup_paths` — 获取 CLI 启动参数

```
同步命令，接收 tauri::State<StartupPaths>，返回 Vec<String>
```

返回程序启动时传入的所有非 `--` 开头的命令行参数。

### `open_file_default` — 系统默认打开

```
异步命令，接收 path: String，返回 Result<(), String>
```

**流程：**

1. Windows 上使用 `cmd /c start "" "filePath"`
2. macOS/Linux 上使用 `open "filePath"`

### `read_image_metadata` — 读取 IPTC + EXIF 元数据

```
同步命令，接收 path: String，返回 Result<MetadataResult, String>
```

**数据结构：**

```rust
struct MetadataField {
    name: String,   // 字段名，如 "Caption (0x9286)"
    value: String,  // 字段值
    source: String, // "IPTC" 或 "EXIF"
}

struct MetadataResult {
    fields: Vec<MetadataField>,
}
```

**IPTC 读取**：使用 `iptc::IPTC::read_from_path()` 获取所有 IPTC 字段

**EXIF 读取**：使用 `exif::Reader` 读取全部 EXIF 字段

**UserComment 特殊处理**：由于 `UserComment` 存储为 `Undefined` 原始字节，实现 `extract_from_undefined()` 函数尝试 UTF-8 → UTF-16LE → ASCII 三级解码

### `toggle_fullscreen` — 切换全屏

```
异步命令，接收 window: tauri::Window，返回 Result<bool, String>
```

使用 Tauri 原生窗口 API `window.setFullscreen()` 切换全屏状态，并返回当前是否全屏。

## 支持的图片格式

| 扩展名    | MIME 类型          |
| --------- | ------------------ |
| jpg/jpeg  | image/jpeg         |
| png       | image/png          |
| gif       | image/gif          |
| bmp       | image/bmp          |
| webp      | image/webp         |
| svg       | image/svg+xml      |
| ico       | image/x-icon       |

## 关键实现细节

### 递归扫描 (scan_directory)

```rust
fn scan_directory(root: &Path, current: &Path, images: &mut Vec<ImageInfo>)
```

- `root` 是用户选择的根目录，`current` 是当前遍历的目录
- 遇到子目录则递归调用
- 相对路径通过 `path.strip_prefix(root)` 计算
- 路径分隔符统一转为 `/`

### 对话框回调转 async

Tauri 2 的 dialog API 使用回调形式而非 async，因此通过 `mpsc::channel` 桥接：

```rust
let (tx, rx) = mpsc::channel();
app.dialog().file().pick_folder(move |path| {
    let _ = tx.send(path);
});
let result = rx.recv().unwrap();
```

### 权限声明

在 `capabilities/default.json` 中声明以下权限：

```json
{
  "permissions": [
    "core:default",
    "opener:default",
    "dialog:default"
  ]
}
```

- `core:default` — 基础窗口操作（含全屏）
- `dialog:default` — 文件夹/文件选择对话框
- `opener:default` — 在资源管理器中显示文件
