# Vue 前端文档

> 所有视图、逻辑和样式集中在 `src/App.vue` 一个单文件组件中。国际化由 `src/i18n.ts` 配置。

## 国际化 (i18n)

### 配置

`src/i18n.ts` 使用 `vue-i18n` 配置双语支持：

- 翻译文件：[src/locales/zh.json](../src/locales/zh.json)、[src/locales/en.json](../src/locales/en.json)
- 语言选择优先级：`localStorage` → `navigator.language` → `zh`（回退）
- 语言持久化在 `localStorage` 的 `locale` key 中

### 使用

```typescript
const { t, locale } = useI18n();

// 模板中使用 $t()
{{ $t('toolbar.loadFolder') }}
{{ $t('group.imageCount', { count: images.length }) }}

// 脚本中使用 t()
t('error.noImagesInFolder')
t('settings.newPreset')
```

### 语言切换

工具栏提供 **中文** / **EN** 两个按钮，调用 `changeLanguage(lang)` 切换：

```typescript
function changeLanguage(lang: string) {
  locale.value = lang;
  localStorage.setItem("locale", lang);
  document.title = t('app.title');
}
```

## 类型定义

```typescript
interface ImageInfo {
  name: string;           // 文件名
  path: string;           // 完整路径
  data_url: string;       // Data URL（直接作为 img.src）
  relative_folder: string; // 相对目录路径（用于分组）
  file_size: number;      // 文件大小（字节）
}

interface TreeNode {
  name: string;           // 文件夹名称
  fullPath: string;       // 相对路径
  children: TreeNode[];   // 子文件夹
  images: ImageInfo[];    // 该文件夹下的图片
  expanded: boolean;      // 展开状态
  level: number;          // 层级深度
}

interface ImageGroup {
  id: number;
  rootPath: string;       // 根目录路径
  tree: TreeNode;         // 树结构
  images: ImageInfo[];    // 该分组全部图片
  groupExpanded: boolean; // 分组折叠状态
  isPlaceholder?: boolean;// 是否为骨架占位分组
}

interface FlatRenderItem {
  id: string;             // 唯一标识（用于 gridRatios）
  type: "folder" | "grid";
  node?: TreeNode;        // 文件夹节点
  images?: ImageInfo[];   // 网格图片列表
  depth: number;          // 缩进层级
  groupId: number;
}
```

## 响应式状态

| 变量              | 用途                               |
| ----------------- | ---------------------------------- |
| `groups`          | 分组列表（核心数据）               |
| `loading`         | 加载中状态（禁用按钮）             |
| `errorMsg`        | 错误信息横幅                       |
| `appendMode`      | 追加模式 / 重置模式                |
| `groupEnabled`    | 分组显示开关                        |
| `gridRatios`      | 各网格独立宽高比记录               |
| `naturalSizes`    | 图片自然尺寸缓存 (path → w/h)      |
| `locale`          | 当前语言（vue-i18n 响应式）         |
| `selectMode`      | 选择模式开关                        |
| `selectedPaths`   | 当前选中的图片路径集合             |

### 大图查看器状态

| 变量              | 用途                               |
| ----------------- | ---------------------------------- |
| `viewerOpen`      | 查看器是否打开                     |
| `viewerImage`     | 当前显示的图片                     |
| `viewerIndex`     | 当前图片在导航列表中的索引         |
| `viewerImages`    | 导航图片列表（分组内/全局）        |
| `viewerZoom`      | 缩放倍数                           |
| `viewerFitMode`   | 适应模式: fit-window / fit-short-edge / original |
| `showImageInfo`   | 是否显示图片信息悬浮栏             |
| `isFullscreen`    | 窗口是否全屏                       |
| `metadataVisible` | 元信息弹窗是否打开                 |
| `metadataFields`  | 元数据字段列表                     |

## 核心逻辑

### buildTree — 构建分组树

从扁平的 `ImageInfo[]` 按 `relative_folder` 构建树形结构：

```
输入: [{name:"a.jpg", relative_folder:""},
       {name:"b.jpg", relative_folder:"sub"},
       {name:"c.jpg", relative_folder:"sub/deep"}]

输出: TreeNode {
  name: "root",
  images: [a.jpg],
  children: [
    { name: "sub", fullPath: "sub",
      images: [],  // sub 目录下没有直接文件
      children: [
        { name: "deep", fullPath: "sub/deep",
          images: [c.jpg], children: [] }
      ]
    }
  ]
}
```

实现方式：使用 `Map<string, TreeNode>` 按 `fullPath` 索引节点，遍历每张图片，按路径分割创建或查找节点。

### flatItems — 展平树结构

将树节点递归展开为 `FlatRenderItem[]`，用于 Vue 的 `v-for` 列表渲染：

- 跳过已折叠的分组 (`group.groupExpanded === false`)
- 跳过已折叠的节点 (`node.expanded === false` 时其子节点和网格不加入)
- 文件夹节点 → `type: "folder"`
- 图片网格 → `type: "grid"`

### 网格独立比例

每个网格（分组内每个文件夹的图片集合）以自身第一张图的宽高比作为 `aspect-ratio`：

```typescript
const gridRatios = ref<Record<string, number>>({});

function onImageLoad(e: Event, gridId: string, index: number) {
  if (index === 0 && img.naturalWidth && img.naturalHeight) {
    if (!gridRatios.value[gridId]) {
      gridRatios.value[gridId] = img.naturalWidth / img.naturalHeight;
    }
  }
}
```

### 骨架占位加载

点击加载按钮后**立即**创建 `isPlaceholder: true` 的分组，显示 skeleton pulse 动画，扫描完成后原地替换为真实分组。不再显示旋转 spinner。

```typescript
function createPlaceholder(label: string): number  // 创建骨架分组
function replacePlaceholder(id: number, group: ImageGroup)  // 替换为真实数据
function removePlaceholder(id: number)  // 取消时移除
```

### 大图查看器

#### 打开流程

```typescript
function openViewer(img, groupId, gridImages, event)
```

1. 从 DOM 捕获图片自然尺寸 → 存入 `naturalSizes`
2. 设置导航列表：
   - 分组模式 + 有 `gridImages` → 使用该文件夹节点内的图片列表
   - 分组模式 + 只有 `groupId` → 使用整个分组的图片
   - 扁平模式 → 使用全局 `allImages`
3. 预加载前后图片的自然尺寸

#### 切换图片

```typescript
function swapImage(newImg)
```

- 立即更新 `viewerImage`
- 如自然尺寸尚未加载，创建离屏 `new Image()` 预加载
- 调用 `preloadAdjacentImages()` 预加载前后图片

#### 三种显示模式

| 模式            | CSS 效果                              | 说明                     |
| --------------- | ------------------------------------- | ------------------------ |
| `fit-window`    | `object-fit: contain` 全屏约束       | 完整显示，黑边留白       |
| `fit-short-edge`| `object-fit: cover` 全屏铺满         | 按短边适配，裁剪长边     |
| `original`      | `object-fit: none` 原始大小          | 1:1 像素，可滚轮缩放     |

滚轮缩放时自动切换到 `original` 模式，缩放值通过 `transform: scale(${zoom})` 叠加。

#### 实际缩放百分比

`viewerDisplayPercent` computed 计算含基础缩放 + zoom 叠加的百分比，初始适应窗口时显示正确比例。

## 视图组成

### 顶部工具栏

| 按钮                      | 功能                                  |
| ------------------------- | ------------------------------------- |
| 📂 加载文件夹             | 调用 `load_images`                    |
| 🖼️ 选择图片               | 调用 `load_image_files`               |
| 🔄 重置 / ➕ 追加          | 切换加载模式                          |
| 排序选择 / 方向           | 排序字段 + 升降序                     |
| 筛选输入框                | 正则筛选（文件/分组/路径模式）        |
| 中文 / EN                 | 切换界面语言                          |
| ⚙️                        | 打开设置面板                          |

### 启动流程 (onMounted)

1. 绑定键盘/拖放事件
2. 调用 `loadSettings()` 加载 YAML 配置
3. 调用 `document.title = t('app.title')` 设置窗口标题
4. 调用 `get_startup_paths()` 检查是否有 CLI 参数
5. 如有启动路径 → 调用 `handleDragDrop()`（复用拖放逻辑，已支持文件+文件夹混合）

### 内容展示区

四种状态：
1. **欢迎页** — 无分组时的空状态引导
2. **分组树状视图** — 分组卡片（可折叠+可删除）→ 子文件夹树节点（可折叠+可删除）→ 独立网格
3. **扁平网格视图** — 所有图片平铺
4. **骨架占位** — 加载中的 pulse 动画方块

### 底部控制栏

| 控件                    | 功能                             |
| ----------------------- | -------------------------------- |
| 📁 分组显示 开关         | 切换树状/扁平视图                |
| 🔽 展开全部 / 🔼 折叠全部 | 递归展开/折叠所有分组和树节点    |

### 大图查看器 (Overlay)

#### 图片区域
- 全窗口显示，`object-fit: contain/cover/none` 三种模式
- 键盘 ← → 导航，循环切换
- 滚轮缩放 (±0.1, 0.1x~10x)
- 可直接拖拽图片到其他程序
- ◀ ▶ 侧边导航按钮

#### 悬浮信息栏
- 格式：`[序号/总数] 文件名 — 宽×高 — 文件大小 — 缩放%`
- 可通过操作栏按钮隐藏/显示
- 毛玻璃效果，不干扰点击

#### 底部操作栏 (hover 显示)
| 按钮              | 功能                             |
| ----------------- | -------------------------------- |
| 100%              | 原始大小显示                     |
| 适应窗口          | contain 适配                    |
| 短边适应          | cover 适配，裁剪长边             |
| 隐藏/显示信息     | 切换悬浮信息栏                   |
| 📋 元信息         | 打开 IPTC + EXIF 弹窗            |
| ⛶ 全屏           | 切换窗口全屏                     |
| 📂 在资源管理器中显示 | 调用 `revealItemInDir`        |
| 🖥️ 默认方式打开   | 调用 Rust 命令系统默认打开       |
| ✕ 关闭            | 关闭大图查看器                   |

#### 元信息弹窗
- 按 IPTC / EXIF 分组展示所有字段
- 表格布局：字段名（左）+ 字段值（右）
- "📋 复制全部" 一键复制所有字段
- ESC / 点击背景 / ✕ 关闭

#### 关闭方式
- 点击 ✕ 关闭按钮 / ✕ 关闭 操作栏按钮
- 按 ESC 键
- **不支持**点击背景关闭（防止误触）
