import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Globe, Upload, Link2, Unlink, Loader2 } from 'lucide-react';
import { cn } from '../lib/utils';
import type { ConnectionState, Environment } from '../types';

interface ConnectionBarProps {
  address: string;
  connectionState: ConnectionState;
  environments: Environment[];
  selectedEnvironmentId?: string;
  showConnectionAction?: boolean;
  onAddressChange: (address: string) => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onOpenImportDialog: () => void;
  onEnvironmentChange: (environmentId: string) => void;
  error?: string;
  className?: string;
}

function useConnectionStateConfig() {
  const { t } = useTranslation();
  return {
    disconnected: {
      label: t('connection.connect'),
      icon: <Link2 size={14} />,
      color: 'bg-[var(--color-connect)] hover:bg-[var(--color-connect-hover)]',
    },
    connecting: {
      label: t('connection.connecting'),
      icon: <Loader2 size={14} className="animate-spin" />,
      color: 'bg-[var(--color-connect)] cursor-not-allowed',
    },
    connected: {
      label: t('connection.disconnect'),
      icon: <Unlink size={14} />,
      color: 'bg-green-600 hover:bg-green-700',
    },
    error: {
      label: t('connection.retry'),
      icon: <Link2 size={14} />,
      color: 'bg-red-600 hover:bg-red-700',
    },
  } as Record<ConnectionState, { label: string; icon: React.ReactNode; color: string }>;
}

export function ConnectionBar({
  address,
  connectionState,
  environments,
  selectedEnvironmentId,
  showConnectionAction = true,
  onAddressChange,
  onConnect,
  onDisconnect,
  onOpenImportDialog,
  onEnvironmentChange,
  error,
  className,
}: ConnectionBarProps) {
  const { t } = useTranslation();
  const [isFocused, setIsFocused] = useState(false);
  const isAddressLocked = showConnectionAction && connectionState === 'connected';

  const handleConnectionClick = useCallback(() => {
    if (connectionState === 'connected') {
      onDisconnect();
    } else if (connectionState !== 'connecting') {
      onConnect();
    }
  }, [connectionState, onConnect, onDisconnect]);

  const connectionStateConfig = useConnectionStateConfig();
  const stateConfig = connectionStateConfig[connectionState];

  return (
    <div
      className={cn(
        'relative flex items-center h-12 px-4 border-b border-[var(--color-surface-3)] bg-[var(--color-surface-1)] gap-3',
        className
      )}
    >
      {/* Environment Selector */}
      <div className="relative">
        <select
          value={selectedEnvironmentId || ''}
          onChange={(e) => onEnvironmentChange(e.target.value)}
          className="appearance-none bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-3 py-1.5 pr-8 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)] cursor-pointer"
        >
          <option value="">{t('connection.noEnvironment')}</option>
          {environments.map((env) => (
            <option key={env.id} value={env.id}>
              {env.name}
            </option>
          ))}
        </select>
        <Globe
          size={14}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--color-text-muted)] pointer-events-none"
        />
      </div>

      {/* Address Input */}
      <div className="flex-1 relative">
        <input
          type="text"
          value={address}
          onChange={(e) => onAddressChange(e.target.value)}
          onFocus={() => setIsFocused(true)}
          onBlur={() => setIsFocused(false)}
          placeholder={t('connection.addressPlaceholder')}
          disabled={isAddressLocked}
          className={cn(
            'w-full bg-[var(--color-surface-0)] border rounded px-3 py-1.5 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-subtle)] focus:outline-none transition-colors',
            isFocused ? 'border-[var(--color-primary)]' : 'border-[var(--color-surface-3)]',
            isAddressLocked && 'opacity-50 cursor-not-allowed'
          )}
        />
        {isAddressLocked && (
          <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
            <span className="text-xs text-green-400">{t('connection.connected')}</span>
          </div>
        )}
      </div>

      {showConnectionAction && (
        <button
          onClick={handleConnectionClick}
          disabled={connectionState === 'connecting'}
          className={cn(
            'px-3 py-1.5 text-[var(--color-text-primary)] text-sm rounded flex items-center gap-1.5 transition-colors shrink-0',
            stateConfig.color
          )}
        >
          {stateConfig.icon}
          {stateConfig.label}
        </button>
      )}

      {/* Import Proto Entry Button */}
      <button
        onClick={onOpenImportDialog}
        className="px-3 py-1.5 bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-[var(--color-text-primary)] text-sm rounded flex items-center gap-1.5 transition-colors shrink-0"
      >
        <Upload size={14} />
        {t('connection.importProto')}
      </button>

      {/* Error Display */}
      {error && (
        <div className="absolute left-0 right-0 top-full mt-1 mx-4 px-3 py-2 bg-red-900/50 border border-red-700 rounded text-xs text-[var(--color-danger-text)]">
          {error}
        </div>
      )}
    </div>
  );
}

export default ConnectionBar;
