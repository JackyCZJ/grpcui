import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Copy, Trash2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { Project } from '../../types/project';

interface ProjectSelectorProps {
  projects: Project[];
  currentProjectId?: string;
  isLoading?: boolean;
  onSelect: (projectId: string) => void;
  onCreate: () => void;
  onClone: () => void;
  onDelete: () => void;
  className?: string;
}

export function ProjectSelector({
  projects,
  currentProjectId,
  isLoading = false,
  onSelect,
  onCreate,
  onClone,
  onDelete,
  className,
}: ProjectSelectorProps) {
  const { t } = useTranslation();
  const currentProject = useMemo(
    () => projects.find((project) => project.id === currentProjectId),
    [projects, currentProjectId]
  );

  return (
    <div className={cn('px-3 py-3 border-b border-[var(--color-surface-3)] space-y-2', className)}>
      <div className="text-xs text-[var(--color-text-muted)] uppercase tracking-wider">{t('project.current')}</div>
      <div className="flex items-center gap-2">
        <select
          value={currentProjectId ?? ''}
          onChange={(event) => onSelect(event.target.value)}
          className="flex-1 bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
          disabled={isLoading || projects.length === 0}
        >
          {projects.length === 0 ? (
            <option value="">{t('project.noProjects')}</option>
          ) : (
            projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))
          )}
        </select>
        <button
          onClick={onCreate}
          className="p-2 rounded bg-[var(--color-surface-3)] text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-surface-4)] transition-colors"
          title={t('project.create')}
        >
          <Plus size={14} />
        </button>
      </div>

      <div className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
        <button
          onClick={onClone}
          disabled={!currentProject}
          className="flex-1 flex items-center justify-center gap-1 py-1 rounded bg-[var(--color-subtle-button)] hover:bg-[var(--color-subtle-button-hover)] disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Copy size={12} />
          {t('project.clone')}
        </button>
        <button
          onClick={onDelete}
          disabled={!currentProject || projects.length <= 1}
          className="flex-1 flex items-center justify-center gap-1 py-1 rounded bg-[var(--color-danger-button)] text-[var(--color-danger-text)] hover:bg-[var(--color-danger-button-hover)] disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Trash2 size={12} />
          {t('project.delete')}
        </button>
      </div>

      {currentProject && (
        <p className="text-[11px] text-[var(--color-text-muted)] line-clamp-2">
          {currentProject.description || t('project.noDescription')}
        </p>
      )}
    </div>
  );
}

export default ProjectSelector;
