import { useState, useCallback, useMemo, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Send, Save, Plus, X } from 'lucide-react';
import { cn } from '../lib/utils';
import { EnvironmentSelector } from './environment/EnvironmentSelector';
import {
  buildSchemaBodyFromRequest,
  buildDefaultBodyFromSchema,
  coerceInputValue,
  getCurrentBodyObject,
  replacePathValue,
} from '../lib/requestSchema';
import type {
  MethodType,
  MetadataEntry,
  MethodInputSchema,
  MessageFieldSchema,
} from '../types';
import type {
  Project,
  ProjectEnvironment,
  EnvRefType,
} from '../types/project';

interface RequestPanelProps {
  selectedMethod?: {
    service: string;
    method: string;
    type: MethodType;
  };
  methodInputSchema?: MethodInputSchema | null;
  body: string;
  metadata: MetadataEntry[];
  project: Project | null;
  environments: ProjectEnvironment[];
  envRefType: EnvRefType;
  selectedEnvironmentId?: string;
  onBodyChange: (body: string) => void;
  onMetadataChange: (metadata: MetadataEntry[]) => void;
  onEnvRefChange: (type: EnvRefType, envId?: string) => void;
  onSend: () => void;
  isStreamConnected?: boolean;
  isStreamInputClosed?: boolean;
  onEndStream?: () => void;
  onCloseStream?: () => void;
  onSave?: () => void;
  isLoading?: boolean;
  className?: string;
}

type RequestTab = 'body' | 'metadata' | 'environment';

function useMethodTypeLabels(): Record<MethodType, string> {
  const { t } = useTranslation();
  return {
    unary: t('method.unary'),
    server_stream: t('method.serverStream'),
    client_stream: t('method.clientStream'),
    bidi_stream: t('method.bidiStream'),
  };
}

const methodTypeColors: Record<MethodType, string> = {
  unary: 'bg-green-500/20 text-green-400',
  server_stream: 'bg-blue-500/20 text-blue-400',
  client_stream: 'bg-yellow-500/20 text-yellow-400',
  bidi_stream: 'bg-purple-500/20 text-purple-400',
};

interface SchemaFieldEditorProps {
  field: MessageFieldSchema;
  path: string[];
  value: unknown;
  onChange: (path: string[], field: MessageFieldSchema, value: unknown) => void;
}

// SchemaFieldEditor 渲染单个 schema 字段编辑器，并在 message 字段上递归渲染子字段。
function SchemaFieldEditor({ field, path, value, onChange }: SchemaFieldEditorProps) {
  const label = `${field.jsonName}${field.required ? ' *' : ''}`;

  if (field.map) {
    return (
      <div className="space-y-1">
        <div className="text-xs text-[var(--color-text-secondary)]">{label}</div>
        <div className="rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-1)] px-3 py-2 text-xs text-[var(--color-text-muted)]">
          {'map<' + (field.mapKeyType ?? 'string') + ', ' + (field.mapValueType ?? 'value') + '>'}
        </div>
      </div>
    );
  }

  if (field.repeated) {
    return (
      <div className="space-y-1">
        <div className="text-xs text-[var(--color-text-secondary)]">{label}</div>
        <textarea
          value={Array.isArray(value) ? JSON.stringify(value, null, 2) : '[]'}
          onChange={(event) => {
            try {
              const parsed = JSON.parse(event.target.value);
              if (Array.isArray(parsed)) {
                onChange(path, field, parsed);
              }
            } catch {
              // ignore invalid array text during typing
            }
          }}
          className="w-full min-h-[72px] rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-0)] px-2 py-1 text-xs text-[var(--color-text-secondary)] focus:border-[var(--color-primary)] focus:outline-none"
        />
      </div>
    );
  }

  if (field.kind === 'message') {
    const objectValue = value && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};

    return (
      <div className="space-y-2 rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-1)] p-3">
        <div className="text-xs text-[var(--color-text-secondary)]">{label} ({field.type})</div>
        <div className="space-y-2">
          {field.fields.map((nestedField) => (
            <SchemaFieldEditor
              key={`${path.join('.')}.${nestedField.jsonName}`}
              field={nestedField}
              path={[...path, nestedField.jsonName]}
              value={objectValue[nestedField.jsonName]}
              onChange={onChange}
            />
          ))}
        </div>
      </div>
    );
  }

  if (field.kind === 'enum') {
    return (
      <label className="block space-y-1">
        <span className="text-xs text-[var(--color-text-secondary)]">{label}</span>
        <select
          value={typeof value === 'string' ? value : (field.enumValues[0] ?? '')}
          onChange={(event) => onChange(path, field, event.target.value)}
          className="w-full rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-0)] px-2 py-1 text-sm text-[var(--color-text-primary)] focus:border-[var(--color-primary)] focus:outline-none"
        >
          {field.enumValues.map((enumValue) => (
            <option key={enumValue} value={enumValue}>
              {enumValue}
            </option>
          ))}
        </select>
      </label>
    );
  }

  if (field.type === 'bool') {
    return (
      <label className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => onChange(path, field, event.target.checked)}
          className="rounded border-[var(--color-surface-3)]"
        />
        <span>{label}</span>
      </label>
    );
  }

  return (
    <label className="block space-y-1">
      <span className="text-xs text-[var(--color-text-secondary)]">
        {label}
        <span className="ml-1 text-[var(--color-text-subtle)]">({field.type})</span>
      </span>
      <input
        type="text"
        value={value == null ? '' : String(value)}
        onChange={(event) => onChange(path, field, coerceInputValue(event.target.value, field))}
        className="w-full rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-0)] px-2 py-1 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-subtle)] focus:border-[var(--color-primary)] focus:outline-none"
      />
    </label>
  );
}

export function RequestPanel({
  selectedMethod,
  methodInputSchema,
  body,
  metadata,
  project,
  environments,
  envRefType,
  selectedEnvironmentId,
  onBodyChange,
  onMetadataChange,
  onEnvRefChange,
  onSend,
  isStreamConnected = false,
  isStreamInputClosed = false,
  onEndStream,
  onCloseStream,
  onSave,
  isLoading = false,
  className,
}: RequestPanelProps) {
  const { t } = useTranslation();
  const methodTypeLabels = useMethodTypeLabels();
  const [activeTab, setActiveTab] = useState<RequestTab>('body');
  const [bodyEditorMode, setBodyEditorMode] = useState<'schema' | 'json'>('json');
  const isStreamingMethod = Boolean(selectedMethod && selectedMethod.type !== 'unary');
  const canSendOnConnectedStream = Boolean(
    selectedMethod && selectedMethod.type !== 'unary' && selectedMethod.type !== 'server_stream'
  );

  const sendButtonLabel = useMemo(() => {
    if (isLoading) {
      return t('request.sending');
    }

    if (!selectedMethod || selectedMethod.type === 'unary') {
      return t('request.send');
    }

    if (!isStreamConnected) {
      return t('request.startStream');
    }

    if (selectedMethod.type === 'server_stream') {
      return t('request.serverStreamRunning');
    }

    return isStreamInputClosed
      ? t('request.streamInputClosedLabel')
      : t('request.sendStreamMessage');
  }, [isLoading, isStreamConnected, isStreamInputClosed, selectedMethod, t]);

  const sendButtonDisabled =
    !selectedMethod ||
    isLoading ||
    (selectedMethod?.type === 'server_stream' && isStreamConnected) ||
    (canSendOnConnectedStream && isStreamConnected && isStreamInputClosed);

  const hasSchemaEditor = Boolean(methodInputSchema && methodInputSchema.fields.length > 0);
  const schemaDefaultBody = useMemo(() => buildDefaultBodyFromSchema(methodInputSchema), [methodInputSchema]);
  const schemaBodyObject = useMemo(
    () => buildSchemaBodyFromRequest(methodInputSchema, body),
    [methodInputSchema, body]
  );

  useEffect(() => {
    if (hasSchemaEditor) {
      setBodyEditorMode('schema');
      return;
    }
    setBodyEditorMode('json');
  }, [hasSchemaEditor, selectedMethod?.service, selectedMethod?.method]);

  const handleAddMetadata = useCallback(() => {
    const newEntry: MetadataEntry = {
      id: crypto.randomUUID(),
      key: '',
      value: '',
      enabled: true,
    };
    onMetadataChange([...metadata, newEntry]);
  }, [metadata, onMetadataChange]);

  const handleUpdateMetadata = useCallback(
    (id: string, updates: Partial<MetadataEntry>) => {
      onMetadataChange(
        metadata.map((entry) =>
          entry.id === id ? { ...entry, ...updates } : entry
        )
      );
    },
    [metadata, onMetadataChange]
  );

  const handleRemoveMetadata = useCallback(
    (id: string) => {
      onMetadataChange(metadata.filter((entry) => entry.id !== id));
    },
    [metadata, onMetadataChange]
  );

  const formatJson = useCallback(() => {
    try {
      const parsed = JSON.parse(body);
      onBodyChange(JSON.stringify(parsed, null, 2));
    } catch {
      // Invalid JSON, ignore
    }
  }, [body, onBodyChange]);

  const handleSchemaFieldChange = useCallback(
    (path: string[], _field: MessageFieldSchema, value: unknown) => {
      const base = buildSchemaBodyFromRequest(methodInputSchema, body);
      const next = replacePathValue(base, path, value);
      onBodyChange(JSON.stringify(next, null, 2));
    },
    [body, methodInputSchema, onBodyChange]
  );

  // handleToggleBodyEditorMode 负责在 JSON 与结构化编辑之间切换。
  // 当 JSON 解析失败但用户仍切回结构化模式时，清空为 schema 默认值，
  // 避免保留不可解析文本导致表单展示与真实请求体不一致。
  const handleToggleBodyEditorMode = useCallback(() => {
    if (!hasSchemaEditor) {
      return;
    }

    setBodyEditorMode((currentMode) => {
      if (currentMode === 'json') {
        const parsed = getCurrentBodyObject(body);
        if (!parsed) {
          onBodyChange(JSON.stringify(schemaDefaultBody, null, 2));
        }
        return 'schema';
      }

      return 'json';
    });
  }, [body, hasSchemaEditor, onBodyChange, schemaDefaultBody]);

  return (
    <div className={cn('flex flex-col h-full bg-[var(--color-surface-0)]', className)}>
      <div className="flex items-center px-4 py-3 border-b border-[var(--color-surface-3)]">
        {selectedMethod ? (
          <>
            <span
              className={cn(
                'px-2 py-1 text-xs rounded mr-2 font-medium',
                methodTypeColors[selectedMethod.type]
              )}
            >
              {methodTypeLabels[selectedMethod.type]}
            </span>
            <span className="text-sm text-[var(--color-text-primary)] font-medium truncate">
              {selectedMethod.service}/{selectedMethod.method}
            </span>
          </>
        ) : (
          <span className="text-sm text-[var(--color-text-muted)]">{t('request.selectMethod')}</span>
        )}

        <div className="ml-auto flex gap-2">
          {onSave && (
            <button
              onClick={onSave}
              disabled={!selectedMethod}
              className="px-3 py-1.5 text-sm text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Save size={14} />
              {t('request.save')}
            </button>
          )}
          {isStreamingMethod && isStreamConnected && onEndStream && canSendOnConnectedStream && (
            <button
              onClick={onEndStream}
              disabled={isLoading || isStreamInputClosed}
              className="px-3 py-1.5 bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] disabled:cursor-not-allowed disabled:opacity-50 text-[var(--color-text-primary)] text-sm rounded transition-colors"
            >
              {t('request.endStream')}
            </button>
          )}
          {isStreamingMethod && isStreamConnected && onCloseStream && (
            <button
              onClick={onCloseStream}
              disabled={isLoading}
              className="px-3 py-1.5 bg-[var(--color-danger-soft)] hover:bg-[var(--color-danger-soft-hover)] disabled:cursor-not-allowed disabled:opacity-50 text-[var(--color-text-primary)] text-sm rounded transition-colors"
            >
              {t('request.closeStream')}
            </button>
          )}
          <button
            onClick={onSend}
            disabled={sendButtonDisabled}
            className="px-4 py-1.5 bg-[var(--color-primary)] hover:bg-[var(--color-primary-strong-hover)] disabled:bg-[var(--color-surface-3)] disabled:cursor-not-allowed text-[var(--color-text-primary)] text-sm rounded flex items-center gap-1.5 transition-colors"
          >
            <Send size={14} />
            {sendButtonLabel}
          </button>
        </div>
      </div>

      <div className="flex items-center px-4 border-b border-[var(--color-surface-3)] bg-[var(--color-surface-1)]">
        <TabButton
          label={t('request.body')}
          active={activeTab === 'body'}
          onClick={() => setActiveTab('body')}
        />
        <TabButton
          label={`${t('request.metadata')} (${metadata.filter((m) => m.enabled).length})`}
          active={activeTab === 'metadata'}
          onClick={() => setActiveTab('metadata')}
        />
        <TabButton
          label={t('request.environment')}
          active={activeTab === 'environment'}
          onClick={() => setActiveTab('environment')}
        />
      </div>

      <div className="flex-1 overflow-hidden">
        {activeTab === 'body' && (
          <div className="h-full flex flex-col">
            <div className="flex items-center justify-between px-4 py-2 border-b border-[var(--color-surface-3)]">
              <span className="text-xs text-[var(--color-text-muted)]">
                {bodyEditorMode === 'schema' ? t('request.structuredBody') : t('request.jsonBody')}
              </span>
              <div className="flex items-center gap-3">
                {hasSchemaEditor && (
                  <button
                    onClick={handleToggleBodyEditorMode}
                    className="text-xs text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)]"
                  >
                    {bodyEditorMode === 'schema' ? t('request.switchToJson') : t('request.switchToStructured')}
                  </button>
                )}
                <button
                  onClick={formatJson}
                  className="text-xs text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)]"
                >
                  {t('request.formatJson')}
                </button>
              </div>
            </div>
            <div className="flex-1 p-4">
              {bodyEditorMode === 'schema' && hasSchemaEditor ? (
                <div className="h-full rounded-lg overflow-auto border border-[var(--color-surface-3)] bg-[var(--color-surface-editor)] p-3 space-y-3">
                  {methodInputSchema?.fields.map((field) => (
                    <SchemaFieldEditor
                      key={field.jsonName}
                      field={field}
                      path={[field.jsonName]}
                      value={schemaBodyObject[field.jsonName]}
                      onChange={handleSchemaFieldChange}
                    />
                  ))}
                </div>
              ) : (
                <div className="h-full rounded-lg overflow-hidden border border-[var(--color-surface-3)]">
                  <textarea
                    value={body}
                    onChange={(e) => onBodyChange(e.target.value)}
                    className="w-full h-full bg-[var(--color-surface-editor)] p-4 text-sm font-mono text-[var(--color-text-secondary)] resize-none focus:outline-none"
                    placeholder={t('request.placeholder')}
                    spellCheck={false}
                  />
                </div>
              )}
            </div>
          </div>
        )}

        {activeTab === 'metadata' && (
          <div className="h-full flex flex-col p-4">
            <div className="flex items-center justify-between mb-3">
              <span className="text-xs text-[var(--color-text-muted)]">{t('request.requestMetadata')}</span>
              <button
                onClick={handleAddMetadata}
                className="text-xs flex items-center gap-1 text-[var(--color-primary)] hover:text-[var(--color-primary-strong-hover)]"
              >
                <Plus size={12} />
                {t('request.add')}
              </button>
            </div>
            <div className="flex-1 overflow-auto space-y-2">
              {metadata.length === 0 ? (
                <div className="text-center text-[var(--color-text-muted)] py-8 text-sm">
                  {t('request.noMetadata')}
                </div>
              ) : (
                metadata.map((entry) => (
                  <div
                    key={entry.id}
                    className="flex items-center gap-2 bg-[var(--color-surface-1)] p-2 rounded"
                  >
                    <input
                      type="checkbox"
                      checked={entry.enabled}
                      onChange={(e) =>
                        handleUpdateMetadata(entry.id, { enabled: e.target.checked })
                      }
                      className="rounded border-[var(--color-surface-3)]"
                    />
                    <input
                      type="text"
                      value={entry.key}
                      onChange={(e) =>
                        handleUpdateMetadata(entry.id, { key: e.target.value })
                      }
                      placeholder={t('request.keyPlaceholder')}
                      className="flex-1 bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-subtle)] focus:outline-none focus:border-[var(--color-primary)]"
                    />
                    <input
                      type="text"
                      value={entry.value}
                      onChange={(e) =>
                        handleUpdateMetadata(entry.id, { value: e.target.value })
                      }
                      placeholder={t('request.valuePlaceholder')}
                      className="flex-1 bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-subtle)] focus:outline-none focus:border-[var(--color-primary)]"
                    />
                    <button
                      onClick={() => handleRemoveMetadata(entry.id)}
                      className="p-1 text-[var(--color-text-muted)] hover:text-red-400"
                    >
                      <X size={14} />
                    </button>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {activeTab === 'environment' && (
          <div className="p-4 h-full overflow-auto">
            <EnvironmentSelector
              project={project}
              environments={environments}
              envRefType={envRefType}
              selectedEnvironmentId={selectedEnvironmentId}
              onEnvRefChange={onEnvRefChange}
            />
          </div>
        )}
      </div>
    </div>
  );
}

interface TabButtonProps {
  label: string;
  active: boolean;
  onClick: () => void;
}

function TabButton({ label, active, onClick }: TabButtonProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'px-4 py-2 text-sm font-medium transition-colors relative',
        active ? 'text-[var(--color-text-primary)]' : 'text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]'
      )}
    >
      {label}
      {active && (
        <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-[var(--color-primary)]" />
      )}
    </button>
  );
}

export default RequestPanel;
