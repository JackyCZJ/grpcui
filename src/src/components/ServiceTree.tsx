import { useState, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Folder, FileCode, ChevronRight, ChevronDown, Trash2 } from 'lucide-react';
import { cn } from '../lib/utils';
import { groupServicesByProtoFolder, UNGROUPED_FOLDER } from '../lib/protoImport';
import type { Service, Method, MethodType } from '../types';

interface ServiceTreeProps {
  services: Service[];
  selectedMethod?: string;
  groupRootPath?: string | null;
  onMethodSelect: (service: Service, method: Method) => void;
  onDeleteFolder?: (sourcePaths: string[], folderLabel: string) => void;
  onDeleteService?: (service: Service) => void;
  onDeleteMethod?: (service: Service, method: Method) => void;
  className?: string;
}

interface ServiceNodeProps {
  service: Service;
  isExpanded: boolean;
  selectedMethod?: string;
  onToggle: (serviceName: string) => void;
  onMethodSelect: (service: Service, method: Method) => void;
  onDeleteService?: (service: Service) => void;
  onDeleteMethod?: (service: Service, method: Method) => void;
}

interface MethodNodeProps {
  service: Service;
  method: Method;
  isSelected: boolean;
  onClick: () => void;
  onDelete?: (service: Service, method: Method) => void;
}

const methodTypeColors: Record<MethodType, string> = {
  unary: 'bg-green-500',
  server_stream: 'bg-blue-500',
  client_stream: 'bg-yellow-500',
  bidi_stream: 'bg-purple-500',
};

function useMethodTypeLabels(): Record<MethodType, string> {
  const { t } = useTranslation();
  return {
    unary: t('method.unary'),
    server_stream: t('method.server'),
    client_stream: t('method.client'),
    bidi_stream: t('method.bidi'),
  };
}

function MethodNode({ service, method, isSelected, onClick, onDelete }: MethodNodeProps) {
  const { t } = useTranslation();
  const methodTypeLabels = useMethodTypeLabels();
  return (
    <div
      onClick={onClick}
      className={cn(
        'flex items-center justify-between px-2 py-1.5 rounded cursor-pointer text-sm group ml-4 gap-2',
        isSelected
          ? 'bg-[var(--color-primary-soft)] text-[var(--color-primary)]'
          : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-3)]'
      )}
    >
      <div className="min-w-0 flex items-center">
        <span
          className={cn(
            'w-2 h-2 rounded-full mr-2',
            methodTypeColors[method.type]
          )}
          title={methodTypeLabels[method.type]}
        />
        <FileCode size={14} className="mr-2 text-[var(--color-text-muted)] group-hover:text-[var(--color-text-secondary)]" />
        <span className="truncate">{method.name}</span>
      </div>
      {onDelete && service.sourcePath && (
        <button
          type="button"
          title={t('service.deleteMethod')}
          onClick={(event) => {
            event.stopPropagation();
            onDelete(service, method);
          }}
          className="opacity-0 group-hover:opacity-100 transition-opacity text-[var(--color-text-muted)] hover:text-[var(--color-danger-text)]"
        >
          <Trash2 size={13} />
        </button>
      )}
    </div>
  );
}

function ServiceNode({
  service,
  isExpanded,
  selectedMethod,
  onToggle,
  onMethodSelect,
  onDeleteService,
  onDeleteMethod,
}: ServiceNodeProps) {
  const { t } = useTranslation();
  const hasSelection = service.methods.some((m) => m.fullName === selectedMethod);

  return (
    <div className="mb-1">
      <div
        onClick={() => onToggle(service.fullName)}
        className={cn(
          'flex items-center justify-between px-2 py-1.5 rounded cursor-pointer group gap-2',
          hasSelection ? 'bg-[var(--color-surface-soft)]' : 'hover:bg-[var(--color-surface-3)]'
        )}
      >
        <div className="min-w-0 flex items-center">
          {isExpanded ? (
            <ChevronDown size={14} className="mr-1 text-[var(--color-text-muted)]" />
          ) : (
            <ChevronRight size={14} className="mr-1 text-[var(--color-text-muted)]" />
          )}
          <Folder size={14} className="mr-2 text-[var(--color-primary)]" />
          <span className="text-sm text-[var(--color-primary)] font-medium truncate">
            {service.name}
          </span>
        </div>
        {onDeleteService && service.sourcePath && (
          <button
            type="button"
            title={t('service.deleteProto')}
            onClick={(event) => {
              event.stopPropagation();
              onDeleteService(service);
            }}
            className="opacity-0 group-hover:opacity-100 transition-opacity text-[var(--color-text-muted)] hover:text-[var(--color-danger-text)]"
          >
            <Trash2 size={13} />
          </button>
        )}
      </div>

      {isExpanded && (
        <div className="mt-1">
          {service.methods.map((method) => (
            <MethodNode
              key={method.fullName}
              service={service}
              method={method}
              isSelected={method.fullName === selectedMethod || method.name === selectedMethod}
              onClick={() => onMethodSelect(service, method)}
              onDelete={onDeleteMethod}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * resolveFolderLabel 把内部目录标识转换为可读文案。
 *
 * `groupServicesByProtoFolder` 使用 `__ungrouped__` 与 `.` 作为技术标记，
 * 这里统一翻译成界面可读文本，避免用户看到内部实现细节。
 */
function resolveFolderLabel(folder: string, t: (key: string) => string): string {
  if (folder === UNGROUPED_FOLDER) {
    return t('service.folderUngrouped');
  }

  if (folder === '.') {
    return t('service.folderRoot');
  }

  return folder;
}

export function ServiceTree({
  services,
  selectedMethod,
  groupRootPath,
  onMethodSelect,
  onDeleteFolder,
  onDeleteService,
  onDeleteMethod,
  className,
}: ServiceTreeProps) {
  const { t } = useTranslation();
  const [expandedServices, setExpandedServices] = useState<Set<string>>(() => {
    if (services.length > 0) {
      return new Set([services[0].fullName]);
    }
    return new Set<string>();
  });

  const serviceGroups = useMemo(
    () => groupServicesByProtoFolder(services, groupRootPath),
    [services, groupRootPath]
  );

  const handleToggle = useCallback((serviceName: string) => {
    setExpandedServices((prev) => {
      const next = new Set(prev);
      if (next.has(serviceName)) {
        next.delete(serviceName);
      } else {
        next.add(serviceName);
      }
      return next;
    });
  }, []);

  const handleMethodSelect = useCallback(
    (service: Service, method: Method) => {
      onMethodSelect(service, method);
    },
    [onMethodSelect]
  );

  if (services.length === 0) {
    return (
      <div
        className={cn(
          'flex flex-col items-center justify-center p-8 text-[var(--color-text-muted)]',
          className
        )}
      >
        <Folder size={48} className="mb-4 opacity-50" />
        <p className="text-sm">{t('service.noServices')}</p>
        <p className="text-xs mt-1">{t('service.importPrompt')}</p>
      </div>
    );
  }

  return (
    <div className={cn('p-3', className)}>
      <div className="text-xs font-medium text-[var(--color-text-muted)] uppercase tracking-wider mb-3">
        {t('service.services')}
      </div>
      <div className="space-y-1">
        {serviceGroups.map((group) => (
          <div key={group.folder} className="mb-3 last:mb-0">
            {serviceGroups.length > 1 && (
              <div className="px-2 py-1 mb-1 text-xs text-[var(--color-text-secondary)] uppercase tracking-wide flex items-center justify-between gap-2">
                <div className="min-w-0 flex items-center gap-2">
                  <span className="truncate">{resolveFolderLabel(group.folder, t)}</span>
                  <span className="text-[10px] text-[var(--color-text-muted)]">{group.services.length}</span>
                </div>
                {onDeleteFolder && (() => {
                  const folderSourcePaths = Array.from(
                    new Set(
                      group.services
                        .map((service) => service.sourcePath?.trim())
                        .filter((path): path is string => Boolean(path))
                    )
                  );

                  if (folderSourcePaths.length === 0) {
                    return null;
                  }

                  return (
                    <button
                      type="button"
                      title={t('service.deleteFolder')}
                      onClick={() =>
                        onDeleteFolder(folderSourcePaths, resolveFolderLabel(group.folder, t))
                      }
                      className="text-[var(--color-text-muted)] hover:text-[var(--color-danger-text)]"
                    >
                      <Trash2 size={13} />
                    </button>
                  );
                })()}
              </div>
            )}
            <div className="space-y-1">
              {group.services.map((service) => (
                <ServiceNode
                  key={service.fullName}
                  service={service}
                  isExpanded={expandedServices.has(service.fullName)}
                  selectedMethod={selectedMethod}
                  onToggle={handleToggle}
                  onMethodSelect={handleMethodSelect}
                  onDeleteService={onDeleteService}
                  onDeleteMethod={onDeleteMethod}
                />
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default ServiceTree;
