import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import type {
  Project,
  ProjectEnvironment,
  EnvRefType,
} from '../../types/project';

interface EnvironmentSelectorProps {
  project: Project | null;
  environments: ProjectEnvironment[];
  envRefType: EnvRefType;
  selectedEnvironmentId?: string;
  onEnvRefChange: (type: EnvRefType, envId?: string) => void;
  className?: string;
}

export function EnvironmentSelector({
  project,
  environments,
  envRefType,
  selectedEnvironmentId,
  onEnvRefChange,
  className,
}: EnvironmentSelectorProps) {
  const { t } = useTranslation();
  const [showPreview, setShowPreview] = useState(false);

  const defaultEnvironment = useMemo(() => {
    if (!project?.defaultEnvironmentId) return null;
    return environments.find((e) => e.id === project.defaultEnvironmentId) || null;
  }, [project, environments]);

  const selectedEnvironment = useMemo(() => {
    if (envRefType !== 'specific' || !selectedEnvironmentId) return null;
    return environments.find((e) => e.id === selectedEnvironmentId) || null;
  }, [envRefType, selectedEnvironmentId, environments]);

  const resolvedEnvironment = useMemo(() => {
    switch (envRefType) {
      case 'inherit':
        return defaultEnvironment;
      case 'specific':
        return selectedEnvironment;
      case 'none':
      default:
        return null;
    }
  }, [envRefType, defaultEnvironment, selectedEnvironment]);

  // handleTypeChange 在切换到 specific 时会自动预选第一个环境，
  // 这样用户无需多一步点击即可立即完成环境绑定。
  const handleTypeChange = (type: EnvRefType) => {
    if (type === 'specific' && environments.length > 0) {
      const firstEnv = environments[0];
      onEnvRefChange(type, firstEnv.id);
    } else {
      onEnvRefChange(type);
    }
  };

  const handleEnvChange = (envId: string) => {
    onEnvRefChange('specific', envId);
  };

  const radioBaseClass =
    'w-4 h-4 rounded-full border-2 border-[var(--color-surface-3)] bg-[var(--color-surface-0)] checked:bg-[var(--color-primary)] checked:border-[var(--color-primary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-primary-soft)] cursor-pointer';

  return (
    <div className={cn('bg-[var(--color-surface-1)] rounded border border-[var(--color-surface-3)]', className)}>
      <div className="px-4 py-3 border-b border-[var(--color-surface-3)]">
        <h3 className="text-sm font-medium text-[var(--color-text-primary)]">{t('environment.referencePolicy')}</h3>
      </div>

      <div className="p-4 space-y-4">
        <label className="flex items-start gap-3 cursor-pointer group">
          <input
            type="radio"
            name="envRefType"
            value="inherit"
            checked={envRefType === 'inherit'}
            onChange={() => handleTypeChange('inherit')}
            className={radioBaseClass}
          />
          <div className="flex-1">
            <span className="text-sm text-[var(--color-text-primary)] group-hover:text-[var(--color-text-secondary)]">{t('environment.inherit')}</span>
            <p className="text-xs text-[var(--color-text-muted)] mt-0.5">
              {defaultEnvironment
                ? t('environment.inheritDescription', { name: defaultEnvironment.name })
                : t('environment.noDefault')}
            </p>
          </div>
        </label>

        <label className="flex items-start gap-3 cursor-pointer group">
          <input
            type="radio"
            name="envRefType"
            value="specific"
            checked={envRefType === 'specific'}
            onChange={() => handleTypeChange('specific')}
            className={radioBaseClass}
          />
          <div className="flex-1">
            <span className="text-sm text-[var(--color-text-primary)] group-hover:text-[var(--color-text-secondary)]">{t('environment.specific')}</span>
            {envRefType === 'specific' && (
              <select
                value={selectedEnvironmentId || ''}
                onChange={(e) => handleEnvChange(e.target.value)}
                className="mt-2 w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-3 py-1.5 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
              >
                {environments.length === 0 ? (
                  <option value="">{t('environment.noEnvironments')}</option>
                ) : (
                  environments.map((env) => (
                    <option key={env.id} value={env.id}>
                      {env.name}
                    </option>
                  ))
                )}
              </select>
            )}
          </div>
        </label>

        <label className="flex items-start gap-3 cursor-pointer group">
          <input
            type="radio"
            name="envRefType"
            value="none"
            checked={envRefType === 'none'}
            onChange={() => handleTypeChange('none')}
            className={radioBaseClass}
          />
          <div className="flex-1">
            <span className="text-sm text-[var(--color-text-primary)] group-hover:text-[var(--color-text-secondary)]">{t('environment.none')}</span>
            <p className="text-xs text-[var(--color-text-muted)] mt-0.5">{t('environment.noneDescription')}</p>
          </div>
        </label>
      </div>

      {resolvedEnvironment && (
        <div className="border-t border-[var(--color-surface-3)]">
          <button
            onClick={() => setShowPreview(!showPreview)}
            className="w-full px-4 py-2 flex items-center justify-between text-xs text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-surface-editor)] transition-colors"
          >
            <span>{t('environment.preview')}</span>
            <span>{showPreview ? '▼' : '▶'}</span>
          </button>

          {showPreview && (
            <div className="px-4 py-3 bg-[var(--color-surface-0)] text-xs space-y-2">
              <div className="flex">
                <span className="text-[var(--color-text-muted)] w-20 shrink-0">{t('environment.name')}：</span>
                <span className="text-[var(--color-text-primary)]">{resolvedEnvironment.name}</span>
              </div>
              <div className="flex">
                <span className="text-[var(--color-text-muted)] w-20 shrink-0">{t('environment.address')}：</span>
                <span className="text-[var(--color-text-primary)] font-mono">{resolvedEnvironment.baseUrl || '-'}</span>
              </div>
              <div className="flex">
                <span className="text-[var(--color-text-muted)] w-20 shrink-0">{t('environment.variables')}：</span>
                <span className="text-[var(--color-text-primary)]">{resolvedEnvironment.variables.length}</span>
              </div>
              <div className="flex">
                <span className="text-[var(--color-text-muted)] w-20 shrink-0">{t('environment.headers')}：</span>
                <span className="text-[var(--color-text-primary)]">
                  {Object.keys(resolvedEnvironment.metadata).length}
                </span>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default EnvironmentSelector;
