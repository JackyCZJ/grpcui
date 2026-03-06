import { describe, expect, it } from 'vitest';

import {
  buildImportPathCandidates,
  decodeProjectProtoFilesForConnect,
  encodeProjectProtoFilesForFolderImport,
  groupServicesByProtoFolder,
  isProtoFilePath,
  isProtoPathMatch,
  removeProjectProtoFilesBySourcePaths,
  resolveProtoImportDisplayName,
  toRelativeProtoPath,
} from '../../src/lib/protoImport';
import type { Service } from '../../src/types';

function makeService(fullName: string, sourcePath?: string): Service {
  return {
    name: fullName.split('.').pop() ?? fullName,
    fullName,
    methods: [],
    sourcePath,
  };
}

describe('isProtoFilePath', () => {
  it('识别大小写不同的 proto 扩展名', () => {
    expect(isProtoFilePath('/tmp/a.proto')).toBe(true);
    expect(isProtoFilePath('/tmp/b.PROTO')).toBe(true);
    expect(isProtoFilePath('/tmp/c.txt')).toBe(false);
  });
});

describe('resolveProtoImportDisplayName', () => {
  it('从 Unix 路径提取文件名', () => {
    expect(resolveProtoImportDisplayName('/tmp/apis/user.proto')).toBe('user.proto');
  });

  it('从 Windows 路径提取文件名', () => {
    expect(resolveProtoImportDisplayName('C:\\repo\\proto\\order.proto')).toBe(
      'order.proto'
    );
  });
});

describe('isProtoPathMatch', () => {
  it('支持绝对路径与相对路径按段匹配', () => {
    expect(
      isProtoPathMatch('/Users/demo/workspace/apis/order/order.proto', 'order/order.proto')
    ).toBe(true);
  });

  it('不匹配不同文件名', () => {
    expect(
      isProtoPathMatch('/Users/demo/workspace/apis/order/order.proto', 'order/user.proto')
    ).toBe(false);
  });
});

describe('toRelativeProtoPath', () => {
  it('路径在根目录内时返回相对路径', () => {
    expect(
      toRelativeProtoPath('/Users/demo/apis/user/service.proto', '/Users/demo/apis')
    ).toBe('user/service.proto');
  });

  it('路径不在根目录内时返回标准化绝对路径', () => {
    expect(
      toRelativeProtoPath('/opt/other/service.proto', '/Users/demo/apis')
    ).toBe('/opt/other/service.proto');
  });
});

describe('groupServicesByProtoFolder', () => {
  it('按 proto 所在目录分组并保持组内服务顺序', () => {
    const services: Service[] = [
      makeService('demo.user.UserService', 'user/user.proto'),
      makeService('demo.order.OrderService', 'order/order.proto'),
      makeService('demo.user.ProfileService', 'user/profile.proto'),
    ];

    const groups = groupServicesByProtoFolder(services);

    expect(groups.map((group) => group.folder)).toEqual(['order', 'user']);
    expect(groups[1].services.map((service) => service.fullName)).toEqual([
      'demo.user.UserService',
      'demo.user.ProfileService',
    ]);
  });

  it('没有 sourcePath 时归入未分组桶', () => {
    const groups = groupServicesByProtoFolder([makeService('demo.Greeter')]);
    expect(groups).toHaveLength(1);
    expect(groups[0].folder).toBe('__ungrouped__');
  });

  it('提供根目录时将绝对路径折叠为相对目录', () => {
    const groups = groupServicesByProtoFolder(
      [makeService('demo.user.UserService', '/Users/demo/apis/user/user.proto')],
      '/Users/demo/apis'
    );

    expect(groups[0].folder).toBe('user');
  });
});


describe('buildImportPathCandidates', () => {
  it('从目录根向上生成稳定的 import 路径候选', () => {
    expect(buildImportPathCandidates('/Users/demo/workspace/common/proto')).toEqual([
      '/Users/demo/workspace/common/proto',
      '/Users/demo/workspace/common',
      '/Users/demo/workspace',
      '/Users/demo',
      '/Users',
      '/',
    ]);
  });

  it('限制最大向上层级，避免导入路径无限扩散', () => {
    expect(buildImportPathCandidates('/a/b/c/d/e', 3)).toEqual([
      '/a/b/c/d/e',
      '/a/b/c/d',
      '/a/b/c',
      '/a/b',
    ]);
  });
});

describe('project proto config encode/decode', () => {
  it('目录导入配置可编码并完整还原连接参数', () => {
    const encoded = encodeProjectProtoFilesForFolderImport('/Users/demo/apis', [
      'user/user.proto',
      'order/order.proto',
    ]);

    const decoded = decodeProjectProtoFilesForConnect(encoded);

    expect(decoded.protoFiles).toEqual(['user/user.proto', 'order/order.proto']);
    expect(decoded.groupRootPath).toBe('/Users/demo/apis');
    expect(decoded.importPaths[0]).toBe('/Users/demo/apis');
  });

  it('兼容旧版直接保存 proto 文件路径的项目配置', () => {
    const decoded = decodeProjectProtoFilesForConnect([
      '/tmp/demo/a.proto',
      '/tmp/demo/b.proto',
    ]);

    expect(decoded.protoFiles).toEqual(['/tmp/demo/a.proto', '/tmp/demo/b.proto']);
    expect(decoded.importPaths).toEqual([]);
    expect(decoded.groupRootPath).toBeNull();
  });
});

describe('removeProjectProtoFilesBySourcePaths', () => {
  it('可从目录导入配置中删除指定 proto，并保留目录根标记', () => {
    const stored = encodeProjectProtoFilesForFolderImport('/Users/demo/apis', [
      'user/user.proto',
      'order/order.proto',
    ]);

    const result = removeProjectProtoFilesBySourcePaths(stored, ['order/order.proto']);
    const decoded = decodeProjectProtoFilesForConnect(result.nextStoredProtoFiles);

    expect(result.removedCount).toBe(1);
    expect(decoded.groupRootPath).toBe('/Users/demo/apis');
    expect(decoded.protoFiles).toEqual(['user/user.proto']);
  });

  it('删除全部目录导入 proto 后返回空数组', () => {
    const stored = encodeProjectProtoFilesForFolderImport('/Users/demo/apis', [
      'user/user.proto',
    ]);

    const result = removeProjectProtoFilesBySourcePaths(stored, ['user/user.proto']);

    expect(result.removedCount).toBe(1);
    expect(result.nextStoredProtoFiles).toEqual([]);
  });

  it('可从单文件导入配置中删除绝对路径', () => {
    const stored = ['/Users/demo/apis/user.proto', '/Users/demo/apis/order.proto'];
    const result = removeProjectProtoFilesBySourcePaths(stored, [
      '/Users/demo/apis/order.proto',
    ]);

    expect(result.removedCount).toBe(1);
    expect(result.nextStoredProtoFiles).toEqual(['/Users/demo/apis/user.proto']);
  });

  it('支持用相对 sourcePath 删除绝对路径存储项', () => {
    const stored = ['/Users/demo/workspace/apis/order/order.proto'];
    const result = removeProjectProtoFilesBySourcePaths(stored, ['order/order.proto']);

    expect(result.removedCount).toBe(1);
    expect(result.nextStoredProtoFiles).toEqual([]);
  });
});
