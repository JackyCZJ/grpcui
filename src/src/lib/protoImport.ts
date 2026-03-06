import { join } from '@tauri-apps/api/path';
import { readDir } from '@tauri-apps/plugin-fs';

import type { Service } from '../types';

/**
 * ProtoFolderGroup 描述“按 proto 文件目录分组”后的服务集合。
 *
 * `folder` 为组标识：
 * - 真实目录时为相对目录（统一使用 `/` 分隔）
 * - 无法推导目录时为 `__ungrouped__`
 */
export interface ProtoFolderGroup {
  folder: string;
  services: Service[];
}

/**
 * ProtoDirectoryImportResult 描述目录导入后的文件发现结果。
 *
 * - `absolutePaths`: 供日志与诊断使用的绝对路径
 * - `relativePaths`: 供后端解析器加载的相对路径（相对 `rootDir`）
 */
export interface ProtoDirectoryImportResult {
  absolutePaths: string[];
  relativePaths: string[];
}

const UNGROUPED_FOLDER = '__ungrouped__';
const PROJECT_PROTO_ROOT_PREFIX = '__grpcui_proto_root__:';

/**
 * ProjectProtoConnectConfig 描述“项目已保存 proto 配置”还原后的连接参数。
 *
 * - protoFiles: 传给后端的 proto 文件列表
 * - importPaths: 传给后端的 import 根候选
 * - groupRootPath: 前端服务树分组时使用的目录根（可为空）
 */
export interface ProjectProtoConnectConfig {
  protoFiles: string[];
  importPaths: string[];
  groupRootPath: string | null;
}

/**
 * ProjectProtoRemovalResult 描述按来源路径删除 proto 后的结果。
 *
 * - nextStoredProtoFiles: 可直接写回项目实体的 protoFiles
 * - removedCount: 实际删除的 proto 条数
 */
export interface ProjectProtoRemovalResult {
  nextStoredProtoFiles: string[];
  removedCount: number;
}

/**
 * pathContainsBySegment 判断路径是否以另一条路径作为“完整段后缀”。
 *
 * 例如：
 * - `/a/b/c.proto` 包含 `b/c.proto`
 * - `b/c.proto` 不包含 `c.protox`（避免误删）
 */
function pathContainsBySegment(fullPath: string, maybeTailPath: string): boolean {
  if (!fullPath || !maybeTailPath) {
    return false;
  }

  if (fullPath === maybeTailPath) {
    return true;
  }

  return fullPath.endsWith(`/${maybeTailPath}`);
}

/**
 * isProtoPathMatch 判断两条 proto 路径是否可视为同一来源。
 *
 * 兼容场景：
 * - 绝对路径与相对路径（例如 `/a/b/user.proto` vs `b/user.proto`）
 * - 不同分隔符风格（Windows `\` 与 Unix `/`）
 * - 完全一致路径
 */
export function isProtoPathMatch(firstPath: string, secondPath: string): boolean {
  const normalizedFirstPath = normalizePath(firstPath);
  const normalizedSecondPath = normalizePath(secondPath);
  if (!normalizedFirstPath || !normalizedSecondPath) {
    return false;
  }

  return (
    pathContainsBySegment(normalizedFirstPath, normalizedSecondPath)
    || pathContainsBySegment(normalizedSecondPath, normalizedFirstPath)
  );
}

/**
 * shouldRemoveProtoFile 判断某个已存储 proto 是否命中删除目标。
 *
 * 兼容场景：
 * - 完全相等（常规目录导入）
 * - 绝对路径 vs 相对 sourcePath（单文件导入常见）
 * - 相对路径 vs 绝对删除路径（保持双向容错）
 */
function shouldRemoveProtoFile(storedFilePath: string, removalPaths: Set<string>): boolean {
  for (const removalPath of removalPaths) {
    if (isProtoPathMatch(storedFilePath, removalPath)) {
      return true;
    }
  }
  return false;
}

/**
 * 将任意平台路径统一为使用 `/` 分隔，并尽量保持根路径语义。
 *
 * 该函数会：
 * - 折叠反斜杠与重复斜杠
 * - 保留 `/` 与 `C:/` 这类根路径
 * - 对普通路径移除末尾斜杠
 */
function normalizePath(path: string): string {
  let normalized = path.replace(/\\/g, '/').trim();
  if (!normalized) {
    return '';
  }

  normalized = normalized.replace(/\/+/g, '/');

  if (normalized === '/') {
    return '/';
  }

  if (/^[a-zA-Z]:\/?$/.test(normalized)) {
    return normalized.length === 2 ? `${normalized}/` : normalized;
  }

  return normalized.replace(/\/+$/g, '');
}

/**
 * 获取给定路径的父目录。
 *
 * - 到达根目录时返回 `null`
 * - 兼容 Unix 根路径 `/` 与 Windows 盘符根 `C:/`
 */
function parentDirectory(path: string): string | null {
  const normalized = normalizePath(path);
  if (!normalized || normalized === '/' || /^[a-zA-Z]:\/$/.test(normalized)) {
    return null;
  }

  const separatorIndex = normalized.lastIndexOf('/');
  if (separatorIndex < 0) {
    return null;
  }

  if (separatorIndex === 0) {
    return '/';
  }

  const parent = normalized.slice(0, separatorIndex);
  if (/^[a-zA-Z]:$/.test(parent)) {
    return `${parent}/`;
  }

  return parent;
}

/**
 * 基于目录根生成 import 路径候选。
 *
 * 候选顺序为：
 * 1. 当前目录
 * 2. 逐级父目录（最多 `maxAncestorDepth` 层）
 *
 * 该策略可兼容两类常见 import：
 * - 相对根目录：`import "user/types.proto"`
 * - 仓库前缀：`import "common/proto/types.proto"`
 */
export function buildImportPathCandidates(
  rootDir: string,
  maxAncestorDepth = 8
): string[] {
  const normalizedRoot = normalizePath(rootDir);
  if (!normalizedRoot) {
    return [];
  }

  const candidates = [normalizedRoot];
  let current = normalizedRoot;

  for (let depth = 0; depth < maxAncestorDepth; depth += 1) {
    const parent = parentDirectory(current);
    if (!parent) {
      break;
    }
    candidates.push(parent);
    current = parent;
  }

  return candidates;
}

/**
 * encodeProjectProtoFilesForFolderImport 将目录导入结果编码为项目可持久化结构。
 *
 * 约定第一项保存导入根目录标记，后续项保存相对 proto 路径。
 * 这样项目再次打开时即可恢复 import root 和服务分组语义。
 */
export function encodeProjectProtoFilesForFolderImport(
  rootDir: string,
  relativePaths: string[]
): string[] {
  const normalizedRoot = normalizePath(rootDir);
  const normalizedRelativePaths = relativePaths
    .map((filePath) => normalizePath(filePath))
    .filter((filePath) => filePath.length > 0);

  if (!normalizedRoot || normalizedRelativePaths.length === 0) {
    return normalizedRelativePaths;
  }

  return [`${PROJECT_PROTO_ROOT_PREFIX}${normalizedRoot}`, ...normalizedRelativePaths];
}

/**
 * decodeProjectProtoFilesForConnect 将项目保存的 proto 配置还原为连接参数。
 *
 * 兼容两类历史数据：
 * 1) 目录导入编码格式（包含 root 标记和相对路径）
 * 2) 旧版直接保存文件路径数组（例如单文件导入）
 */
export function decodeProjectProtoFilesForConnect(
  storedProtoFiles?: string[] | null
): ProjectProtoConnectConfig {
  const normalizedStored = (storedProtoFiles ?? [])
    .map((filePath) => filePath.trim())
    .filter((filePath) => filePath.length > 0);

  if (normalizedStored.length === 0) {
    return {
      protoFiles: [],
      importPaths: [],
      groupRootPath: null,
    };
  }

  const rootMarker = normalizedStored[0];
  if (rootMarker.startsWith(PROJECT_PROTO_ROOT_PREFIX)) {
    const rootDir = normalizePath(rootMarker.slice(PROJECT_PROTO_ROOT_PREFIX.length));
    const protoFiles = normalizedStored
      .slice(1)
      .map((filePath) => normalizePath(filePath))
      .filter((filePath) => filePath.length > 0);

    return {
      protoFiles,
      importPaths: rootDir ? buildImportPathCandidates(rootDir) : [],
      groupRootPath: rootDir || null,
    };
  }

  return {
    protoFiles: normalizedStored
      .map((filePath) => normalizePath(filePath))
      .filter((filePath) => filePath.length > 0),
    importPaths: [],
    groupRootPath: null,
  };
}

/**
 * removeProjectProtoFilesBySourcePaths 按 sourcePath 删除项目里的 proto 引用。
 *
 * 该函数用于“删除文件夹 / 删除 proto / 删除方法所属 proto”场景：
 * 1) 先把项目存储格式解码成可操作模型；
 * 2) 按路径集合执行删除（大小写与分隔符规整后比较）；
 * 3) 再编码回可持久化的 protoFiles，保证项目切换后仍可正确恢复。
 */
export function removeProjectProtoFilesBySourcePaths(
  storedProtoFiles: string[] | null | undefined,
  sourcePaths: string[]
): ProjectProtoRemovalResult {
  const connectConfig = decodeProjectProtoFilesForConnect(storedProtoFiles);
  const normalizedRemovalPaths = new Set(
    sourcePaths
      .map((path) => normalizePath(path))
      .filter((path) => path.length > 0)
  );

  if (normalizedRemovalPaths.size === 0 || connectConfig.protoFiles.length === 0) {
    return {
      nextStoredProtoFiles: storedProtoFiles ?? [],
      removedCount: 0,
    };
  }

  const nextProtoFiles = connectConfig.protoFiles.filter((filePath) => {
    const normalizedFilePath = normalizePath(filePath);
    return !shouldRemoveProtoFile(normalizedFilePath, normalizedRemovalPaths);
  });

  const removedCount = connectConfig.protoFiles.length - nextProtoFiles.length;

  if (!connectConfig.groupRootPath) {
    return {
      nextStoredProtoFiles: nextProtoFiles,
      removedCount,
    };
  }

  return {
    nextStoredProtoFiles:
      nextProtoFiles.length > 0
        ? encodeProjectProtoFilesForFolderImport(connectConfig.groupRootPath, nextProtoFiles)
        : [],
    removedCount,
  };
}

/**
 * resolveProtoImportDisplayName 用于从完整路径提取预览展示名。
 *
 * 规则：
 * - 优先取最后一级文件名（兼容 Unix `/` 与 Windows `\` 分隔符）
 * - 若路径异常导致无法提取，则回退原始路径，避免 UI 展示空字符串
 */
export function resolveProtoImportDisplayName(path: string): string {
  const normalizedPath = path.trim();
  if (!normalizedPath) {
    return '';
  }

  const fileName = normalizedPath.split(/[\\/]/).filter(Boolean).pop();
  return fileName || normalizedPath;
}

/**
 * 判断给定路径是否为 proto 文件。
 *
 * 使用大小写不敏感判断，确保 `.proto` / `.PROTO` 都能被识别。
 */
export function isProtoFilePath(path: string): boolean {
  return path.trim().toLowerCase().endsWith('.proto');
}

/**
 * 将绝对文件路径转换为相对根目录路径。
 *
 * 若文件位于 `rootDir` 下，则返回相对路径（使用 `/` 分隔）；
 * 若不在根目录下，则返回标准化后的原始路径，用于保底兼容。
 */
export function toRelativeProtoPath(absolutePath: string, rootDir: string): string {
  const normalizedRoot = normalizePath(rootDir);
  const normalizedAbsolute = normalizePath(absolutePath);

  if (!normalizedRoot) {
    return normalizedAbsolute;
  }

  if (normalizedAbsolute === normalizedRoot) {
    return '';
  }

  if (normalizedRoot === '/') {
    return normalizedAbsolute.startsWith('/')
      ? normalizedAbsolute.slice(1)
      : normalizedAbsolute;
  }

  const rootPrefix = `${normalizedRoot}/`;
  if (normalizedAbsolute.startsWith(rootPrefix)) {
    return normalizedAbsolute.slice(rootPrefix.length);
  }

  return normalizedAbsolute;
}

/**
 * 从 sourcePath 解析用于分组的目录名。
 *
 * - 若 `sourcePath` 可解析出目录，返回目录路径
 * - 若在根目录下（无子目录），返回 `.`
 * - 若无 sourcePath，返回 `null`
 */
function resolveFolderFromSourcePath(
  sourcePath: string | undefined,
  rootDir?: string | null
): string | null {
  if (!sourcePath) {
    return null;
  }

  const relativeSource = rootDir
    ? toRelativeProtoPath(sourcePath, rootDir)
    : normalizePath(sourcePath);

  const segments = relativeSource.split('/').filter(Boolean);
  if (segments.length === 0) {
    return null;
  }

  if (segments.length === 1) {
    return '.';
  }

  return segments.slice(0, -1).join('/');
}

/**
 * 按 proto 所在目录对服务进行分组。
 *
 * 分组规则：
 * - `service.sourcePath` 可解析目录：按目录分组
 * - 无法解析：放入 `__ungrouped__`
 *
 * 返回结果会按目录名升序排序，便于 UI 稳定展示。
 */
export function groupServicesByProtoFolder(
  services: Service[],
  rootDir?: string | null
): ProtoFolderGroup[] {
  const grouped = new Map<string, Service[]>();

  for (const service of services) {
    const folder = resolveFolderFromSourcePath(service.sourcePath, rootDir) ?? UNGROUPED_FOLDER;
    if (!grouped.has(folder)) {
      grouped.set(folder, []);
    }
    grouped.get(folder)?.push(service);
  }

  return Array.from(grouped.entries())
    .sort(([folderA], [folderB]) => folderA.localeCompare(folderB))
    .map(([folder, groupedServices]) => ({
      folder,
      services: groupedServices,
    }));
}

/**
 * 递归扫描目录并收集全部 proto 文件。
 *
 * 该函数用于“导入文件夹”场景：
 * 1. 深度遍历子目录
 * 2. 收集 `.proto` 文件绝对路径
 * 3. 生成相对根目录路径，供后端按 import root 解析
 */
export async function collectProtoFilesFromDirectory(
  rootDir: string
): Promise<ProtoDirectoryImportResult> {
  const discovered = new Set<string>();

  // walk 负责递归遍历目录树，逐层展开并收集 proto 文件。
  async function walk(dir: string): Promise<void> {
    const entries = await readDir(dir);

    for (const entry of entries) {
      const entryPath = await join(dir, entry.name);

      if (entry.isDirectory) {
        await walk(entryPath);
        continue;
      }

      if (entry.isFile && isProtoFilePath(entryPath)) {
        discovered.add(normalizePath(entryPath));
      }
    }
  }

  await walk(rootDir);

  const absolutePaths = Array.from(discovered).sort((a, b) => a.localeCompare(b));
  const relativePaths = absolutePaths.map((absolutePath) =>
    toRelativeProtoPath(absolutePath, rootDir)
  );

  return {
    absolutePaths,
    relativePaths,
  };
}

export { UNGROUPED_FOLDER };
