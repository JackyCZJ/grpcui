import { describe, expect, it } from 'vitest';

import {
  buildTlsConfigFromDraft,
  buildTlsDraftFromConfig,
  validateTlsDraft,
  type TlsDraft,
} from '../../src/lib/environmentTls';

describe('environmentTls', () => {
  // createTlsDraft 统一构建测试草稿，避免每个用例重复声明完整结构。
  const createTlsDraft = (patch?: Partial<TlsDraft>): TlsDraft => ({
    useGrpcs: false,
    useMtls: false,
    authority: '',
    caCertPath: '',
    clientCertPath: '',
    clientKeyPath: '',
    skipVerify: false,
    ...patch,
  });

  it('应将 insecure 配置映射为 grpcs 关闭草稿', () => {
    const draft = buildTlsDraftFromConfig({ mode: 'insecure', authority: 'api.example.com' });
    expect(draft.useGrpcs).toBe(false);
    expect(draft.useMtls).toBe(false);
    expect(draft.authority).toBe('api.example.com');
  });

  it('应将 custom + client cert/key 映射为 mTLS 草稿', () => {
    const draft = buildTlsDraftFromConfig({
      mode: 'custom',
      caCert: '/tmp/ca.pem',
      clientCert: '/tmp/client.crt',
      clientKey: '/tmp/client.key',
      skipVerify: true,
    });

    expect(draft.useGrpcs).toBe(true);
    expect(draft.useMtls).toBe(true);
    expect(draft.caCertPath).toBe('/tmp/ca.pem');
    expect(draft.clientCertPath).toBe('/tmp/client.crt');
    expect(draft.clientKeyPath).toBe('/tmp/client.key');
    expect(draft.skipVerify).toBe(true);
  });

  it('应将 grpcs 关闭草稿回写为 insecure', () => {
    const tls = buildTlsConfigFromDraft(createTlsDraft());
    expect(tls).toEqual({ mode: 'insecure', authority: undefined });
  });

  it('应将 grpcs + mtls 草稿回写为 custom', () => {
    const tls = buildTlsConfigFromDraft(
      createTlsDraft({
        useGrpcs: true,
        useMtls: true,
        caCertPath: '  /tmp/ca.pem ',
        clientCertPath: ' /tmp/client.crt ',
        clientKeyPath: '/tmp/client.key ',
        skipVerify: true,
      })
    );

    expect(tls).toEqual({
      mode: 'custom',
      authority: undefined,
      caCert: '/tmp/ca.pem',
      clientCert: '/tmp/client.crt',
      clientKey: '/tmp/client.key',
      skipVerify: true,
    });
  });

  it('应支持 authority 字段回写到 TLS 配置', () => {
    const tls = buildTlsConfigFromDraft(
      createTlsDraft({
        useGrpcs: true,
        authority: ' api.example.com ',
      })
    );

    expect(tls).toEqual({
      mode: 'system',
      authority: 'api.example.com',
      skipVerify: false,
    });
  });

  it('应校验 mTLS 缺少证书路径的错误', () => {
    const error = validateTlsDraft(
      createTlsDraft({
        useGrpcs: true,
        useMtls: true,
        clientCertPath: '',
        clientKeyPath: '/tmp/client.key',
      })
    );

    expect(error).toContain('Client Cert');
  });
});
