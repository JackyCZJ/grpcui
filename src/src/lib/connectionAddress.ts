/**
 * AddressEnvironment 描述地址解析所需的最小环境结构。
 *
 * 仅保留 `id` 和 `baseUrl` 两个字段，便于在 UI 与测试里复用，
 * 避免为纯地址逻辑引入不必要的项目领域耦合。
 */
export interface AddressEnvironment {
  id: string;
  baseUrl: string;
}

/**
 * 将输入地址规范化为可直接使用的字符串。
 *
 * 这里统一做 `trim`，保证后续连接逻辑不会因为首尾空格导致
 * “看起来有值、实际不可用”的问题。
 */
function normalizeAddress(address: string | null | undefined): string {
  return typeof address === 'string' ? address.trim() : '';
}

/**
 * 解析最终连接地址。
 *
 * 优先级：
 * 1. 用户在地址栏输入的地址
 * 2. 当前环境的 baseUrl
 * 3. 默认地址 `localhost:50051`
 *
 * 该函数用于 connect/import proto 等需要“兜底地址”场景，
 * 保证入口行为一致，减少重复判断逻辑。
 */
export function resolveConnectionAddress(
  inputAddress: string,
  environmentBaseUrl?: string | null,
  fallbackAddress = 'localhost:50051'
): string {
  const manualAddress = normalizeAddress(inputAddress);
  if (manualAddress) {
    return manualAddress;
  }

  const envAddress = normalizeAddress(environmentBaseUrl);
  if (envAddress) {
    return envAddress;
  }

  return fallbackAddress;
}

/**
 * 根据用户在下拉框中选择的环境，解析应回填到地址栏的地址。
 *
 * - 找到环境且 baseUrl 非空：返回该地址（已 trim）
 * - 未找到环境或地址为空：返回 null
 *
 * UI 可用该返回值决定是否覆盖当前地址，
 * 从而实现“选择环境后自动回填地址栏”。
 */
export function resolveAddressForEnvironmentSelection(
  environments: AddressEnvironment[],
  selectedEnvironmentId: string
): string | null {
  const selected = environments.find((env) => env.id === selectedEnvironmentId);
  const envAddress = normalizeAddress(selected?.baseUrl);
  return envAddress || null;
}

/**
 * 规范化 Proto 文件选择结果。
 *
 * Tauri Dialog 的 `open` 在单选模式下理论上返回 `string | null`，
 * 但类型上仍可能是 `string[]`。这里统一折叠为单个路径，
 * 让调用方只处理 `string | null`，避免分支遗漏导致“点击无反应”。
 */
export function normalizeProtoDialogSelection(
  fileSelection: string | string[] | null
): string | null {
  if (!fileSelection) {
    return null;
  }

  if (Array.isArray(fileSelection)) {
    return fileSelection[0] || null;
  }

  return fileSelection;
}
