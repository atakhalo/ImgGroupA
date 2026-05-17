<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

// ---- Types ----
interface ImageInfo {
  name: string;
  path: string;
  data_url: string;
  relative_folder: string;
  file_size: number;
  modified_date: number;
}

interface LoadImagesResult {
  folder_path: string;
  images: ImageInfo[];
}

interface TreeNode {
  name: string;
  fullPath: string;
  children: TreeNode[];
  images: ImageInfo[];
  expanded: boolean;
  level: number;
}

interface ImageGroup {
  id: number;
  rootPath: string;
  tree: TreeNode;
  images: ImageInfo[];
  groupExpanded: boolean;
  isPlaceholder?: boolean;
}

interface FlatRenderItem {
  id: string;
  type: "folder" | "grid";
  node?: TreeNode;
  images?: ImageInfo[];
  depth: number;
  groupId: number;
}

interface GridSettings {
  border_radius: number;
  gap: number;
  min_width: number;
  background_color: string;
  root_title_color: string;
  root_title_bg: string;
  child_title_color: string;
  child_title_bg: string;
  group_bg: string;
}


interface ExternalProgram {
  name: string;
  path: string;
}

interface FilterPreset {
  name: string;
  pattern: string;
  mode: string;
}

interface AppSettings {
  grid: GridSettings;
  filter_presets: FilterPreset[];
  external_programs: ExternalProgram[];
}

// ---- State ----
const groups = ref<ImageGroup[]>([]);
const loading = ref(false);
const errorMsg = ref("");
const appendMode = ref(true);
const groupEnabled = ref(false);
const showNodeTitles = ref(true);
const selectMode = ref(false);
const selectedPaths = ref<Set<string>>(new Set());
const showNewGroupDialog = ref(false);
const newGroupName = ref("");
const showDeleteConfirm = ref(false);
const deleting = ref(false);
const gridRatios = ref<Record<string, number>>({});
const naturalSizes = ref<Record<string, { w: number; h: number }>>({});

function toggleSelectMode() {
  selectMode.value = !selectMode.value;
  selectedPaths.value = new Set();
  showNewGroupDialog.value = false;
}

// ---- i18n ----
const { t, locale } = useI18n();

function changeLanguage(lang: string) {
  locale.value = lang;
  localStorage.setItem("locale", lang);
  document.title = t('app.title');
}

function toggleImageSelection(path: string) {
  const s = new Set(selectedPaths.value);
  if (s.has(path)) {
    s.delete(path);
  } else {
    s.add(path);
  }
  selectedPaths.value = s;
}

function openNewGroupDialog() {
  if (selectedPaths.value.size === 0) return;
  newGroupName.value = nextTempGroupName();
  showNewGroupDialog.value = true;
}

function createGroupFromSelection() {
  const paths = [...selectedPaths.value];
  if (paths.length === 0) return;

  const selectedImgs = allImages.value.filter((img) => paths.includes(img.path));
  if (selectedImgs.length === 0) return;

  const groupName = newGroupName.value.trim() || nextTempGroupName();
  const tree = buildTree(selectedImgs, groupName);
  const group: ImageGroup = {
    id: ++groupIdCounter,
    rootPath: groupName,
    tree,
    images: selectedImgs,
    groupExpanded: true,
  };

  groups.value.push(group);
  selectedPaths.value = new Set();
  showNewGroupDialog.value = false;
}

function onGridItemClick(img: ImageInfo, groupId: number | undefined, gridImages: ImageInfo[] | undefined, event: MouseEvent) {
  if (selectMode.value) {
    toggleImageSelection(img.path);
  } else {
    openViewer(img, groupId, gridImages, event);
  }
}

// ============ 对比模式 ============
const compareOpen = ref(false);
const compareImages = ref<[ImageInfo, ImageInfo] | null>(null);
const compareZoom = ref(1);
const compareFitMode = ref<"fit-window" | "fit-short-edge" | "original">("fit-window");
const dividerPos = ref(50);
const draggingDivider = ref(false);
const comparePanMode = ref(false);
const comparePanX = ref(0);
const comparePanY = ref(0);
const comparePanDragging = ref(false);
let comparePanStart = { x: 0, y: 0, px: 0, py: 0 };

function onComparePanDown(e: MouseEvent) {
  if (!comparePanMode.value) return;
  if ((e.target as HTMLElement).closest(".compare-divider")) return;
  e.preventDefault();
  comparePanDragging.value = true;
  comparePanStart = { x: e.clientX, y: e.clientY, px: comparePanX.value, py: comparePanY.value };
  document.addEventListener("mousemove", onComparePanMove);
  document.addEventListener("mouseup", onComparePanUp);
}
function onComparePanMove(e: MouseEvent) {
  if (!comparePanDragging.value) return;
  comparePanX.value = comparePanStart.px + (e.clientX - comparePanStart.x);
  comparePanY.value = comparePanStart.py + (e.clientY - comparePanStart.y);
}
function onComparePanUp() {
  comparePanDragging.value = false;
  document.removeEventListener("mousemove", onComparePanMove);
  document.removeEventListener("mouseup", onComparePanUp);
}
function toggleComparePan() {
  comparePanMode.value = !comparePanMode.value;
  comparePanX.value = 0;
  comparePanY.value = 0;
}

function onCompareDragStart(e: DragEvent, idx: number) {
  if (comparePanMode.value) { e.preventDefault(); return; }
  if (!compareImages.value) return;
  setDragData(e.dataTransfer, compareImages.value[idx], e.target as HTMLElement);
}

function openCompare() {
  const paths = [...selectedPaths.value];
  if (paths.length !== 2) return;
  const imgs = allImages.value.filter((img) => paths.includes(img.path));
  if (imgs.length !== 2) return;
  compareImages.value = [imgs[0], imgs[1]];
  [imgs[0], imgs[1]].forEach((img) => {
    if (!naturalSizes.value[img.path]) preloadSize(img);
  });
  compareOpen.value = true;
  compareZoom.value = 1;
  compareFitMode.value = "fit-window";
  dividerPos.value = 50;
}

function preloadSize(img: ImageInfo) {
  const off = new Image();
  off.onload = () => { naturalSizes.value[img.path] = { w: off.naturalWidth, h: off.naturalHeight }; };
  off.src = img.data_url;
}

function closeCompare() {
  compareOpen.value = false;
  compareImages.value = null;
}

function onCompareWheel(e: WheelEvent) {
  e.preventDefault();
  const delta = e.deltaY > 0 ? -0.1 : 0.1;
  compareZoom.value = Math.max(0.1, Math.min(10, compareZoom.value + delta));
  compareFitMode.value = "original";
}

function onCompareMousedown(_e: MouseEvent) {
  draggingDivider.value = true;
  document.addEventListener("mousemove", onCompareMousemove);
  document.addEventListener("mouseup", onCompareMouseup);
}

const compareViewportRef = ref<HTMLElement | null>(null);

function onCompareMousemove(e: MouseEvent) {
  if (!draggingDivider.value) return;
  const rect = compareViewportRef.value?.getBoundingClientRect();
  if (!rect) return;
  dividerPos.value = Math.max(5, Math.min(95, ((e.clientX - rect.left) / rect.width) * 100));
}

function onCompareMouseup() {
  draggingDivider.value = false;
  document.removeEventListener("mousemove", onCompareMousemove);
  document.removeEventListener("mouseup", onCompareMouseup);
}

const gridScale = ref(1);
const contentRef = ref<HTMLElement | null>(null);
let groupIdCounter = 0;
let tempGroupCounter = 0;

function nextTempGroupName(): string {
  tempGroupCounter++;
  return `${t('tempGroup')}-${tempGroupCounter}`;
}

function onGridWheel(e: WheelEvent) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  const delta = e.deltaY > 0 ? -0.08 : 0.08;
  const minScale = 10 / appSettings.value.grid.min_width;
  gridScale.value = Math.max(minScale, Math.min(3, gridScale.value + delta));
}

// ---- Drag & Drop ----
const dragOver = ref(false);
const internalDrag = ref(false);

async function handleDragDrop(paths: string[]) {
  if (paths.length === 0) return;
  dragOver.value = false;
  errorMsg.value = "";
  loading.value = true;

  const placeholderId = createPlaceholder(t('loading.droppedFiles'));

  try {
    const result = await invoke<LoadImagesResult>("load_dropped_files", { paths });

    if (result.images.length === 0) {
      removePlaceholder(placeholderId);
      errorMsg.value = t('error.noImagesInDrop');
      return;
    }

    const groupName = result.folder_path === "__temp__"
      ? nextTempGroupName()
      : result.folder_path;
    const tree = buildTree(result.images, groupName);
    const group: ImageGroup = {
      id: placeholderId,
      rootPath: groupName,
      tree,
      images: result.images,
      groupExpanded: true,
    };

    replacePlaceholder(placeholderId, group);
  } catch (e) {
    removePlaceholder(placeholderId);
    errorMsg.value = String(e);
  } finally {
    loading.value = false;
  }
}

// ---- Sorting ----
const sortBy = ref<"name" | "date" | "size">("name");
const sortAsc = ref(true);

function sortImages(imgs: ImageInfo[]) {
  imgs.sort((a, b) => {
    let cmp = 0;
    switch (sortBy.value) {
      case "name": cmp = a.name.localeCompare(b.name); break;
      case "date": cmp = a.modified_date - b.modified_date; break;
      case "size": cmp = a.file_size - b.file_size; break;
    }
    return sortAsc.value ? cmp : -cmp;
  });
}

// ---- Filtering ----
const filterText = ref("");
const filterMode = ref<"file" | "group" | "path">("file");
const filterRegexError = ref(false);

function tryBuildRegex(): RegExp | null {
  if (!filterText.value) return null;
  try {
    const r = new RegExp(filterText.value, "i");
    filterRegexError.value = false;
    return r;
  } catch {
    filterRegexError.value = true;
    return null;
  }
}

// ---- Computed ----
const allImages = computed(() => groups.value.flatMap((g) => g.images));
const imageCount = computed(() => allImages.value.length);

// 仅按分组名筛选（不重建树，保留展开状态）
const displayedGroups = computed(() => {
  const regex = tryBuildRegex();
  if (!regex || filterMode.value !== "group") return groups.value;
  return groups.value.filter((g) => {
    if (g.isPlaceholder) return true;
    const groupName = g.rootPath.replace(/\\/g, "/").split("/").pop() || g.rootPath;
    if (regex.test(groupName)) return true;
    return hasAnyMatchingNode(g.tree, regex);
  });
});

// 排序+筛选后的图片（用于扁平视图）
const displayImages = computed(() => {
  let imgs = [...allImages.value];
  sortImages(imgs);
  const regex = tryBuildRegex();
  if (regex) {
    const field = filterMode.value === "path" ? "path" : "name";
    imgs = imgs.filter((img) => regex.test(img[field]));
  }
  return imgs;
});

const flatItems = computed(() => {
  const regex = tryBuildRegex();
  const items: FlatRenderItem[] = [];
  for (const group of displayedGroups.value) {
    if (!group.groupExpanded) continue;
    flattenGroup(group, items, regex);
  }
  return items;
});

// ---- Tree helpers ----
function prepareGridImages(nodeImages: ImageInfo[], regex: RegExp | null): ImageInfo[] {
  let imgs = [...nodeImages];
  sortImages(imgs);
  if (regex && filterMode.value !== "group") {
    const field = filterMode.value === "path" ? "path" : "name";
    imgs = imgs.filter((img) => regex.test(img[field]));
  }
  return imgs;
}

function hasAnyMatchingNode(node: TreeNode, regex: RegExp): boolean {
  if (regex.test(node.name)) return true;
  for (const child of node.children) {
    if (hasAnyMatchingNode(child, regex)) return true;
  }
  return false;
}

function flattenGroup(group: ImageGroup, items: FlatRenderItem[], regex: RegExp | null): void {
  const root = group.tree;
  const groupRegex = filterMode.value === "group" ? regex : null;
  for (const child of root.children) {
    if (groupRegex && !hasAnyMatchingNode(child, groupRegex)) continue;
    flattenNode(child, items, true, group.id, regex, groupRegex);
  }
  if (root.images.length > 0) {
    items.push({
      id: `${group.id}-root-grid`,
      type: "grid",
      images: prepareGridImages(root.images, regex),
      depth: 0,
      groupId: group.id,
    });
  }
}

function flattenNode(
  node: TreeNode,
  items: FlatRenderItem[],
  parentVisible: boolean,
  groupId: number,
  regex: RegExp | null,
  groupRegex: RegExp | null = null
): void {
  if (!parentVisible) return;

  items.push({
    id: `${groupId}-${node.fullPath || "root"}`,
    type: "folder",
    node,
    depth: node.level,
    groupId,
  });

  if (node.expanded) {
    for (const child of node.children) {
      if (groupRegex && !hasAnyMatchingNode(child, groupRegex)) continue;
      flattenNode(child, items, true, groupId, regex, groupRegex);
    }
    if (node.images.length > 0) {
      items.push({
        id: `${groupId}-${node.fullPath || "root"}-grid`,
        type: "grid",
        images: prepareGridImages(node.images, regex),
        depth: node.level + 1,
        groupId,
      });
    }
  }
}

function buildTree(images: ImageInfo[], rootPath: string): TreeNode {
  const rootName =
    rootPath.split("\\").pop() ||
    rootPath.split("/").pop() ||
    rootPath;
  const root: TreeNode = {
    name: rootName,
    fullPath: "",
    children: [],
    images: [],
    expanded: true,
    level: 0,
  };

  const folderMap = new Map<string, TreeNode>();

  for (const img of images) {
    if (!img.relative_folder) {
      root.images.push(img);
    } else {
      const parts = img.relative_folder.replace(/\\/g, "/").split("/");
      let currentPath = "";

      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        const parentPath = currentPath;
        currentPath = currentPath ? `${currentPath}/${part}` : part;

        if (!folderMap.has(currentPath)) {
          const node: TreeNode = {
            name: part,
            fullPath: currentPath,
            children: [],
            images: [],
            expanded: true,
            level: i + 1,
          };
          folderMap.set(currentPath, node);

          if (i === 0) {
            root.children.push(node);
          } else {
            const parent = folderMap.get(parentPath);
            if (parent) {
              parent.children.push(node);
            }
          }
        }
      }

      const leafNode = folderMap.get(currentPath);
      if (leafNode) {
        leafNode.images.push(img);
      }
    }
  }

  return root;
}

function countImagesInNode(node: TreeNode): number {
  let count = node.images.length;
  for (const child of node.children) {
    count += countImagesInNode(child);
  }
  return count;
}

function toggleExpand(node: TreeNode) {
  node.expanded = !node.expanded;
}

function toggleGroup(group: ImageGroup) {
  group.groupExpanded = !group.groupExpanded;
}

function removeGroup(groupId: number) {
  groups.value = groups.value.filter((g) => g.id !== groupId);
}

const allCollapsed = computed(() =>
  groups.value.length > 0 && groups.value.every((g) => !g.groupExpanded)
);

function toggleAllGroups() {
  const newState = allCollapsed.value;
  groups.value.forEach((g) => {
    g.groupExpanded = newState;
    toggleNodeRecursive(g.tree, newState);
  });
}

function toggleNodeRecursive(node: TreeNode, expanded: boolean) {
  node.expanded = expanded;
  for (const child of node.children) {
    toggleNodeRecursive(child, expanded);
  }
}

function removeTreeNode(groupId: number, target: TreeNode) {
  const group = groups.value.find((g) => g.id === groupId);
  if (!group) return;

  function removeFromParent(children: TreeNode[]): boolean {
    for (let i = 0; i < children.length; i++) {
      if (children[i] === target) {
        children.splice(i, 1);
        return true;
      }
      if (removeFromParent(children[i].children)) return true;
    }
    return false;
  }

  removeFromParent(group.tree.children);

  const prefix = target.fullPath ? target.fullPath + '/' : '';
  group.images = group.images.filter(
    (img) =>
      img.relative_folder !== target.fullPath &&
      !img.relative_folder.startsWith(prefix)
  );
}

// ---- Folder loading ----
async function handleLoadImages() {
  errorMsg.value = "";

  const placeholderId = createPlaceholder(t('loading.folder'));
  loading.value = true;

  try {
    const result = await invoke<LoadImagesResult>("load_images");

    if (result.images.length === 0) {
      removePlaceholder(placeholderId);
      errorMsg.value = t('error.noImagesInFolder');
      return;
    }

    const tree = buildTree(result.images, result.folder_path);
    const group: ImageGroup = {
      id: placeholderId,
      rootPath: result.folder_path,
      tree,
      images: result.images,
      groupExpanded: true,
    };

    replacePlaceholder(placeholderId, group);
  } catch (e) {
    removePlaceholder(placeholderId);
    const msg = String(e);
    if (!msg.includes("CANCELLED")) {
      errorMsg.value = msg || t('error.loadFailed');
    }
  } finally {
    loading.value = false;
  }
}

async function handleLoadFiles() {
  errorMsg.value = "";

  const placeholderId = createPlaceholder(t('loading.images'));
  loading.value = true;

  try {
    const result = await invoke<LoadImagesResult>("load_image_files");

    if (result.images.length === 0) {
      removePlaceholder(placeholderId);
      errorMsg.value = t('error.noImagesInFiles');
      return;
    }

    const groupName = nextTempGroupName();
    const tree = buildTree(result.images, groupName);
    const group: ImageGroup = {
      id: placeholderId,
      rootPath: groupName,
      tree,
      images: result.images,
      groupExpanded: true,
    };

    replacePlaceholder(placeholderId, group);
  } catch (e) {
    removePlaceholder(placeholderId);
    const msg = String(e);
    if (!msg.includes("CANCELLED")) {
      errorMsg.value = msg || t('error.loadFailed');
    }
  } finally {
    loading.value = false;
  }
}

function createPlaceholder(label: string): number {
  const id = ++groupIdCounter;
  const emptyNode: TreeNode = {
    name: "",
    fullPath: "",
    children: [],
    images: [],
    expanded: true,
    level: 0,
  };
  const placeholder: ImageGroup = {
    id,
    rootPath: label,
    tree: emptyNode,
    images: [],
    groupExpanded: true,
    isPlaceholder: true,
  };

  if (appendMode.value) {
    groups.value.push(placeholder);
  } else {
    groups.value = [placeholder];
    gridRatios.value = {};
  }
  return id;
}

function removePlaceholder(id: number) {
  groups.value = groups.value.filter((g) => g.id !== id);
}

function replacePlaceholder(id: number, group: ImageGroup) {
  const idx = groups.value.findIndex((g) => g.id === id);
  if (idx !== -1) {
    groups.value[idx] = group;
  } else {
    if (appendMode.value) {
      groups.value.push(group);
    } else {
      groups.value = [group];
    }
  }
}

function onImageLoad(e: Event, gridId: string, index: number) {
  const img = e.target as HTMLImageElement;
  const { naturalWidth, naturalHeight } = img;
  if (index === 0 && naturalWidth && naturalHeight) {
    if (!gridRatios.value[gridId]) {
      gridRatios.value[gridId] = naturalWidth / naturalHeight;
    }
  }
}

// ============ 图片大图查看器 ============

const viewerOpen = ref(false);
const viewerImage = ref<ImageInfo | null>(null);
const viewerIndex = ref(0);
const viewerImages = ref<ImageInfo[]>([]);
const viewerZoom = ref(1);
const viewerFitMode = ref<"fit-window" | "fit-short-edge" | "original">("fit-window");
const showImageInfo = ref(true);
const openWithMenuOpen = ref(false);
const viewerPanMode = ref(false);
const viewerPanX = ref(0);
const viewerPanY = ref(0);
const viewerPanDragging = ref(false);
let viewerPanStart = { x: 0, y: 0, px: 0, py: 0 };

function onViewerPanDown(e: MouseEvent) {
  if (!viewerPanMode.value) return;
  e.preventDefault();
  viewerPanDragging.value = true;
  viewerPanStart = { x: e.clientX, y: e.clientY, px: viewerPanX.value, py: viewerPanY.value };
  document.addEventListener("mousemove", onViewerPanMove);
  document.addEventListener("mouseup", onViewerPanUp);
}
function onViewerPanMove(e: MouseEvent) {
  if (!viewerPanDragging.value) return;
  viewerPanX.value = viewerPanStart.px + (e.clientX - viewerPanStart.x);
  viewerPanY.value = viewerPanStart.py + (e.clientY - viewerPanStart.y);
}
function onViewerPanUp() {
  viewerPanDragging.value = false;
  document.removeEventListener("mousemove", onViewerPanMove);
  document.removeEventListener("mouseup", onViewerPanUp);
}
function toggleViewerPan() {
  viewerPanMode.value = !viewerPanMode.value;
  viewerPanX.value = 0;
  viewerPanY.value = 0;
}

function pathToFileUri(path: string): string {
  // Windows: E:\foo\中文.png → file:///E:/foo/%E4%B8%AD%E6%96%87.png
  const n = path.replace(/\\/g, "/");
  const parts = n.split("/");
  return "file:///" + parts[0] + "/" + parts.slice(1).map(encodeURIComponent).join("/");
}

function dataUrlToFile(dataUrl: string, name: string): File {
  const [meta, b64] = dataUrl.split(",");
  const mime = meta.match(/:(.*?);/)?.[1] || "image/png";
  const raw = atob(b64);
  const arr = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
  return new File([arr], name, { type: mime });
}

function setDragData(dt: DataTransfer | null, img: ImageInfo, el?: HTMLElement) {
  if (!dt) return;
  dt.clearData(); // 清除浏览器默认的 text/html 等内置字段
  dt.effectAllowed = "copy";
  dt.setData("text/plain", '123/123.jpg');
  dt.setData("text/uri-list", '123/123.jpg');
//   dt.setData("text/plain", img.path);
  dt.setData("text/uri-list", pathToFileUri(img.path));
  dt.items.add(dataUrlToFile(img.data_url, img.name));
  if (el) dt.setDragImage(el, 0, 0);
  console.log('setDragData');
  console.log(dt.getData('text/plain'));
  console.log(dt.getData('text/uri-list'));
  console.log(dt.getData('Files'));
}

function onViewerDragStart(e: DragEvent) {
  if (viewerPanMode.value) { e.preventDefault(); return; }
  if (!viewerImage.value) return;
  setDragData(e.dataTransfer, viewerImage.value, e.target as HTMLElement);
}

const viewerDisplayPercent = computed(() => {
  const img = viewerImage.value;
  if (!img) return 100;
  const size = naturalSizes.value[img.path];
  if (!size) return Math.round(viewerZoom.value * 100);
  if (viewerFitMode.value === "original") {
    return Math.round(viewerZoom.value * 100);
  }
  const scaleX = window.innerWidth / size.w;
  const scaleY = window.innerHeight / size.h;
  const base = viewerFitMode.value === "fit-short-edge" ? Math.max(scaleX, scaleY) : Math.min(scaleX, scaleY);
  return Math.round(base * viewerZoom.value * 100);
});

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const viewerInfoText = computed(() => {
  const img = viewerImage.value;
  if (!img) return "";
  const size = naturalSizes.value[img.path];
  const dims = size ? `${size.w} × ${size.h}` : "? × ?";
  const idx = viewerImages.value.length > 0 ? `${viewerIndex.value + 1}/${viewerImages.value.length}` : "";
  const fsize = formatFileSize(img.file_size);
  return `${idx} ${img.name} — ${dims} — ${fsize} — ${viewerDisplayPercent.value}%`.trim();
});

// 打开方式列表
const openWithList = computed(() => {
  const list: { name: string; action: () => void }[] = [];
  list.push({ name: t('viewer.openInExplorer'), action: openInExplorer });
  list.push({ name: t('viewer.openWithDefault'), action: openWithDefault });
  for (const prog of appSettings.value.external_programs) {
    list.push({
      name: `🔧 ${prog.name}`,
      action: () => openWithCustom(prog.path),
    });
  }
  return list;
});

function openViewer(
  img: ImageInfo,
  groupId: number | undefined,
  gridImages: ImageInfo[] | undefined,
  event: MouseEvent
) {
  const gridItem = event.currentTarget as HTMLElement;
  const imgEl = gridItem.querySelector("img");
  if (imgEl?.naturalWidth && imgEl?.naturalHeight) {
    naturalSizes.value[img.path] = { w: imgEl.naturalWidth, h: imgEl.naturalHeight };
  }
  viewerImage.value = img;
  viewerOpen.value = true;
  openWithMenuOpen.value = false;
  viewerFitMode.value = "fit-window";
  viewerZoom.value = 1;

  if (groupEnabled.value && gridImages) {
    viewerImages.value = gridImages;
  } else if (groupEnabled.value && groupId !== undefined) {
    const group = displayedGroups.value.find((g) => g.id === groupId);
    viewerImages.value = group ? group.images : displayImages.value;
  } else {
    viewerImages.value = displayImages.value;
  }
  viewerIndex.value = viewerImages.value.findIndex((i) => i.path === img.path);
  preloadAdjacentImages();
}

function closeViewer() {
  viewerOpen.value = false;
  viewerImage.value = null;
  openWithMenuOpen.value = false;
}

function swapImage(newImg: ImageInfo) {
  viewerImage.value = newImg;
  if (!naturalSizes.value[newImg.path]) {
    const offscreen = new Image();
    offscreen.onload = () => {
      naturalSizes.value[newImg.path] = { w: offscreen.naturalWidth, h: offscreen.naturalHeight };
    };
    offscreen.src = newImg.data_url;
  }
  preloadAdjacentImages();
}

function prevImage() {
  if (viewerImages.value.length === 0) return;
  viewerIndex.value = (viewerIndex.value - 1 + viewerImages.value.length) % viewerImages.value.length;
  swapImage(viewerImages.value[viewerIndex.value]);
}

function nextImage() {
  if (viewerImages.value.length === 0) return;
  viewerIndex.value = (viewerIndex.value + 1) % viewerImages.value.length;
  swapImage(viewerImages.value[viewerIndex.value]);
}

function preloadAdjacentImages() {
  const len = viewerImages.value.length;
  if (len <= 1) return;
  const prev = viewerImages.value[(viewerIndex.value - 1 + len) % len];
  const next = viewerImages.value[(viewerIndex.value + 1) % len];
  [prev, next].forEach((img) => {
    if (!naturalSizes.value[img.path]) {
      const off = new Image();
      off.onload = () => {
        naturalSizes.value[img.path] = { w: off.naturalWidth, h: off.naturalHeight };
      };
      off.src = img.data_url;
    }
  });
}

function onViewerImageLoad(e: Event) {
  const img = e.target as HTMLImageElement;
  if (!viewerImage.value) return;
  if (img.naturalWidth && img.naturalHeight) {
    naturalSizes.value[viewerImage.value.path] = { w: img.naturalWidth, h: img.naturalHeight };
  }
}

function setFitMode(mode: "fit-window" | "fit-short-edge" | "original") {
  viewerFitMode.value = mode;
  viewerZoom.value = 1;
}

function onViewerWheel(e: WheelEvent) {
  e.preventDefault();
  const delta = e.deltaY > 0 ? -0.1 : 0.1;
  viewerZoom.value = Math.max(0.1, Math.min(10, viewerZoom.value + delta));
  viewerFitMode.value = "original";
}

// ---- Delete ----
async function deleteImages(paths: string[]) {
  if (paths.length === 0) return;

  deleting.value = true;
  try {
    await invoke("delete_files", { paths });

    const pathSet = new Set(paths);

    // Remove deleted images from all groups
    for (const group of groups.value) {
      group.images = group.images.filter(img => !pathSet.has(img.path));
      if (group.images.length > 0) {
        group.tree = buildTree(group.images, group.rootPath);
      }
    }

    // Remove groups that are now empty (skip placeholders)
    groups.value = groups.value.filter(g => g.isPlaceholder || g.images.length > 0);

    // Clear selection
    selectedPaths.value = new Set();

    // Update viewer if open: remove deleted images from navigation list
    if (viewerOpen.value) {
      const remaining = viewerImages.value.filter(img => !pathSet.has(img.path));
      if (remaining.length === 0) {
        closeViewer();
      } else {
        viewerImages.value = remaining;
        if (viewerImage.value && pathSet.has(viewerImage.value.path)) {
          // Current image was deleted — switch to first remaining
          viewerIndex.value = 0;
          viewerImage.value = remaining[0];
          if (!naturalSizes.value[remaining[0].path]) {
            preloadSize(remaining[0]);
          }
        } else {
          // Recalculate index of current image
          viewerIndex.value = remaining.findIndex(img => img.path === viewerImage.value!.path);
          if (viewerIndex.value === -1) viewerIndex.value = 0;
        }
      }
    }

    showDeleteConfirm.value = false;
  } catch (e) {
    errorMsg.value = t('delete.error') + ": " + String(e);
  } finally {
    deleting.value = false;
  }
}

function openDeleteConfirm() {
  showDeleteConfirm.value = true;
}

async function deleteViewerImage() {
  if (!viewerImage.value) return;
  await deleteImages([viewerImage.value.path]);
}

function onViewerKeydown(e: KeyboardEvent) {
  if (compareOpen.value && e.key === "Escape") {
    closeCompare();
    return;
  }
  if (!viewerOpen.value) {
    if (e.key === "Escape") {
      getCurrentWebviewWindow().close();
    }
    return;
  }
  if (e.key === "Escape") {
    closeViewer();
  } else if (e.key === "ArrowLeft") {
    prevImage();
  } else if (e.key === "ArrowRight") {
    nextImage();
  } else if (e.key === "Delete") {
    deleteViewerImage();
  }
}

async function openInExplorer() {
  if (!viewerImage.value) return;
  try {
    await revealItemInDir(viewerImage.value.path);
  } catch (_) { /* ignore */ }
  openWithMenuOpen.value = false;
}

async function openWithDefault() {
  if (!viewerImage.value) return;
  try {
    await invoke("open_file_default", { path: viewerImage.value.path });
  } catch (_) { /* ignore */ }
  openWithMenuOpen.value = false;
}

async function openWithCustom(programPath: string) {
  if (!viewerImage.value) return;
  try {
    await invoke("open_with_program", { programPath, filePath: viewerImage.value.path });
  } catch (_) { /* ignore */ }
  openWithMenuOpen.value = false;
}

// ---- Fullscreen ----
const isFullscreen = ref(false);

async function toggleFullscreen() {
  try {
    isFullscreen.value = await invoke<boolean>("toggle_fullscreen");
  } catch (_) { /* ignore */ }
}

// ---- Metadata ----
interface MetadataField {
  name: string;
  value: string;
  source: string;
}

const metadataVisible = ref(false);
const metadataLoading = ref(false);
const metadataFields = ref<MetadataField[]>([]);
const metadataError = ref("");

async function showMetadata() {
  if (!viewerImage.value) return;
  metadataVisible.value = true;
  metadataLoading.value = true;
  metadataFields.value = [];
  metadataError.value = "";
  try {
    const result = await invoke<{ fields: MetadataField[] }>("read_image_metadata", {
      path: viewerImage.value.path,
    });
    metadataFields.value = result.fields;
  } catch (e) {
    metadataError.value = String(e);
  } finally {
    metadataLoading.value = false;
  }
}

async function copyMetadata() {
  const text = metadataFields.value
    .map((f) => `[${f.source}] ${f.name}: ${f.value}`)
    .join("\n");
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch (_) { /* ignore */ }
}

function groupMetadata() {
  const groups: { source: string; items: MetadataField[] }[] = [];
  const map = new Map<string, MetadataField[]>();
  for (const f of metadataFields.value) {
    if (!map.has(f.source)) map.set(f.source, []);
    map.get(f.source)!.push(f);
  }
  for (const [source, items] of map) {
    groups.push({ source, items });
  }
  return groups;
}

// ---- Settings ----
const settingsOpen = ref(false);
const settingsTab = ref<"grid" | "presets" | "programs">("grid");
const appSettings = ref<AppSettings>({
  grid: {
    border_radius: 8, gap: 12, min_width: 200, background_color: "#0f1a30",
    root_title_color: "#cccccc", root_title_bg: "#1a2a4a",
    child_title_color: "#cccccc", child_title_bg: "#141e33",
    group_bg: "#141e33",
  },
  filter_presets: [],
  external_programs: [],
});

async function loadSettings() {
  try {
    const s = await invoke<AppSettings>("read_settings");
    appSettings.value = s;
  } catch (_) { /* ignore */ }
}

async function saveSettings() {
  try {
    await invoke("write_settings", { settings: appSettings.value });
  } catch (e) {
    errorMsg.value = t('error.saveFailed') + ": " + String(e);
  }
}

function resetStyleSettings() {
  appSettings.value.grid = {
    border_radius: 8, gap: 12, min_width: 200, background_color: "#0f1a30",
    root_title_color: "#cccccc", root_title_bg: "#1a2a4a",
    child_title_color: "#cccccc", child_title_bg: "#141e33",
    group_bg: "#141e33",
  };
}

async function browseExternalProgram() {
  try {
    const path = await invoke<string>("browse_file");
    if (path) {
      const name = path.split("\\").pop()?.split("/").pop()?.replace(/\.[^.]+$/, "") || path;
      appSettings.value = {
        ...appSettings.value,
        external_programs: [
          ...appSettings.value.external_programs,
          { name, path },
        ],
      };
    }
  } catch (_) { /* ignore */ }
}

function removeExternalProgram(idx: number) {
  appSettings.value.external_programs.splice(idx, 1);
}

function addFilterPreset() {
  appSettings.value = {
    ...appSettings.value,
    filter_presets: [
      ...appSettings.value.filter_presets,
      { name: t('settings.newPreset'), pattern: "", mode: "file" as const },
    ],
  };
}

function removeFilterPreset(idx: number) {
  appSettings.value.filter_presets.splice(idx, 1);
}

function applyPreset(preset: FilterPreset) {
  filterText.value = preset.pattern;
  filterMode.value = preset.mode as "file" | "group" | "path";
  settingsOpen.value = false;
}

function splitRootPath(rootPath: string): { parentPath: string; folderName: string } {
  const n = rootPath.replace(/\\/g, "/");
  const i = n.lastIndexOf("/");
  if (i === -1) return { parentPath: "", folderName: rootPath };
  return { parentPath: n.substring(0, i + 1), folderName: n.substring(i + 1) };
}

const gridCssVars = computed(() => ({
  "--cell-radius": appSettings.value.grid.border_radius + "px",
  "--cell-gap": appSettings.value.grid.gap + "px",
  "--cell-min-width": Math.round(appSettings.value.grid.min_width * gridScale.value) + "px",
  "--cell-bg": appSettings.value.grid.background_color,
  "--root-title-color": appSettings.value.grid.root_title_color,
  "--root-title-bg": appSettings.value.grid.root_title_bg,
  "--child-title-color": appSettings.value.grid.child_title_color,
  "--child-title-bg": appSettings.value.grid.child_title_bg,
  "--group-bg": appSettings.value.grid.group_bg,
}));

let unlistenDragDrop: (() => void) | null = null;

function onInternalDragStart() { internalDrag.value = true; }
function onInternalDragEnd() { internalDrag.value = false; }

onMounted(async () => {
  window.addEventListener("keydown", onViewerKeydown);
  loadSettings();
  document.title = t('app.title');

  // Load from CLI args if provided (supports files, folders, or mixed)
  try {
    const startupPaths = await invoke<string[]>("get_startup_paths");
    if (startupPaths && startupPaths.length > 0) {
      await handleDragDrop(startupPaths);
    }
  } catch (_) { /* ignore if command not available */ }

  // 跟踪应用内原生拖拽，避免触发外部拖入覆盖层
  document.addEventListener("dragstart", onInternalDragStart);
  document.addEventListener("dragend", onInternalDragEnd);

  // Ctrl+滚轮缩放（手动注册绕过 Vue 事件系统开销）
  contentRef.value?.addEventListener("wheel", onGridWheel);

  // 设置拖放监听
  try {
    unlistenDragDrop = await getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (internalDrag.value) return;
      if (event.payload.type === "over") {
        dragOver.value = true;
      } else if (event.payload.type === "leave") {
        dragOver.value = false;
      } else if (event.payload.type === "drop") {
        handleDragDrop(event.payload.paths);
      }
    });
  } catch (_) {
    // 非 Tauri 环境忽略
  }
});

onUnmounted(() => {
  window.removeEventListener("keydown", onViewerKeydown);
  if (unlistenDragDrop) unlistenDragDrop();
  contentRef.value?.removeEventListener("wheel", onGridWheel);
  document.removeEventListener("dragstart", onInternalDragStart);
  document.removeEventListener("dragend", onInternalDragEnd);
});
</script>

<template>
  <div class="app">
    <!-- ===== 顶部工具栏 ===== -->
    <header class="toolbar">
      <div class="toolbar-left">
        <button class="btn" @click="handleLoadImages" :disabled="loading">
          <span>📂</span>
          <span>{{ $t('toolbar.loadFolder') }}</span>
        </button>
        <button class="btn" @click="handleLoadFiles" :disabled="loading">
          <span>🖼️</span>
          <span>{{ $t('toolbar.selectImages') }}</span>
        </button>
        <button
          class="btn"
          :class="{ 'btn-active': appendMode }"
          @click="appendMode = !appendMode"
          :disabled="loading"
          :title="appendMode ? $t('toolbar.appendModeTitle') : $t('toolbar.resetModeTitle')"
        >
          <span>{{ appendMode ? "➕" : "🔄" }}</span>
          <span>{{ appendMode ? $t('toolbar.append') : $t('toolbar.reset') }}</span>
        </button>
        <span class="toolbar-sep"></span>
        <!-- 排序 -->
        <select v-model="sortBy" class="sort-select" :title="$t('toolbar.sortBy')">
          <option value="name">{{ $t('toolbar.sortByName') }}</option>
          <option value="date">{{ $t('toolbar.sortByDate') }}</option>
          <option value="size">{{ $t('toolbar.sortBySize') }}</option>
        </select>
        <button class="btn btn-sm" @click="sortAsc = !sortAsc" :title="sortAsc ? $t('toolbar.ascending') : $t('toolbar.descending')">
          {{ sortAsc ? "↑" : "↓" }}
        </button>
      </div>
      <div class="toolbar-right">
        <span v-if="imageCount > 0" class="badge">{{ $t('status.displayImages', { displayCount: displayImages.length, totalCount: imageCount }) }}</span>
        <span v-if="groups.length > 1" class="badge badge-secondary">{{ $t('status.groupCount', { count: groups.length }) }}</span>
        <!-- 筛选 -->
        <select v-model="filterMode" class="mode-select" :title="$t('toolbar.filterMode')">
          <option value="file">{{ $t('toolbar.filterFile') }}</option>
          <option value="group">{{ $t('toolbar.filterGroup') }}</option>
          <option value="path">{{ $t('toolbar.filterPath') }}</option>
        </select>
        <input
          v-model="filterText"
          class="filter-input"
          :class="{ 'filter-error': filterRegexError }"
          :placeholder="$t('toolbar.filterPlaceholder')"
          :title="$t('toolbar.filterTitle')"
        />
        <!-- 语言选择 -->
        <div class="language-switcher">
          <button class="btn btn-sm lang-btn" @click="changeLanguage('zh')" :class="{ 'lang-active': locale === 'zh' }">中文</button>
          <button class="btn btn-sm lang-btn" @click="changeLanguage('en')" :class="{ 'lang-active': locale === 'en' }">EN</button>
        </div>
        <!-- 设置 -->
        <button class="btn btn-sm" @click="settingsOpen = true" :title="$t('toolbar.settings')">⚙️</button>
      </div>
    </header>

    <!-- ===== 错误横幅 ===== -->
    <div v-if="errorMsg" class="error-banner">
      <span>⚠️ {{ errorMsg }}</span>
      <button class="error-close" @click="errorMsg = ''">✕</button>
    </div>

    <!-- ===== 主内容区域 ===== -->
    <main ref="contentRef" class="content">
      <div v-if="groups.length === 0 && !loading" class="state-overlay welcome">
        <div class="welcome-icon">🖼️</div>
        <h2>{{ $t('welcome.title') }}</h2>
        <p>{{ $t('welcome.description') }}</p>
        <p class="welcome-tip">{{ $t('welcome.tip') }}</p>
      </div>

      <!-- 分组树状视图 -->
      <div v-else-if="groupEnabled" class="grouped-view">
        <div v-for="group in displayedGroups" :key="group.id" class="group-section" :style="gridCssVars">
          <div v-show="showNodeTitles" class="group-header" :style="{ '--root-title-color': appSettings.grid.root_title_color, '--root-title-bg': appSettings.grid.root_title_bg }" @click="toggleGroup(group)">
            <span class="tree-toggle">{{ group.groupExpanded ? "▼" : "▶" }}</span>
            <span class="group-icon">📁</span>
            <span class="group-path-prefix">{{ splitRootPath(group.rootPath).parentPath }}</span>
            <span class="group-path-name">{{ splitRootPath(group.rootPath).folderName }}</span>
            <span class="group-header-right">
              <span class="badge">{{ group.isPlaceholder ? $t('group.loading') : $t('group.imageCount', { count: group.images.length }) }}</span>
              <button
                v-if="!group.isPlaceholder"
                class="delete-btn"
                @click.stop="removeGroup(group.id)"
                :title="$t('group.deleteGroup')"
              >✕</button>
            </span>
          </div>

          <div v-if="group.groupExpanded" class="tree-container">
            <div v-if="group.isPlaceholder" class="tree-grid" :style="{ paddingLeft: '12px' }">
              <div class="image-grid skeleton-grid">
                <div v-for="n in 12" :key="n" class="grid-item skeleton-item">
                  <div class="skeleton-pulse"></div>
                </div>
              </div>
            </div>

            <template v-else>
              <template v-for="item in flatItems" :key="item.id">
                <template v-if="item.groupId === group.id">
                  <div
                    v-if="item.type === 'folder' && item.node"
                    class="tree-node"
                    :style="{ paddingLeft: item.depth * 20 + 12 + 'px' }"
                  >
                    <div
                      v-show="showNodeTitles"
                      class="tree-node-header"
                      :style="{ '--child-title-color': appSettings.grid.child_title_color, '--child-title-bg': appSettings.grid.child_title_bg }"
                      @click="toggleExpand(item.node!)"
                    >
                      <span class="tree-toggle">{{ item.node.expanded ? "▼" : "▶" }}</span>
                      <span class="tree-folder-icon">📁</span>
                      <span class="tree-node-name">{{ item.node.name }}</span>
                      <span class="tree-node-right">
                        <span class="tree-node-count">{{ $t('group.nodeCount', { count: countImagesInNode(item.node) }) }}</span>
                        <button
                          class="delete-btn node-delete"
                          @click.stop="removeTreeNode(item.groupId, item.node!)"
                          :title="$t('group.deleteNode')"
                        >✕</button>
                      </span>
                    </div>
                  </div>
                  <div
                    v-if="item.type === 'grid' && item.images"
                    class="tree-grid"
                    :style="{ paddingLeft: item.depth * 20 + 12 + 'px' }"
                  >
                    <div
                      class="image-grid tree-image-grid"
                      :style="{ ...gridCssVars, '--cell-ratio': gridRatios[item.id] || 1 }"
                    >
                      <div
                        v-for="(img, idx) in item.images"
                        :key="img.path + idx"
                        class="grid-item"
                        :class="{ 'grid-item-selected': selectMode && selectedPaths.has(img.path) }"
                        @click="onGridItemClick(img, item.groupId, item.images, $event)"
                      >
                        <img
                          :src="img.data_url"
                          :alt="img.name"
                          :title="img.name"
                          @load="onImageLoad($event, item.id, idx)"
                        />
                        <div v-if="selectMode" class="select-check" :class="{ checked: selectedPaths.has(img.path) }">
                          <span v-if="selectedPaths.has(img.path)">✓</span>
                        </div>
                        <div class="image-name">{{ img.name }}</div>
                      </div>
                    </div>
                  </div>
                </template>
              </template>
            </template>
          </div>
        </div>
      </div>

      <!-- 扁平网格视图 -->
      <div v-else-if="!loading || displayImages.length > 0" class="image-grid" :style="{ ...gridCssVars, '--cell-ratio': gridRatios['flat'] || 1 }">
        <div
          v-for="(img, index) in displayImages"
          :key="img.path + index"
          class="grid-item"
          :class="{ 'grid-item-selected': selectMode && selectedPaths.has(img.path) }"
          @click="onGridItemClick(img, undefined, undefined, $event)"
        >
          <img
            :src="img.data_url"
            :alt="img.name"
            :title="img.name"
            @load="onImageLoad($event, 'flat', index)"
          />
          <div v-if="selectMode" class="select-check" :class="{ checked: selectedPaths.has(img.path) }">
            <span v-if="selectedPaths.has(img.path)">✓</span>
          </div>
          <div class="image-name">{{ img.name }}</div>
        </div>
      </div>

      <div v-else class="image-grid skeleton-grid">
        <div v-for="n in 12" :key="n" class="grid-item skeleton-item">
          <div class="skeleton-pulse"></div>
        </div>
      </div>
    </main>

    <!-- ===== 图片大图查看器 ===== -->
    <div v-if="viewerOpen" class="viewer-overlay" @wheel="onViewerWheel">
      <div class="viewer-backdrop"></div>

      <div
        class="viewer-image-center"
        :class="{ 'pan-active': viewerPanMode }"
        @click.stop
        @wheel.prevent.stop="onViewerWheel"
        @mousedown="onViewerPanDown"
      >
        <img
          v-if="viewerImage"
          :src="viewerImage.data_url"
          :alt="viewerImage.name"
          class="viewer-image"
          :class="[viewerFitMode === 'fit-short-edge' ? 'img-cover' : 'img-contain', viewerFitMode === 'original' ? 'img-original' : '']"
          :style="{ transform: `scale(${viewerZoom}) translate(${viewerPanX}px, ${viewerPanY}px)` }"
          @load="onViewerImageLoad"
          @dragstart="onViewerDragStart($event)"
        />
      </div>

      <div v-if="showImageInfo && viewerImage" class="viewer-info" @click.stop>
        {{ viewerInfoText }}
      </div>

      <button class="viewer-nav viewer-prev" @click.stop="prevImage">‹</button>
      <button class="viewer-nav viewer-next" @click.stop="nextImage">›</button>

      <button class="viewer-close" @click.stop="closeViewer">✕</button>

      <div class="viewer-controls-zone">
        <div class="viewer-controls">
          <button class="btn btn-sm" :class="{ 'btn-active': viewerFitMode === 'original' && viewerZoom === 1 }" @click.stop="setFitMode('original')">{{ $t('viewer.fitOriginal') }}</button>
          <button class="btn btn-sm" :class="{ 'btn-active': viewerFitMode === 'fit-window' }" @click.stop="setFitMode('fit-window')">{{ $t('viewer.fitWindow') }}</button>
          <button class="btn btn-sm" :class="{ 'btn-active': viewerFitMode === 'fit-short-edge' }" @click.stop="setFitMode('fit-short-edge')">{{ $t('viewer.fitShortEdge') }}</button>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" :class="{ 'btn-active': viewerPanMode }" @click.stop="toggleViewerPan">{{ $t('viewer.pan') }}</button>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" @click.stop="showImageInfo = !showImageInfo">{{ showImageInfo ? $t('viewer.hideInfo') : $t('viewer.showInfo') }}</button>
          <button class="btn btn-sm" @click.stop="showMetadata">{{ $t('viewer.metadata') }}</button>
          <span class="viewer-sep"></span>
          <div class="open-with-wrapper" @click.stop>
            <button class="btn btn-sm" @click.stop="openWithMenuOpen = !openWithMenuOpen">{{ $t('viewer.openWith') }}</button>
            <div v-if="openWithMenuOpen" class="open-with-menu">
              <button
                v-for="(item, idx) in openWithList"
                :key="idx"
                class="open-with-item"
                @click.stop="item.action()"
              >{{ item.name }}</button>
            </div>
          </div>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" style="color:#ff6b6b" @click.stop="deleteViewerImage">{{ $t('viewer.delete') }}</button>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" @click.stop="toggleFullscreen">{{ $t('viewer.fullscreen') }}</button>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" @click.stop="closeViewer">{{ $t('viewer.close') }}</button>
        </div>
      </div>

      <!-- 元信息弹窗 -->
      <div v-if="metadataVisible" class="metadata-overlay" @click.stop="metadataVisible = false">
        <div class="metadata-panel" @click.stop>
          <div class="metadata-header">
            <span>{{ $t('metadata.title') }}</span>
            <button class="metadata-close" @click="metadataVisible = false">✕</button>
          </div>
          <div class="metadata-body">
            <div v-if="metadataLoading" class="metadata-loading">{{ $t('metadata.loading') }}</div>
            <div v-else-if="metadataError" class="metadata-error">{{ metadataError }}</div>
            <div v-else-if="metadataFields.length > 0" class="metadata-content">
              <template v-for="(section, sIdx) in groupMetadata()" :key="sIdx">
                <div class="metadata-section-label">{{ section.source }}</div>
                <div
                  v-for="(f, idx) in section.items"
                  :key="idx"
                  class="metadata-row"
                >
                  <span class="metadata-row-name">{{ f.name }}</span>
                  <span class="metadata-row-value">{{ f.value }}</span>
                </div>
              </template>
              <button class="btn btn-sm" style="margin-top:12px" @click="copyMetadata">{{ $t('metadata.copyAll') }}</button>
            </div>
            <div v-else class="metadata-empty">{{ $t('metadata.empty') }}</div>
          </div>
        </div>
      </div>
    </div>

    <!-- ===== 对比模式 ===== -->
    <div v-if="compareOpen && compareImages" class="compare-overlay" @wheel="onCompareWheel">
      <div class="compare-backdrop" @click="closeCompare"></div>
      <div
        ref="compareViewportRef"
        class="compare-viewport"
        :class="{ 'pan-active': comparePanMode }"
        @mousemove="draggingDivider && onCompareMousemove($event)"
        @mousedown="onComparePanDown"
      >
        <!-- 图片 A (左侧/底层) -->
        <div class="compare-pane compare-pane-left">
          <img
            :src="compareImages[0].data_url"
            :alt="compareImages[0].name"
            class="compare-image"
            :class="[compareFitMode === 'fit-short-edge' ? 'img-cover' : 'img-contain', compareFitMode === 'original' ? 'img-original' : '']"
            :style="{ transform: `scale(${compareZoom}) translate(${comparePanX}px, ${comparePanY}px)` }"
            @dragstart="onCompareDragStart($event, 0)"
          />
        </div>
        <!-- 图片 B (右侧，左侧裁切) -->
        <div class="compare-pane compare-pane-right" :style="{ clipPath: `inset(0 0 0 ${dividerPos}%)` }">
          <img
            :src="compareImages[1].data_url"
            :alt="compareImages[1].name"
            class="compare-image"
            :class="[compareFitMode === 'fit-short-edge' ? 'img-cover' : 'img-contain', compareFitMode === 'original' ? 'img-original' : '']"
            :style="{ transform: `scale(${compareZoom}) translate(${comparePanX}px, ${comparePanY}px)` }"
            @dragstart="onCompareDragStart($event, 1)"
          />
        </div>
        <!-- 分隔线 -->
        <div class="compare-divider" :style="{ left: dividerPos + '%' }" @mousedown.prevent="onCompareMousedown">
          <div class="compare-divider-handle">⟷</div>
        </div>
        <!-- 标签 -->
        <div class="compare-label compare-label-left">{{ compareImages[0].name }}</div>
        <div class="compare-label compare-label-right">{{ compareImages[1].name }}</div>
      </div>
      <!-- 关闭按钮 -->
      <button class="viewer-close" @click="closeCompare">✕</button>
      <!-- 底部悬浮操作栏 -->
      <div class="viewer-controls-zone">
        <div class="viewer-controls">
          <button class="btn btn-sm" :class="{ 'btn-active': compareFitMode === 'original' && compareZoom === 1 }" @click.stop="compareFitMode = 'original'; compareZoom = 1">{{ $t('viewer.fitOriginal') }}</button>
          <button class="btn btn-sm" :class="{ 'btn-active': compareFitMode === 'fit-window' }" @click.stop="compareFitMode = 'fit-window'; compareZoom = 1">{{ $t('viewer.fitWindow') }}</button>
          <button class="btn btn-sm" :class="{ 'btn-active': compareFitMode === 'fit-short-edge' }" @click.stop="compareFitMode = 'fit-short-edge'; compareZoom = 1">{{ $t('viewer.fitShortEdge') }}</button>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" :class="{ 'btn-active': comparePanMode }" @click.stop="toggleComparePan">{{ $t('viewer.pan') }}</button>
          <span class="viewer-sep"></span>
          <button class="btn btn-sm" @click.stop="closeCompare">{{ $t('viewer.close') }}</button>
        </div>
      </div>
    </div>

    <!-- ===== 设置面板 ===== -->
    <div v-if="settingsOpen" class="metadata-overlay" @click.self="settingsOpen = false">
      <div class="metadata-panel settings-panel" @click.stop>
        <div class="metadata-header">
          <span>{{ $t('settings.title') }}</span>
          <button class="metadata-close" @click="settingsOpen = false">✕</button>
        </div>
        <div class="settings-tabs">
          <button
            class="settings-tab"
            :class="{ 'tab-active': settingsTab === 'grid' }"
            @click="settingsTab = 'grid'"
          >{{ $t('settings.gridStyle') }}</button>
          <button
            class="settings-tab"
            :class="{ 'tab-active': settingsTab === 'presets' }"
            @click="settingsTab = 'presets'"
          >{{ $t('settings.filterPresets') }}</button>
          <button
            class="settings-tab"
            :class="{ 'tab-active': settingsTab === 'programs' }"
            @click="settingsTab = 'programs'"
          >{{ $t('settings.externalPrograms') }}</button>
        </div>
        <div class="metadata-body">
          <!-- 格子样式 -->
          <div v-if="settingsTab === 'grid'" class="settings-form">
            <label class="setting-row">
              <span>{{ $t('settings.borderRadius') }}</span>
              <input type="range" v-model.number="appSettings.grid.border_radius" min="0" max="30" />
              <span class="setting-val">{{ appSettings.grid.border_radius }}</span>
            </label>
            <label class="setting-row">
              <span>{{ $t('settings.gap') }}</span>
              <input type="range" v-model.number="appSettings.grid.gap" min="0" max="40" />
              <span class="setting-val">{{ appSettings.grid.gap }}</span>
            </label>
            <label class="setting-row">
              <span>{{ $t('settings.minWidth') }}</span>
              <input type="range" v-model.number="appSettings.grid.min_width" min="10" max="400" />
              <span class="setting-val">{{ appSettings.grid.min_width }}</span>
            </label>
            <label class="setting-row">
              <span>{{ $t('settings.backgroundColor') }}</span>
              <input type="color" v-model="appSettings.grid.background_color" />
              <span class="setting-val" style="font-family:monospace">{{ appSettings.grid.background_color }}</span>
            </label>
            <div class="settings-sub-label">{{ $t('settings.rootTitle') }}</div>
            <label class="setting-row">
              <span>{{ $t('settings.textColor') }}</span>
              <input type="color" v-model="appSettings.grid.root_title_color" />
              <span class="setting-val" style="font-family:monospace">{{ appSettings.grid.root_title_color }}</span>
            </label>
            <label class="setting-row">
              <span>{{ $t('settings.bgColor') }}</span>
              <input type="color" v-model="appSettings.grid.root_title_bg" />
              <span class="setting-val" style="font-family:monospace">{{ appSettings.grid.root_title_bg }}</span>
            </label>
            <div class="settings-sub-label">{{ $t('settings.childTitle') }}</div>
            <label class="setting-row">
              <span>{{ $t('settings.textColor') }}</span>
              <input type="color" v-model="appSettings.grid.child_title_color" />
              <span class="setting-val" style="font-family:monospace">{{ appSettings.grid.child_title_color }}</span>
            </label>
            <label class="setting-row">
              <span>{{ $t('settings.bgColor') }}</span>
              <input type="color" v-model="appSettings.grid.child_title_bg" />
              <span class="setting-val" style="font-family:monospace">{{ appSettings.grid.child_title_bg }}</span>
            </label>
            <div class="settings-sub-label">{{ $t('settings.groupContainer') }}</div>
            <label class="setting-row">
              <span>{{ $t('settings.bgColor') }}</span>
              <input type="color" v-model="appSettings.grid.group_bg" />
              <span class="setting-val" style="font-family:monospace">{{ appSettings.grid.group_bg }}</span>
            </label>
          </div>
          <!-- 筛选预设 -->
          <div v-if="settingsTab === 'presets'" class="settings-form">
            <div v-for="(preset, idx) in appSettings.filter_presets" :key="idx" class="preset-card">
              <input v-model="preset.name" class="preset-name" :placeholder="$t('settings.presetNamePlaceholder')" />
              <input v-model="preset.pattern" class="preset-pattern" :placeholder="$t('settings.presetPatternPlaceholder')" />
              <select v-model="preset.mode" class="mode-select">
                <option value="file">{{ $t('toolbar.filterFile') }}</option>
                <option value="group">{{ $t('toolbar.filterGroup') }}</option>
                <option value="path">{{ $t('toolbar.filterPath') }}</option>
              </select>
              <button class="btn btn-sm" @click="applyPreset(preset)">{{ $t('settings.apply') }}</button>
              <button class="delete-btn" @click="removeFilterPreset(idx)">✕</button>
            </div>
            <button class="btn btn-sm" style="margin-top:8px" @click="addFilterPreset">{{ $t('settings.addPreset') }}</button>
          </div>
          <!-- 外部程序 -->
          <div v-if="settingsTab === 'programs'" class="settings-form">
            <div v-for="(prog, idx) in appSettings.external_programs" :key="idx" class="preset-card">
              <input v-model="prog.name" class="preset-name" :placeholder="$t('settings.programNamePlaceholder')" />
              <span class="program-path" :title="prog.path">{{ prog.path }}</span>
              <button class="delete-btn" @click="removeExternalProgram(idx)">✕</button>
            </div>
            <button class="btn btn-sm" style="margin-top:8px" @click="browseExternalProgram">{{ $t('settings.browseProgram') }}</button>
          </div>
        </div>
        <div class="settings-footer">
          <button v-if="settingsTab === 'grid'" class="btn" @click="resetStyleSettings">{{ $t('settings.resetStyle') }}</button>
          <div class="settings-footer-spacer"></div>
          <button class="btn" @click="saveSettings">{{ $t('settings.save') }}</button>
        </div>
      </div>
    </div>

    <!-- ===== 新分组命名对话框 ===== -->
    <div v-if="showNewGroupDialog" class="metadata-overlay" @click.self="showNewGroupDialog = false">
      <div class="metadata-panel" style="width:400px" @click.stop>
        <div class="metadata-header">
          <span>{{ $t('groupDialog.title') }}</span>
          <button class="metadata-close" @click="showNewGroupDialog = false">✕</button>
        </div>
        <div class="metadata-body">
          <label class="setting-row" style="margin-bottom:12px">
            <span>{{ $t('groupDialog.name') }}</span>
            <input
              v-model="newGroupName"
              class="filter-input"
              style="flex:1;width:auto"
              @keydown.enter="createGroupFromSelection()"
            />
          </label>
          <p style="font-size:12px;color:#888">
            {{ $t('groupDialog.selectedCount', { count: selectedPaths.size }) }}
          </p>
        </div>
        <div class="settings-footer">
          <button class="btn btn-sm" @click="showNewGroupDialog = false">{{ $t('groupDialog.cancel') }}</button>
          <button class="btn" @click="createGroupFromSelection">{{ $t('groupDialog.create') }}</button>
        </div>
      </div>
    </div>

    <!-- ===== 底部控制栏 ===== -->
    <footer class="bottom-bar">
      <div class="bottom-bar-left">
        <button
          class="btn btn-sm"
          :class="{ 'btn-active': selectMode }"
          @click="toggleSelectMode"
          :title="selectMode ? $t('bottombar.selectModeTitle') : $t('bottombar.viewModeTitle')"
        >
          <span>{{ selectMode ? $t('bottombar.select') : $t('bottombar.view') }}</span>
        </button>
        <span class="toolbar-sep"></span>
        <label class="toggle-label">
          <span>{{ $t('bottombar.groupDisplay') }}</span>
          <div class="toggle-switch" @click="groupEnabled = !groupEnabled">
            <div class="toggle-thumb" :class="{ active: groupEnabled }"></div>
          </div>
        </label>
        <button
          v-if="groups.length > 0 && groupEnabled"
          class="btn btn-sm"
          @click="toggleAllGroups"
        >
          <span>{{ allCollapsed ? $t('bottombar.expandAll') : $t('bottombar.collapseAll') }}</span>
        </button>
        <button
          v-if="groupEnabled"
          class="btn btn-sm"
          :class="{ 'btn-active': !showNodeTitles }"
          @click="showNodeTitles = !showNodeTitles"
          :title="showNodeTitles ? $t('bottombar.hideTitlesTitle') : $t('bottombar.showTitlesTitle')"
        >
          <span>{{ showNodeTitles ? "👁️" : "🚫" }}</span>
          <span>{{ $t('bottombar.titles') }}</span>
        </button>
      </div>
      <div class="bottom-bar-center" v-if="selectMode">
        <button
          v-if="selectedPaths.size > 0"
          class="btn btn-sm"
          style="background:#6b1a1a;border-color:#c73a3a;color:#ffaaaa"
          @click="openDeleteConfirm()"
        >
          🗑️ {{ $t('delete.confirmButton') }} ({{ selectedPaths.size }})
        </button>
        <button
          class="btn btn-sm"
          style="background:#1a5bc7;border-color:#2d7eff"
          @click="openNewGroupDialog"
        >
          {{ $t('bottombar.newGroup', { count: selectedPaths.size }) }}
        </button>
        <button
          v-if="selectedPaths.size === 2"
          class="btn btn-sm"
          style="background:#2a6b2a;border-color:#3a8b3a"
          @click="openCompare"
        >
          {{ $t('bottombar.compare') }}
        </button>
      </div>
      <div class="bottom-bar-right">
        <span v-if="groups.length > 0" class="badge">{{ $t('bottombar.groupCount', { count: groups.length }) }}</span>
        <span v-if="imageCount > 0" class="badge">{{ $t('bottombar.imageCount', { count: imageCount }) }}</span>
      </div>
    </footer>

    <!-- ===== 删除确认对话框 ===== -->
    <div v-if="showDeleteConfirm" class="metadata-overlay" @click.self="showDeleteConfirm = false">
      <div class="metadata-panel" style="width:400px" @click.stop>
        <div class="metadata-header">
          <span>{{ $t('delete.title') }}</span>
          <button class="metadata-close" @click="showDeleteConfirm = false">✕</button>
        </div>
        <div class="metadata-body">
          <p style="color:#ccc;margin-bottom:8px">{{ $t('delete.confirm', { count: selectedPaths.size }) }}</p>
        </div>
        <div class="settings-footer">
          <button class="btn btn-sm" @click="showDeleteConfirm = false" :disabled="deleting">{{ $t('delete.cancel') }}</button>
          <button class="btn" style="background:#6b1a1a;border-color:#c73a3a;color:#ffaaaa" @click="deleteImages([...selectedPaths])" :disabled="deleting">{{ deleting ? '...' : $t('delete.confirmButton') }}</button>
        </div>
      </div>
    </div>

    <!-- ===== 拖放覆盖层 ===== -->
    <div v-if="dragOver" class="drag-overlay">
      <div class="drag-box">
        <div class="drag-icon">📥</div>
        <div class="drag-text">{{ $t('dragOverlay.text') }}</div>
      </div>
    </div>
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

:root {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica,
    Arial, sans-serif;
  font-size: 14px;
  line-height: 1.5;
  color: #e0e0e0;
  background-color: #1a1a2e;
}

body {
  overflow: hidden;
}

::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: #444;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #555;
}
</style>

<style scoped>
/* ===== 布局 ===== */
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

/* ===== 通用按钮 ===== */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 18px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  border: 1px solid #0f3460;
  border-radius: 8px;
  background: #1a3a6b;
  color: #e0e0e0;
  transition: all 0.2s;
  user-select: none;
}
.btn:hover:not(:disabled) {
  background: #234b8a;
  border-color: #1a5bc7;
}
.btn:active:not(:disabled) {
  transform: scale(0.97);
}
.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.btn-active {
  background: #1a5bc7;
  border-color: #2d7eff;
}
.btn-sm {
  padding: 5px 12px;
  font-size: 12px;
  gap: 4px;
}

/* ===== 分隔线 ===== */
.toolbar-sep {
  width: 1px;
  height: 24px;
  background: #2a3a5a;
  flex-shrink: 0;
}

/* ===== 排序 & 筛选控件 ===== */
.sort-select,
.mode-select {
  padding: 6px 10px;
  font-size: 12px;
  border: 1px solid #0f3460;
  border-radius: 6px;
  background: #1a3a6b;
  color: #e0e0e0;
  cursor: pointer;
  outline: none;
}
.sort-select:focus,
.mode-select:focus {
  border-color: #1a5bc7;
}

.filter-input {
  padding: 6px 10px;
  font-size: 12px;
  border: 1px solid #0f3460;
  border-radius: 6px;
  background: #1a2a4a;
  color: #e0e0e0;
  outline: none;
  width: 140px;
  transition: border-color 0.2s;
}
.filter-input:focus {
  border-color: #1a5bc7;
}
.filter-input.filter-error {
  border-color: #ff6b6b;
}

/* ===== 徽标 ===== */
.badge {
  font-size: 13px;
  color: #aaa;
  background: #1e2d50;
  padding: 4px 12px;
  border-radius: 12px;
  white-space: nowrap;
}
.badge-secondary {
  background: #2a1a6b;
  color: #b8a0ff;
}

/* ===== 工具栏 ===== */
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 20px;
  background: #16213e;
  border-bottom: 1px solid #0f3460;
  flex-shrink: 0;
  user-select: none;
}
.toolbar-left {
  display: flex;
  align-items: center;
  gap: 10px;
}
.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* ===== 错误横幅 ===== */
.error-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 20px;
  background: #3a1a1a;
  border-bottom: 1px solid #6a2a2a;
  color: #ff6b6b;
  font-size: 14px;
  flex-shrink: 0;
}
.error-close {
  background: none;
  border: none;
  color: #ff6b6b;
  cursor: pointer;
  font-size: 16px;
  padding: 2px 6px;
  border-radius: 4px;
}
.error-close:hover {
  background: rgba(255, 107, 107, 0.15);
}

/* ===== 主内容 ===== */
.content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

/* ===== 欢迎页 ===== */
.state-overlay {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: 12px;
  color: #888;
}
.welcome-icon {
  font-size: 64px;
  margin-bottom: 8px;
  opacity: 0.6;
}
.state-overlay h2 {
  font-size: 24px;
  font-weight: 600;
  color: #aaa;
}
.state-overlay p {
  font-size: 15px;
  color: #777;
}
.welcome-tip {
  font-size: 13px !important;
  color: #555 !important;
  margin-top: 4px;
}

/* ===== 骨架屏 ===== */
.skeleton-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
}
.skeleton-item {
  position: relative;
  border-radius: 8px;
  overflow: hidden;
  background: #0f1a30;
  aspect-ratio: 1;
}
.skeleton-pulse {
  width: 100%;
  height: 100%;
  background: linear-gradient(
    110deg,
    #0f1a30 30%,
    #1a2a4a 50%,
    #0f1a30 70%
  );
  background-size: 200% 100%;
  animation: skeleton-shine 1.5s ease-in-out infinite;
}
@keyframes skeleton-shine {
  0% {
    background-position: 200% 0;
  }
  100% {
    background-position: -200% 0;
  }
}

/* ===== 扁平网格 ===== */
.image-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(var(--cell-min-width, 200px), 1fr));
  gap: var(--cell-gap, 12px);
}
.tree-image-grid {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
}
.tree-image-grid .grid-item {
  width: var(--cell-min-width, 200px);
}
.grid-item {
  position: relative;
  border-radius: var(--cell-radius, 8px);
  overflow: hidden;
  background: var(--cell-bg, #0f1a30);
  aspect-ratio: var(--cell-ratio, 1);
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}
.grid-item:hover {
  transform: translateY(-2px);
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
}
.grid-item img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.image-name {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  padding: 6px 10px;
  font-size: 12px;
  color: #fff;
  background: linear-gradient(transparent, rgba(0, 0, 0, 0.8));
  opacity: 0;
  transition: opacity 0.25s;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.grid-item:hover .image-name {
  opacity: 1;
}
.grid-item-selected {
  outline: 3px solid #4a9eff;
  outline-offset: -3px;
}
.select-check {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 2px solid rgba(255,255,255,0.6);
  background: rgba(0,0,0,0.35);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2;
  transition: all 0.15s;
}
.select-check.checked {
  background: #4a9eff;
  border-color: #4a9eff;
  color: #fff;
  font-size: 13px;
  font-weight: 700;
}

/* ===== 分组视图 ===== */
.grouped-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.group-section {
  background: var(--group-bg, #141e33);
  border: 1px solid #1a2a4a;
  border-radius: 10px;
  overflow: hidden;
}
.group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  background: var(--root-title-bg, #1a2a4a);
  border-bottom: 1px solid #22335a;
  font-size: 13px;
  color: var(--root-title-color, #ccc);
  user-select: none;
  cursor: pointer;
  position: relative;
}
.group-icon {
  font-size: 16px;
  flex-shrink: 0;
}
.group-path-prefix {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: monospace;
  font-size: 11px;
  color: #666;
  max-width: 35%;
  flex-shrink: 1;
}
.group-path-name {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  font-weight: 600;
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 40%;
  pointer-events: none;
}
.group-header-right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
.delete-btn {
  background: none;
  border: none;
  color: #666;
  cursor: pointer;
  font-size: 14px;
  padding: 2px 6px;
  border-radius: 4px;
  line-height: 1;
  transition: all 0.15s;
  flex-shrink: 0;
}
.delete-btn:hover {
  color: #ff6b6b;
  background: rgba(255, 107, 107, 0.12);
}
.node-delete {
  font-size: 11px;
  padding: 1px 4px;
  opacity: 0;
  margin-left: 2px;
}
.tree-node-header:hover .node-delete {
  opacity: 1;
}

/* ===== 树状结构 ===== */
.tree-container {
  padding: 8px 0;
}
.tree-node {
  margin: 1px 0;
}
.tree-node-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-radius: 6px;
  cursor: pointer;
  user-select: none;
  background: var(--child-title-bg, transparent);
  color: var(--child-title-color, #ccc);
  transition: filter 0.15s;
}
.tree-node-header:hover {
  filter: brightness(1.2);
}
.tree-toggle {
  font-size: 10px;
  width: 14px;
  text-align: center;
  color: #888;
  flex-shrink: 0;
}
.tree-folder-icon {
  font-size: 14px;
  flex-shrink: 0;
}
.tree-node-name {
  flex: 1;
  text-align: center;
  font-size: 13px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tree-node-right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.tree-node-count {
  font-size: 11px;
  color: #888;
}
.tree-grid {
  padding: 4px 0 8px;
}

/* ===== 底部控制栏 ===== */
.bottom-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 20px;
  background: #16213e;
  border-top: 1px solid #0f3460;
  flex-shrink: 0;
  user-select: none;
}
.bottom-bar-left {
  display: flex;
  align-items: center;
  gap: 12px;
}
.bottom-bar-center {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  justify-content: center;
}
.bottom-bar-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* ===== 开关 ===== */
.toggle-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: #aaa;
  cursor: pointer;
}
.toggle-switch {
  position: relative;
  width: 40px;
  height: 22px;
  background: #2a3a5a;
  border-radius: 11px;
  transition: background 0.25s;
  cursor: pointer;
}
.toggle-thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  background: #888;
  border-radius: 50%;
  transition: all 0.25s;
}
.toggle-thumb.active {
  left: 20px;
  background: #4a9eff;
}

/* ===== 图片大图查看器 ===== */
.viewer-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  user-select: none;
}
.viewer-backdrop {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.85);
}
.viewer-image-center {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1;
}
.viewer-image {
  display: block;
  border-radius: 2px;
  box-shadow: 0 4px 40px rgba(0, 0, 0, 0.6);
  transition: transform 0.1s;
  transform-origin: center center;
}
.viewer-image.img-contain {
  max-width: 100vw;
  max-height: 100vh;
  object-fit: contain;
  width: auto;
  height: auto;
}
.viewer-image.img-cover {
  width: 100vw;
  height: 100vh;
  object-fit: cover;
  max-width: none;
  max-height: none;
}
.viewer-image.img-original {
  max-width: none;
  max-height: none;
  object-fit: none;
}
.pan-active {
  cursor: grab;
}
.pan-active:active {
  cursor: grabbing;
}

.viewer-info {
  position: fixed;
  bottom: 80px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 3;
  padding: 6px 16px;
  background: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(6px);
  border-radius: 8px;
  font-size: 13px;
  color: #ccc;
  text-align: center;
  max-width: 80vw;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  pointer-events: none;
}
.viewer-nav {
  position: fixed;
  top: 50%;
  transform: translateY(-50%);
  z-index: 2;
  width: 48px;
  height: 80px;
  border: none;
  background: rgba(255, 255, 255, 0.06);
  color: #ccc;
  font-size: 32px;
  cursor: pointer;
  border-radius: 8px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}
.viewer-nav:hover {
  background: rgba(255, 255, 255, 0.14);
  color: #fff;
}
.viewer-prev {
  left: 16px;
}
.viewer-next {
  right: 16px;
}
.viewer-close {
  position: fixed;
  top: 16px;
  right: 16px;
  z-index: 2;
  width: 40px;
  height: 40px;
  border: none;
  background: rgba(255, 255, 255, 0.08);
  color: #ccc;
  font-size: 20px;
  cursor: pointer;
  border-radius: 8px;
  transition: all 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}
.viewer-close:hover {
  background: rgba(255, 80, 80, 0.25);
  color: #ff6b6b;
}
.viewer-controls-zone {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  height: 70px;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
}
.viewer-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 18px;
  background: rgba(22, 33, 62, 0.92);
  border: 1px solid #0f3460;
  border-radius: 12px;
  backdrop-filter: blur(8px);
  opacity: 0;
  transition: opacity 0.25s;
  pointer-events: none;
}
.viewer-controls-zone:hover .viewer-controls {
  opacity: 1;
  pointer-events: auto;
}
.viewer-sep {
  width: 1px;
  height: 24px;
  background: #2a3a5a;
  flex-shrink: 0;
}

/* 打开方式下拉 */
.open-with-wrapper {
  position: relative;
}
.open-with-menu {
  position: absolute;
  bottom: 100%;
  left: 0;
  margin-bottom: 6px;
  background: #1a2a4a;
  border: 1px solid #0f3460;
  border-radius: 8px;
  overflow: hidden;
  min-width: 200px;
  z-index: 10;
}
.open-with-item {
  display: block;
  width: 100%;
  padding: 8px 14px;
  font-size: 13px;
  color: #ccc;
  background: none;
  border: none;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  cursor: pointer;
  text-align: left;
  transition: background 0.15s;
}
.open-with-item:last-child {
  border-bottom: none;
}
.open-with-item:hover {
  background: rgba(74, 158, 255, 0.12);
  color: #fff;
}

/* ===== 对比模式 ===== */
.compare-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  user-select: none;
}
.compare-backdrop {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.92);
}
.compare-viewport {
  position: absolute;
  inset: 0;
  overflow: hidden;
}
.compare-pane {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}
.compare-pane-right {
  z-index: 1;
}
.compare-image {
  display: block;
  min-width: 0;
  min-height: 0;
}
.compare-image.img-contain {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  width: auto;
  height: auto;
}
.compare-image.img-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.compare-image.img-original {
  max-width: none;
  max-height: none;
  object-fit: none;
}
.compare-divider {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 4px;
  background: rgba(255,255,255,0.5);
  transform: translateX(-50%);
  z-index: 2;
  cursor: col-resize;
}
.compare-divider-handle {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 36px;
  height: 36px;
  background: rgba(0,0,0,0.7);
  border: 2px solid rgba(255,255,255,0.5);
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  font-size: 14px;
  cursor: col-resize;
}
.compare-label {
  position: absolute;
  top: 12px;
  padding: 4px 12px;
  background: rgba(0,0,0,0.6);
  border-radius: 6px;
  font-size: 13px;
  color: #ccc;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  z-index: 2;
  pointer-events: none;
}
.compare-label-left {
  left: 12px;
}
.compare-label-right {
  right: 12px;
}

/* ===== 元信息弹窗 ===== */
.metadata-overlay {
  position: fixed;
  inset: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.4);
}
.metadata-panel {
  background: #1a2a4a;
  border: 1px solid #0f3460;
  border-radius: 12px;
  width: 480px;
  max-width: 90vw;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 40px rgba(0, 0, 0, 0.5);
}
.settings-panel {
  width: 560px;
}
.metadata-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 18px;
  border-bottom: 1px solid #0f3460;
  font-size: 14px;
  font-weight: 600;
  color: #ccc;
}
.metadata-close {
  background: none;
  border: none;
  color: #888;
  font-size: 18px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
}
.metadata-close:hover {
  color: #ff6b6b;
  background: rgba(255, 107, 107, 0.12);
}
.metadata-body {
  padding: 18px;
  overflow-y: auto;
  min-height: 80px;
}
.metadata-loading,
.metadata-empty,
.metadata-error {
  color: #888;
  text-align: center;
  padding: 20px 0;
}
.metadata-error {
  color: #ff6b6b;
}
.metadata-section-label {
  font-size: 13px;
  font-weight: 600;
  color: #8ab4f8;
  margin: 12px 0 6px;
  padding-bottom: 4px;
  border-bottom: 1px solid #1a2a4a;
}
.metadata-section-label:first-child {
  margin-top: 0;
}
.metadata-row {
  display: flex;
  gap: 8px;
  padding: 4px 0;
  font-size: 13px;
  line-height: 1.5;
  border-bottom: 1px solid rgba(255, 255, 255, 0.03);
}
.metadata-row-name {
  flex: 0 0 160px;
  color: #888;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex-shrink: 0;
}
.metadata-row-value {
  flex: 1;
  color: #ddd;
  word-break: break-word;
}

/* ===== 设置面板 ===== */
.settings-tabs {
  display: flex;
  border-bottom: 1px solid #0f3460;
  padding: 0 18px;
  gap: 0;
}
.settings-tab {
  padding: 8px 16px;
  font-size: 13px;
  color: #888;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  transition: all 0.2s;
}
.settings-tab:hover {
  color: #ccc;
}
.settings-tab.tab-active {
  color: #4a9eff;
  border-bottom-color: #4a9eff;
}
.settings-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.setting-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  color: #ccc;
}
.setting-row span:first-child {
  width: 110px;
  flex-shrink: 0;
  color: #999;
}
.settings-sub-label {
  font-size: 12px;
  font-weight: 600;
  color: #8ab4f8;
  padding-top: 8px;
  border-top: 1px solid rgba(255,255,255,0.05);
}
.setting-row input[type="range"] {
  flex: 1;
  accent-color: #4a9eff;
}
.setting-row input[type="color"] {
  width: 36px;
  height: 28px;
  border: 1px solid #0f3460;
  border-radius: 4px;
  cursor: pointer;
  background: none;
  padding: 2px;
}
.setting-val {
  width: 60px;
  text-align: right;
  color: #aaa;
  font-size: 12px;
  flex-shrink: 0;
}
.preset-card {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}
.preset-name {
  width: 100px;
  padding: 5px 8px;
  font-size: 12px;
  border: 1px solid #0f3460;
  border-radius: 4px;
  background: #0f1a30;
  color: #ddd;
  outline: none;
}
.preset-pattern {
  flex: 1;
  padding: 5px 8px;
  font-size: 12px;
  border: 1px solid #0f3460;
  border-radius: 4px;
  background: #0f1a30;
  color: #ddd;
  outline: none;
  font-family: monospace;
}
.preset-name:focus,
.preset-pattern:focus {
  border-color: #1a5bc7;
}
.program-path {
  flex: 1;
  font-size: 11px;
  color: #888;
  font-family: monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.settings-footer {
  padding: 12px 18px;
  border-top: 1px solid #0f3460;
  display: flex;
  gap: 10px;
}
.settings-footer-spacer {
  flex: 1;
}

/* ===== 拖放覆盖层 ===== */
.drag-overlay {
  position: fixed;
  inset: 0;
  z-index: 999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(4px);
  pointer-events: none;
}
.drag-box {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding: 48px 64px;
  border: 3px dashed #4a9eff;
  border-radius: 16px;
  background: rgba(26, 58, 107, 0.4);
}
.drag-icon {
  font-size: 48px;
  opacity: 0.8;
}
.drag-text {
  font-size: 18px;
  color: #ccc;
  font-weight: 500;
}

/* ===== 语言切换 ===== */
.language-switcher {
  display: flex;
  gap: 2px;
}
.lang-btn {
  padding: 4px 7px !important;
  font-size: 11px !important;
  min-width: 34px;
  justify-content: center;
}
.lang-btn.lang-active {
  background: #1a5bc7;
  border-color: #2d7eff;
}
</style>
