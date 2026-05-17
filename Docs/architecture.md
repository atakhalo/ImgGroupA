# 系统架构

## 技术栈

| 层       | 技术                                   |
| -------- | -------------------------------------- |
| 桌面框架 | Tauri 2.x                              |
| 前端     | Vue 3 + TypeScript + Vite 6            |
| 国际化   | vue-i18n 9（中/英双语）                 |
| 后端     | Rust (Tauri 命令)                      |
| 对话框  | tauri-plugin-dialog                    |
| 图片编码 | base64 (Rust crate)                    |
| 元数据   | kamadak-exif, iptc crate               |
| 配置持久 | serde_yaml (config-user.yaml)          |
| 参数解析 | std::env::args (支持单图/多图/文件夹)  |

## 项目结构

```
imggroup/
├── src/                    # Vue 前端
│   ├── App.vue             # 主组件（视图 + 逻辑 + 样式，含大图查看器）
│   ├── main.ts             # Vue 入口
│   ├── i18n.ts             # vue-i18n 配置（语言检测、持久化）
│   ├── locales/            # 翻译文件
│   │   ├── zh.json         # 中文翻译
│   │   └── en.json         # 英文翻译
│   └── vite-env.d.ts       # Vite 类型声明
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── lib.rs          # Tauri 命令实现（12 个命令）
│   │   └── main.rs         # 程序入口
│   ├── Cargo.toml          # Rust 依赖（含 iptc, exif, serde_yaml）
│   ├── tauri.conf.json     # Tauri 配置
│   └── capabilities/       # 权限声明
├── Docs/                   # 文档
├── index.html              # HTML 入口
├── package.json            # Node 依赖
└── vite.config.ts          # Vite 配置
```

## 架构图

```
┌─────────────────────────────────────────────────────┐
│                  用户界面 (Vue)                       │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │  顶部工具栏   │  │  内容展示区   │  │  底部控制栏  │ │
│  │ - 加载文件夹  │  │ - 网格视图   │  │ - 分组开关  │ │
│  │ - 选择图片   │  │ - 树状分组   │  │ - 折叠/展开 │ │
│  │ - 追加/重置  │  │ - 骨架占位   │  │ - 统计信息  │ │
│  │             │  │ - 欢迎页     │  │             │ │
│  └─────────────┘  └──────┬───────┘  └─────────────┘ │
│                          │                           │
│  ┌───────────────────────▼─────────────────────────┐ │
│  │              大图查看器 Overlay                   │ │
│  │  - 全窗口显示  - 滚轮缩放  - 左右切换             │ │
│  │  - 适应窗口/短边适应/100%  - 全屏                 │ │
│  │  - 悬浮信息栏  - 元信息弹窗  - 操作栏(hover显示) │ │
│  │  - 资源管理器显示  - 默认打开  - 拖拽             │ │
│  └─────────────────────────────────────────────────┘ │
└──────────────────────┬──────────────────────────────┘
                       │ invoke()
                       ▼
┌─────────────────────────────────────────────────────┐
│                Tauri IPC 通信层                       │
└──────────────────────┬──────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────┐
│                   Rust 后端                           │
│  ┌─────────────────────────────────────────────────┐ │
│  │  load_images        (文件夹选择 + 递归扫描)       │ │
│  │  load_image_files   (多文件选择)                  │ │
│  │  open_file_default  (系统默认打开)                │ │
│  │  read_image_metadata(IPTC + EXIF 读取)           │ │
│  │  load_dropped_files   (拖放/CLI 混合处理)         │ │
│  │  load_images_from_path(指定路径加载，支持文件/目录)  │ │
│  │  get_startup_paths    (获取 CLI 参数路径)           │ │
│  │  open_file_default  (系统默认打开)                │ │
│  │  open_with_program  (自定义程序打开)               │ │
│  │  read_image_metadata(IPTC + EXIF 读取)           │ │
│  │  toggle_fullscreen  (全屏切换)                    │ │
│  │  read_settings      (读取 YAML 配置)              │ │
│  │  write_settings     (保存 YAML 配置)              │ │
│  │  browse_file        (文件浏览对话框)               │ │
│  └─────────────────────────────────────────────────┘ │
│                      ↓                               │
│  ┌─────────────────────────────────────────────────┐ │
│  │  图片处理流水线                                   │ │
│  │  读取文件 → 检查格式 → base64 编码 → Data URL    │ │
│  └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

## 数据流

### 图片加载流
```
用户点击按钮 / 命令行参数 / 拖放
    ↓
@click → invoke("load_images" / "load_image_files")
CLI   → invoke("get_startup_paths") → invoke("load_dropped_files")
拖放   → invoke("load_dropped_files")
    ↓ (Tauri IPC)
Rust 命令 → 原生对话框 (dialog plugin) 或直接文件读取
    ↓
用户选择文件夹 / 多选文件，或 CLI 路径直接解析
    ↓
Rust: 读取文件 → base64 编码 → 组装 ImageInfo[] (含 file_size)
    ↓ (返回 Result<LoadImagesResult>)
Vue: 接收数据 → buildTree() 构建树结构 → 存入 groups
    ↓
computed 计算:
  - allImages (扁平列表)
  - flatItems (分组树展平)
    ↓
template 渲染:
  - 分组关闭 → 扁平 <image-grid>
  - 分组开启 → 树节点 + 独立 <image-grid>
```

### i18n 翻译流
```
用户切换语言 (中文/EN)
    ↓
changeLanguage(lang) → locale.value = lang
    ↓
localStorage.setItem("locale", lang)
    ↓
vue-i18n 响应式更新所有 $t() 和 t() 调用
    ↓
模板 + computed 自动重新渲染
```

### 大图查看流
```
用户点击网格图片 → openViewer(img, groupId, gridImages, $event)
    ↓
捕获自然尺寸 → 设置导航列表 (分组内/全局)
    ↓
显示 viewer overlay → 适应窗口模式
    ↓
键盘 ← → / 按钮切换 → swapImage → 预加载邻图尺寸
    ↓
滚轮缩放 / 模式切换 → 更新 transform: scale()
    ↓
全屏 / 元信息 / 打开 / 拖拽
```
