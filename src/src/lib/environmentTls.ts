import type { TLSConfig } from '../types/project';

export interface TlsDraft {
  useGrpcs: boolean;
  useMtls: boolean;
  authority: string;
  caCertPath: string;
  clientCertPath: string;
  clientKeyPath: string;
  skipVerify: boolean;
}

// buildTlsDraftFromConfig 将持久化 TLS 配置映射为环境编辑弹窗的草稿状态。
//
// 该转换的目标是把 mode 抽象成更直观的开关：
// - useGrpcs: 是否使用 TLS
// - useMtls:  是否启用双向证书
// - authority: 连接层 authority（用于网关主机路由与 TLS SNI）
//
// 这样在 UI 层可以直接围绕“是否 grpcs / 是否 mTLS”渲染，而不暴露底层 mode 细节。
export function buildTlsDraftFromConfig(tls?: TLSConfig): TlsDraft {
  if (!tls || tls.mode === 'insecure') {
    return {
      useGrpcs: false,
      useMtls: false,
      authority: tls?.authority ?? '',
      caCertPath: '',
      clientCertPath: '',
      clientKeyPath: '',
      skipVerify: false,
    };
  }

  return {
    useGrpcs: true,
    useMtls: Boolean(tls.clientCert || tls.clientKey),
    authority: tls.authority ?? '',
    caCertPath: tls.caCert ?? '',
    clientCertPath: tls.clientCert ?? '',
    clientKeyPath: tls.clientKey ?? '',
    skipVerify: Boolean(tls.skipVerify),
  };
}

// buildTlsConfigFromDraft 将环境编辑草稿转换为可持久化 TLS 配置。
//
// 约定如下：
// 1) useGrpcs=false 时返回 insecure（明文 gRPC），并保留 authority；
// 2) useGrpcs=true 且 useMtls=true 时返回 custom，并附带 client cert/key；
// 3) useGrpcs=true 且仅配置 CA 时返回 custom（单向 TLS + 自定义信任）；
// 4) useGrpcs=true 且未配置证书时返回 system（单向 TLS + 系统信任）；
// 5) authority 在三种模式下都透传，供连接层设置 HTTP/2 `:authority` / TLS SNI。
export function buildTlsConfigFromDraft(draft: TlsDraft): TLSConfig {
  const authority = draft.authority.trim();
  if (!draft.useGrpcs) {
    return {
      mode: 'insecure',
      authority: authority || undefined,
    };
  }

  const caCert = draft.caCertPath.trim();
  const clientCert = draft.clientCertPath.trim();
  const clientKey = draft.clientKeyPath.trim();

  if (draft.useMtls) {
    return {
      mode: 'custom',
      authority: authority || undefined,
      caCert: caCert || undefined,
      clientCert: clientCert || undefined,
      clientKey: clientKey || undefined,
      skipVerify: draft.skipVerify,
    };
  }

  if (caCert) {
    return {
      mode: 'custom',
      authority: authority || undefined,
      caCert,
      skipVerify: draft.skipVerify,
    };
  }

  return {
    mode: 'system',
    authority: authority || undefined,
    skipVerify: draft.skipVerify,
  };
}

// validateTlsDraft 校验 TLS 草稿的必要条件。
//
// 当前只对 mTLS 做强校验：启用 mTLS 时 client cert 与 client key 必须同时提供，
// 否则握手一定失败，提前在前端阻断可减少一次无效请求。
export function validateTlsDraft(draft: TlsDraft): string | null {
  if (!draft.useGrpcs || !draft.useMtls) {
    return null;
  }

  if (!draft.clientCertPath.trim()) {
    return '启用 mTLS 时请填写 Client Cert 路径。';
  }

  if (!draft.clientKeyPath.trim()) {
    return '启用 mTLS 时请填写 Client Key 路径。';
  }

  return null;
}
