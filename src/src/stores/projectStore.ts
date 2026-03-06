import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';
import type {
  Project,
  CreateProjectData,
  UpdateProjectData,
  ProjectEnvironment,
  ProjectCollection,
} from '../types/project';
import { tauriApi } from '../lib/tauriApi';

interface ProjectState {
  // State
  projects: Project[];
  currentProject: Project | null;
  environments: ProjectEnvironment[];
  collections: ProjectCollection[];
  activeEnvironmentId: string | null;
  isLoading: boolean;
  error: string | null;
}

interface ProjectActions {
  // Actions
  loadProjects: () => Promise<void>;
  selectProject: (id: string) => Promise<void>;
  createProject: (data: CreateProjectData) => Promise<Project>;
  updateProject: (id: string, data: UpdateProjectData) => Promise<void>;
  deleteProject: (id: string) => Promise<void>;
  cloneProject: (id: string, newName: string) => Promise<Project>;
  loadProjectEnvironments: (projectId: string) => Promise<void>;
  createEnvironment: (projectId: string, data: Omit<ProjectEnvironment, 'id' | 'projectId' | 'createdAt' | 'updatedAt'>) => Promise<ProjectEnvironment>;
  updateEnvironment: (environment: ProjectEnvironment) => Promise<ProjectEnvironment>;
  deleteEnvironment: (projectId: string, envId: string) => Promise<void>;
  loadProjectCollections: (projectId: string) => Promise<void>;
  setDefaultEnvironment: (projectId: string, envId: string) => Promise<void>;
  setActiveEnvironment: (envId: string | null) => void;
  clearError: () => void;
}

type ProjectStore = ProjectState & ProjectActions;

const initialState: ProjectState = {
  projects: [],
  currentProject: null,
  environments: [],
  collections: [],
  activeEnvironmentId: null,
  isLoading: false,
  error: null,
};

// resolveStoreErrorMessage 统一解析 tauri / JS 抛出的错误类型，
// 避免字符串错误对象被误判后只显示笼统文案，帮助用户定位真实失败原因。
function resolveStoreErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  if (typeof error === 'string' && error.trim()) {
    return error;
  }

  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === 'string' && message.trim()) {
      return message;
    }
  }

  return fallback;
}

export const useProjectStore = create<ProjectStore>()(
  devtools(
    persist(
      (set, get) => ({
        ...initialState,

        loadProjects: async () => {
          set({ isLoading: true, error: null });
          try {
            const projects = await tauriApi.getProjects();
            set({ projects, isLoading: false });
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to load projects'),
              isLoading: false,
            });
          }
        },

        // selectProject 会先清空上一个项目的环境/集合快照，
        // 防止异步切换期间出现“项目 A 的环境按钮去操作项目 B”的错配。
        selectProject: async (id: string) => {
          const { projects, loadProjectEnvironments, loadProjectCollections } = get();
          const project = projects.find((p) => p.id === id) || null;

          set({
            currentProject: project,
            environments: [],
            collections: [],
            activeEnvironmentId: null,
            error: null,
          });

          if (!project) {
            return;
          }

          await Promise.all([
            loadProjectEnvironments(id),
            loadProjectCollections(id),
          ]);

          // 项目切换是异步的，等待加载完成后再次确认当前项目未变化，
          // 避免快速切换时把旧项目的默认环境写回到新项目状态。
          const latestProject = get().currentProject;
          if (latestProject?.id !== id) {
            return;
          }

          if (project.defaultEnvironmentId) {
            const hasDefaultEnvironment = get()
              .environments
              .some((env) => env.id === project.defaultEnvironmentId);
            set({
              activeEnvironmentId: hasDefaultEnvironment ? project.defaultEnvironmentId : null,
            });
          }
        },

        createProject: async (data: CreateProjectData) => {
          set({ isLoading: true, error: null });
          try {
            const project = await tauriApi.createProject({
              id: crypto.randomUUID(),
              name: data.name,
              description: data.description || '',
              defaultEnvironmentId: data.defaultEnvironmentId,
              // 不发送 createdAt 和 updatedAt，让后端自动生成
            });
            set((state) => ({
              projects: [...state.projects, project],
              isLoading: false,
            }));
            return project;
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to create project'),
              isLoading: false,
            });
            throw err;
          }
        },

        updateProject: async (id: string, data: UpdateProjectData) => {
          set({ isLoading: true, error: null });
          try {
            const { projects, currentProject } = get();
            const existingProject = projects.find((p) => p.id === id);
            if (!existingProject) {
              throw new Error('Project not found');
            }

            const updatedProject: Project = {
              ...existingProject,
              ...data,
              updatedAt: new Date().toISOString(),
            };

            await tauriApi.updateProject(updatedProject);

            set((state) => ({
              projects: state.projects.map((p) => (p.id === id ? updatedProject : p)),
              currentProject: currentProject?.id === id ? updatedProject : currentProject,
              isLoading: false,
            }));
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to update project'),
              isLoading: false,
            });
            throw err;
          }
        },

        deleteProject: async (id: string) => {
          set({ isLoading: true, error: null });
          try {
            await tauriApi.deleteProject(id);

            const { currentProject } = get();
            set((state) => ({
              projects: state.projects.filter((p) => p.id !== id),
              currentProject: currentProject?.id === id ? null : currentProject,
              environments: currentProject?.id === id ? [] : state.environments,
              collections: currentProject?.id === id ? [] : state.collections,
              isLoading: false,
            }));
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to delete project'),
              isLoading: false,
            });
            throw err;
          }
        },

        cloneProject: async (id: string, newName: string) => {
          set({ isLoading: true, error: null });
          try {
            const clonedProject = await tauriApi.cloneProject(id, newName);
            set((state) => ({
              projects: [...state.projects, clonedProject],
              isLoading: false,
            }));
            return clonedProject;
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to clone project'),
              isLoading: false,
            });
            throw err;
          }
        },

        loadProjectEnvironments: async (projectId: string) => {
          try {
            const environments = await tauriApi.getEnvironmentsByProject(projectId);
            set({ environments });
          } catch (err) {
            console.error('Failed to load environments:', err);
            set({ environments: [] });
          }
        },

        createEnvironment: async (projectId: string, data: Omit<ProjectEnvironment, 'id' | 'projectId' | 'createdAt' | 'updatedAt'>) => {
          set({ isLoading: true, error: null });
          try {
            const payload: ProjectEnvironment = {
              id: crypto.randomUUID(),
              projectId,
              name: data.name,
              baseUrl: data.baseUrl,
              tls: data.tls ?? { mode: 'insecure' },
              metadata: data.metadata ?? {},
              variables: data.variables ?? [],
              isDefault: data.isDefault ?? false,
              // 不发送 createdAt 和 updatedAt，让后端自动生成
            };
            await tauriApi.saveEnvironment(payload);

            // 创建完成后强制从后端刷新环境列表，
            // 统一使用后端实际生成的环境 ID，避免前端临时 ID 与数据库不一致。
            await get().loadProjectEnvironments(projectId);
            const { environments: refreshedEnvironments, currentProject } = get();

            const createdEnv =
              refreshedEnvironments.find(
                (env) =>
                  env.name === payload.name &&
                  env.baseUrl === payload.baseUrl
              ) ?? refreshedEnvironments[refreshedEnvironments.length - 1];

            if (!createdEnv) {
              throw new Error('Failed to load created environment');
            }

            // setDefaultEnvironment 会同步更新 project.defaultEnvironmentId，
            // 保证“新建后默认环境”状态与后端保持一致。
            if (currentProject && (refreshedEnvironments.length === 1 || payload.isDefault)) {
              await get().setDefaultEnvironment(projectId, createdEnv.id);
            } else {
              set({ isLoading: false });
            }

            return createdEnv;
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to create environment'),
              isLoading: false,
            });
            throw err;
          }
        },

        // updateEnvironment 负责更新已有环境配置并回读后端结果，
        // 防止前端本地状态与持久化状态出现偏差。
        updateEnvironment: async (environment: ProjectEnvironment) => {
          set({ isLoading: true, error: null });
          try {
            await tauriApi.saveEnvironment(environment);
            await get().loadProjectEnvironments(environment.projectId);

            const updatedEnvironment = get().environments.find((env) => env.id === environment.id);
            if (!updatedEnvironment) {
              throw new Error('Failed to reload updated environment');
            }

            set({ isLoading: false });
            return updatedEnvironment;
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to update environment'),
              isLoading: false,
            });
            throw err;
          }
        },

        // deleteEnvironment 负责删除环境并同步项目默认环境与当前激活环境，
        // 避免删除后界面仍指向已经不存在的环境 ID。
        deleteEnvironment: async (projectId: string, envId: string) => {
          set({ isLoading: true, error: null });
          try {
            await tauriApi.deleteEnvironment(envId);

            const { environments, projects, currentProject, activeEnvironmentId } = get();
            const nextEnvironments = environments.filter((env) => env.id !== envId);
            const nextProjects = projects.map((project) =>
              project.id === projectId && project.defaultEnvironmentId === envId
                ? { ...project, defaultEnvironmentId: undefined }
                : project
            );
            const nextCurrentProject =
              currentProject?.id === projectId && currentProject.defaultEnvironmentId === envId
                ? { ...currentProject, defaultEnvironmentId: undefined }
                : currentProject;
            const nextActiveEnvironmentId =
              activeEnvironmentId === envId
                ? nextEnvironments.find((env) => env.isDefault)?.id ?? nextEnvironments[0]?.id ?? null
                : activeEnvironmentId;

            set({
              environments: nextEnvironments,
              projects: nextProjects,
              currentProject: nextCurrentProject,
              activeEnvironmentId: nextActiveEnvironmentId,
              isLoading: false,
            });
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to delete environment'),
              isLoading: false,
            });
            throw err;
          }
        },

        loadProjectCollections: async (projectId: string) => {
          try {
            const collections = await tauriApi.getCollectionsByProject(projectId);
            set({ collections });
          } catch (err) {
            console.error('Failed to load collections:', err);
            set({ collections: [] });
          }
        },

        // setDefaultEnvironment 会优先使用环境自身携带的 projectId，
        // 规避前端异步切换项目时 projectId 过期导致的“Environment not found in project”。
        setDefaultEnvironment: async (projectId: string, envId: string) => {
          set({ isLoading: true, error: null });
          try {
            const { environments } = get();
            const targetEnvironment = environments.find((env) => env.id === envId);
            const resolvedProjectId = targetEnvironment?.projectId || projectId;

            await tauriApi.setDefaultEnvironment(resolvedProjectId, envId);

            const { projects, currentProject } = get();
            const updatedProjects = projects.map((p) =>
              p.id === resolvedProjectId ? { ...p, defaultEnvironmentId: envId } : p
            );
            const updatedCurrentProject =
              currentProject?.id === resolvedProjectId
                ? { ...currentProject, defaultEnvironmentId: envId }
                : currentProject;
            const updatedEnvironments = environments.map((env) =>
              env.projectId === resolvedProjectId
                ? { ...env, isDefault: env.id === envId }
                : env
            );

            set({
              projects: updatedProjects,
              currentProject: updatedCurrentProject,
              environments: updatedEnvironments,
              isLoading: false,
            });
          } catch (err) {
            set({
              error: resolveStoreErrorMessage(err, 'Failed to set default environment'),
              isLoading: false,
            });
            throw err;
          }
        },

        setActiveEnvironment: (envId: string | null) => {
          set({ activeEnvironmentId: envId });
        },

        clearError: () => {
          set({ error: null });
        },
      }),
      {
        name: 'grpcui-project',
        partialize: (state) => ({
          projects: state.projects,
          currentProject: state.currentProject,
          activeEnvironmentId: state.activeEnvironmentId,
        }),
      }
    ),
    { name: 'ProjectStore' }
  )
);
