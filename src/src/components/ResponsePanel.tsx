import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle, XCircle, Clock, AlertCircle } from 'lucide-react';
import { cn, formatDuration } from '../lib/utils';
import type { Response, ResponseStatus, StreamMessage } from '../types';

interface ResponsePanelProps {
  response?: Response;
  streamMessages?: StreamMessage[];
  isStreaming?: boolean;
  className?: string;
}

type ResponseTab = 'body' | 'metadata' | 'trailers';

const statusIcons: Record<ResponseStatus, React.ReactNode> = {
  pending: <Clock size={12} className="text-yellow-400" />,
  success: <CheckCircle size={12} className="text-green-400" />,
  error: <XCircle size={12} className="text-red-400" />,
  streaming: <Clock size={12} className="text-blue-400 animate-pulse" />,
};

function useStatusLabels(): Record<ResponseStatus, string> {
  const { t } = useTranslation();
  return {
    pending: t('response.pending'),
    success: t('response.success'),
    error: t('response.error'),
    streaming: t('response.streaming'),
  };
}

const statusColors: Record<ResponseStatus, string> = {
  pending: 'text-yellow-400',
  success: 'text-green-400',
  error: 'text-red-400',
  streaming: 'text-blue-400',
};

export function ResponsePanel({
  response,
  streamMessages = [],
  isStreaming = false,
  className,
}: ResponsePanelProps) {
  const { t } = useTranslation();
  const statusLabels = useStatusLabels();
  const [activeTab, setActiveTab] = useState<ResponseTab>('body');

  const formattedBody = useMemo(() => {
    if (!response?.body) return '';
    try {
      const parsed = JSON.parse(response.body);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return response.body;
    }
  }, [response?.body]);

  const formattedStreamMessages = useMemo(() => {
    return streamMessages.map((msg) => {
      if (typeof msg.payload === 'string') {
        return msg.payload;
      }
      try {
        return JSON.stringify(msg.payload, null, 2);
      } catch {
        return String(msg.payload);
      }
    });
  }, [streamMessages]);

  if (!response) {
    return (
      <div className={cn('flex flex-col h-full bg-[var(--color-surface-0)]', className)}>
        <div className="flex items-center px-4 py-3 border-b border-[var(--color-surface-3)]">
          <span className="text-sm text-[var(--color-text-primary)] font-medium">{t('response.title')}</span>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center text-[var(--color-text-muted)] p-8">
          <div className="text-sm">{t('response.noResponse')}</div>
          <div className="text-xs mt-2">{t('response.sendToSee')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col h-full bg-[var(--color-surface-0)]', className)}>
      {/* Response Header */}
      <div className="flex items-center px-4 py-3 border-b border-[var(--color-surface-3)]">
        <span className="text-sm text-[var(--color-text-primary)] font-medium">{t('response.title')}</span>
        <div className="ml-auto flex items-center gap-3 text-xs">
          <span className={cn('flex items-center gap-1', statusColors[response.status])}>
            {statusIcons[response.status]}
            {response.statusCode || statusLabels[response.status]}
          </span>
          {response.duration > 0 && (
            <span className="text-[var(--color-text-muted)]">{formatDuration(response.duration)}</span>
          )}
          {response.body && (
            <span className="text-[var(--color-text-muted)]">
              {(response.body.length / 1024).toFixed(2)}KB
            </span>
          )}
        </div>
      </div>

      {/* Response Tabs */}
      <div className="flex items-center px-4 border-b border-[var(--color-surface-3)] bg-[var(--color-surface-1)]">
        <TabButton
          label={t('request.body')}
          active={activeTab === 'body'}
          onClick={() => setActiveTab('body')}
        />
        <TabButton
          label={`${t('request.metadata')} (${Object.keys(response.metadata).length})`}
          active={activeTab === 'metadata'}
          onClick={() => setActiveTab('metadata')}
        />
        <TabButton
          label={`Trailers (${Object.keys(response.trailers).length})`}
          active={activeTab === 'trailers'}
          onClick={() => setActiveTab('trailers')}
        />
      </div>

      {/* Response Content */}
      <div className="flex-1 overflow-auto">
        {activeTab === 'body' && (
          <div className="h-full">
            {response.error ? (
              <div className="p-4">
                <div className="flex items-start gap-2 text-red-400 bg-red-500/10 p-4 rounded">
                  <AlertCircle size={16} className="mt-0.5 flex-shrink-0" />
                  <div>
                    <div className="font-medium">{t('response.error')}</div>
                    <div className="text-sm mt-1 font-mono whitespace-pre-wrap">
                      {response.error}
                    </div>
                  </div>
                </div>
              </div>
            ) : isStreaming || streamMessages.length > 0 ? (
              <div className="p-4 space-y-3">
                {isStreaming && (
                  <div className="text-xs text-blue-400 animate-pulse">
                    {t('response.streamingInProgress')}
                  </div>
                )}
                {formattedStreamMessages.map((msg, index) => (
                  <div
                    key={index}
                    className="bg-[var(--color-surface-1)] rounded p-3 border border-[var(--color-surface-3)]"
                  >
                    <div className="text-xs text-[var(--color-text-muted)] mb-1">
                      {t('response.message')} {index + 1}
                    </div>
                    <pre className="text-sm font-mono text-[var(--color-text-secondary)] overflow-x-auto">
                      {msg}
                    </pre>
                  </div>
                ))}
                {response.body && !streamMessages.length && (
                  <pre className="text-sm font-mono text-[var(--color-text-secondary)]">
                    {formattedBody}
                  </pre>
                )}
              </div>
            ) : (
              <div className="p-4">
                <pre className="text-sm font-mono text-[var(--color-text-secondary)] overflow-x-auto">
                  {formattedBody || t('response.noResponseBody')}
                </pre>
              </div>
            )}
          </div>
        )}

        {activeTab === 'metadata' && (
          <div className="p-4">
            {Object.keys(response.metadata).length === 0 ? (
              <div className="text-center text-[var(--color-text-muted)] py-8 text-sm">
                {t('response.noMetadata')}
              </div>
            ) : (
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--color-surface-3)]">
                    <th className="text-left text-[var(--color-text-muted)] py-2 px-3 font-medium">
                      {t('response.key')}
                    </th>
                    <th className="text-left text-[var(--color-text-muted)] py-2 px-3 font-medium">
                      {t('response.value')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {Object.entries(response.metadata).map(([key, value]) => (
                    <tr key={key} className="border-b border-[var(--color-surface-soft)]">
                      <td className="py-2 px-3 text-[var(--color-primary)] font-mono">{key}</td>
                      <td className="py-2 px-3 text-[var(--color-text-secondary)] font-mono">{value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {activeTab === 'trailers' && (
          <div className="p-4">
            {Object.keys(response.trailers).length === 0 ? (
              <div className="text-center text-[var(--color-text-muted)] py-8 text-sm">
                {t('response.noTrailers')}
              </div>
            ) : (
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--color-surface-3)]">
                    <th className="text-left text-[var(--color-text-muted)] py-2 px-3 font-medium">
                      {t('response.key')}
                    </th>
                    <th className="text-left text-[var(--color-text-muted)] py-2 px-3 font-medium">
                      {t('response.value')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {Object.entries(response.trailers).map(([key, value]) => (
                    <tr key={key} className="border-b border-[var(--color-surface-soft)]">
                      <td className="py-2 px-3 text-[var(--color-primary)] font-mono">{key}</td>
                      <td className="py-2 px-3 text-[var(--color-text-secondary)] font-mono">{value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
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

export default ResponsePanel;
