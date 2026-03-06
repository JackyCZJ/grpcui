import { useState, useCallback, useEffect, useMemo, useRef } from 'react';
import { cn } from './lib/utils';
import { useTranslation } from 'react-i18next';
import {
  FolderOpen,
  History,
  Settings,
  Globe,
  ChevronRight,
  ChevronDown,
  Plus,
  Cog,
  Upload,
  FileCode2,
} from 'lucide-react';
import { confirm as confirmDialog, open } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ServiceTree } from './components/ServiceTree';
import { RequestPanel } from './components/RequestPanel';
import { ResponsePanel } from './components/ResponsePanel';
import { ConnectionBar } from './components/ConnectionBar';
import { ProjectSelector } from './components/project/ProjectSelector';
import { tauriApi } from './lib/tauriApi';
import {
  normalizeProtoDialogSelection,
  resolveAddressForEnvironmentSelection,
  resolveConnectionAddress,
} from './lib/connectionAddress';
import {
  buildImportPathCandidates,
  decodeProjectProtoFilesForConnect,
  encodeProjectProtoFilesForFolderImport,
  isProtoFilePath,
  removeProjectProtoFilesBySourcePaths,
  resolveProtoImportDisplayName,
} from './lib/protoImport';
import {
  buildTlsConfigFromDraft,
  buildTlsDraftFromConfig,
  validateTlsDraft,
  type TlsDraft,
} from './lib/environmentTls';
import { buildDefaultBodyFromSchema } from './lib/requestSchema';
import { useGrpcStream } from './hooks/useGrpcStream';
import { useProjectStore } from './stores/projectStore';
import {
  applyThemeMode,
  getStoredThemeMode,
  subscribeSystemThemeChange,
  type ThemeMode,
} from './lib/theme';
import type {
  Service,
  Method,
  MethodInputSchema,
  ConnectionState,
  MetadataEntry,
  Response,
  StreamMessage,
  History as HistoryItem,
  RequestItem,
} from './types';
import type {
  EnvRefType,
  ProjectEnvironment,
  RequestItem as ProjectRequestItem,
  TLSConfig as ProjectTlsConfig,
  Variable,
} from './types/project';

type Tab = 'services' | 'collections' | 'environments' | 'history';

interface EnvironmentFormData {
  name: string;
  baseUrl: string;
  variables: Variable[];
  metadata: Record<string, string>;
  tls: ProjectTlsConfig;
}

type ProtoImportSourceType = 'file' | 'directory';

interface ProtoImportPreview {
  sourceType: ProtoImportSourceType;
  sourcePath: string;
  rootDir: string | null;
  absolutePaths: string[];
  relativePaths: string[];
}

function App() {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<Tab>('services');
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // Connection state
  const [address, setAddress] = useState('localhost:50051');
  const [connectionState, setConnectionState] =
    useState<ConnectionState>('disconnected');

  // Service selection
  const [services, setServices] = useState<Service[]>([]);
  const [protoGroupRoot, setProtoGroupRoot] = useState<string | null>(null);
  const [selectedMethod, setSelectedMethod] = useState<
    { service: string; method: string; type: Method['type'] } | undefined
  >();
  const [methodInputSchema, setMethodInputSchema] = useState<MethodInputSchema | null>(null);

  // Request state
  const [requestBody, setRequestBody] = useState('{}');
  const [metadata, setMetadata] = useState<MetadataEntry[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [envRefType, setEnvRefType] = useState<EnvRefType>('inherit');
  const [requestEnvironmentId, setRequestEnvironmentId] = useState<string | undefined>();

  // Response state
  const [response, setResponse] = useState<Response | undefined>();
  const [streamMessages, setStreamMessages] = useState<StreamMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamInputClosed, setStreamInputClosed] = useState(false);

  // Error / history state
  const [error, setError] = useState<string | undefined>();
  const [histories, setHistories] = useState<HistoryItem[]>([]);
  const [historyDeletingId, setHistoryDeletingId] = useState<string | null>(null);
  const [isClearingHistory, setIsClearingHistory] = useState(false);

  // Dialog states
  const [isEnvDialogOpen, setIsEnvDialogOpen] = useState(false);
  const [editingEnvironment, setEditingEnvironment] = useState<ProjectEnvironment | null>(null);
  const [isProjectDialogOpen, setIsProjectDialogOpen] = useState(false);
  const [projectDialogMode, setProjectDialogMode] = useState<'create' | 'clone'>('create');
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isProtoImportDialogOpen, setIsProtoImportDialogOpen] = useState(false);
  const [protoImportPreview, setProtoImportPreview] = useState<ProtoImportPreview | null>(null);
  const [protoImportError, setProtoImportError] = useState<string | null>(null);
  const [isPreparingProtoImportPreview, setIsPreparingProtoImportPreview] = useState(false);
  const [isImportingProto, setIsImportingProto] = useState(false);
  const [isProtoImportDragActive, setIsProtoImportDragActive] = useState(false);
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => getStoredThemeMode());
  const schemaLoadTokenRef = useRef(0);
  const currentProjectIdRef = useRef<string | null>(null);

  const {
    projects,
    currentProject,
    environments: projectEnvironments,
    collections: projectCollections,
    activeEnvironmentId,
    isLoading: projectLoading,
    error: projectError,
    loadProjects,
    selectProject,
    createProject,
    updateProject,
    cloneProject,
    deleteProject,
    loadProjectCollections,
    createEnvironment,
    updateEnvironment,
    deleteEnvironment,
    setDefaultEnvironment,
    setActiveEnvironment,
    clearError: clearProjectError,
  } = useProjectStore();

  // Check if we need to show project onboarding
  const needsProjectOnboarding = !currentProject && projects.length === 0;

  // syncCurrentProjectIdRef 用于在异步导入流程里读取“最新当前项目”：
  // 通过 ref 可避免闭包拿到旧值，防止导入完成时把服务树错误地渲染到其他项目视图。
  useEffect(() => {
    currentProjectIdRef.current = currentProject?.id ?? null;
  }, [currentProject?.id]);

  const grpcStream = useGrpcStream({
    onMessage: (message) => {
      const newMessage: StreamMessage = {
        id: crypto.randomUUID(),
        type: 'message',
        payload: message,
        timestamp: Date.now(),
      };
      setStreamMessages((prev) => [...prev, newMessage]);
    },
    onMetadata: (streamMetadata) => {
      setResponse((prev) =>
        prev
          ? {
              ...prev,
              metadata: {
                ...prev.metadata,
                ...streamMetadata,
              },
            }
          : prev
      );
    },
    onError: (err) => {
      const newMessage: StreamMessage = {
        id: crypto.randomUUID(),
        type: 'error',
        error: err,
        timestamp: Date.now(),
      };
      setStreamMessages((prev) => [...prev, newMessage]);
      setIsStreaming(false);
      setStreamInputClosed(false);
      setResponse((prev) =>
        prev
          ? {
              ...prev,
              status: 'error',
              error: err,
            }
          : undefined
      );
    },
    onClose: () => {
      setIsStreaming(false);
      setStreamInputClosed(false);
      setResponse((prev) =>
        prev
          ? {
              ...prev,
              status: prev.status === 'streaming' ? 'success' : prev.status,
            }
          : undefined
      );
    },
  });

  // initProjectState 负责初始化项目上下文：
  // 1) 拉取全部项目；2) 自动选择当前项目；3) 无项目时创建默认项目。
  const initProjectState = useCallback(async () => {
    await loadProjects();
    const state = useProjectStore.getState();

    if (state.projects.length === 0) {
      const project = await state.createProject({
        name: t('project.defaultName'),
        description: t('project.defaultDescription'),
      });
      await state.selectProject(project.id);
      return;
    }

    if (state.currentProject?.id) {
      await state.selectProject(state.currentProject.id);
      return;
    }

    await state.selectProject(state.projects[0].id);
  }, [loadProjects, t]);

  useEffect(() => {
    void initProjectState();
  }, [initProjectState]);

  // autoDismissError 会在短时间后自动清空顶部错误提示，
  // 避免用户已经修复问题后仍被旧错误占据视图。
  useEffect(() => {
    if (!error && !projectError) {
      return;
    }

    const timer = window.setTimeout(() => {
      setError(undefined);
      clearProjectError();
    }, 4000);

    return () => {
      window.clearTimeout(timer);
    };
  }, [error, projectError, clearProjectError]);

  // syncThemeMode 负责在主题模式变更时立即更新页面主题并持久化设置，
  // 确保用户在设置中切换后无需刷新即可看到完整配色变化。
  useEffect(() => {
    applyThemeMode(themeMode);
  }, [themeMode]);

  // followSystemTheme 仅在“跟随系统”模式下监听系统主题变化，
  // 当系统从浅色/深色切换时自动刷新界面，保持与操作系统一致。
  useEffect(() => {
    if (themeMode !== 'system') {
      return;
    }

    return subscribeSystemThemeChange(() => {
      applyThemeMode('system');
    });
  }, [themeMode]);

  // handleThemeModeChange 负责响应设置弹窗的主题切换动作，
  // 统一通过主题模式状态驱动全局样式更新与本地持久化。
  const handleThemeModeChange = useCallback((nextMode: ThemeMode) => {
    setThemeMode(nextMode);
  }, []);

  useEffect(() => {
    if (!currentProject) {
      setHistories([]);
      return;
    }

    void tauriApi.getHistories(200).then((items) => {
      setHistories(items.filter((item) => !item.projectId || item.projectId === currentProject.id));
    });
  }, [currentProject?.id]);

  // restoreProjectServices 负责在项目切换后恢复项目内保存的 proto 服务列表。
  // 若项目未保存 proto 配置，则清空当前服务树，避免展示上一个项目的残留服务。
  useEffect(() => {
    if (!currentProject) {
      setServices([]);
      setProtoGroupRoot(null);
      setConnectionState('disconnected');
      return;
    }

    const connectConfig = decodeProjectProtoFilesForConnect(currentProject.protoFiles);
    if (connectConfig.protoFiles.length === 0) {
      setServices([]);
      setProtoGroupRoot(null);
      setConnectionState('disconnected');
      return;
    }

    let cancelled = false;

    const restore = async () => {
      // 导入 proto 仅加载服务描述，不代表已建立到目标地址的运行时连接。
      // 项目切换时先清空当前服务树，避免旧项目内容在异步恢复期间短暂残留。
      setServices([]);
      setProtoGroupRoot(null);
      setConnectionState('disconnected');
      setError(undefined);

      // 恢复项目服务前先清理后端运行态，确保切项目时总是全量重载。
      try {
        const disconnectResult = await tauriApi.grpcDisconnect();
        if (!disconnectResult.success && !cancelled) {
          console.warn(
            '[grpcui] 恢复项目前清理后端状态返回失败',
            disconnectResult.error || 'unknown error'
          );
        }
      } catch (err) {
        if (!cancelled) {
          console.warn('[grpcui] 恢复项目前清理后端状态失败，将继续尝试重连', err);
        }
      }

      try {
        const connectResult = await tauriApi.grpcConnect('localhost:50051', undefined, {
          protoFiles: connectConfig.protoFiles,
          importPaths:
            connectConfig.importPaths.length > 0 ? connectConfig.importPaths : undefined,
          useReflection: false,
        });

        if (cancelled) {
          return;
        }

        if (!connectResult.success) {
          setConnectionState('disconnected');
          setServices([]);
          setError(connectResult.error || '加载项目服务失败');
          return;
        }

        const servicesResult = await tauriApi.grpcListServices();
        if (cancelled) {
          return;
        }

        setConnectionState('disconnected');
        setServices(servicesResult.services);
        setProtoGroupRoot(connectConfig.groupRootPath);
      } catch (err) {
        if (cancelled) {
          return;
        }

        setConnectionState('disconnected');
        setServices([]);
        setError(err instanceof Error ? err.message : '加载项目服务失败');
      }
    };

    void restore();

    return () => {
      cancelled = true;
    };
  }, [currentProject?.id, currentProject?.protoFiles]);

  const activeProjectEnvironment = useMemo(
    () => projectEnvironments.find((env) => env.id === activeEnvironmentId) ?? null,
    [projectEnvironments, activeEnvironmentId]
  );

  // syncAddressWithActiveEnvironment 负责在项目默认环境恢复/环境切换后同步地址栏，
  // 保证顶部连接地址始终跟随“当前环境”的 baseUrl。
  useEffect(() => {
    if (!activeEnvironmentId) {
      return;
    }

    const selectedAddress = resolveAddressForEnvironmentSelection(
      projectEnvironments,
      activeEnvironmentId
    );

    if (!selectedAddress) {
      return;
    }

    setAddress(selectedAddress);
    setError(undefined);
  }, [activeEnvironmentId, projectEnvironments]);

  const resolvedRequestEnvironment = useMemo<ProjectEnvironment | null>(() => {
    if (!currentProject) {
      return null;
    }

    if (envRefType === 'none') {
      return null;
    }

    if (envRefType === 'specific' && requestEnvironmentId) {
      return (
        projectEnvironments.find((env) => env.id === requestEnvironmentId) ??
        null
      );
    }

    const fallbackId = activeEnvironmentId || currentProject.defaultEnvironmentId;
    if (!fallbackId) {
      return null;
    }

    return projectEnvironments.find((env) => env.id === fallbackId) ?? null;
  }, [
    currentProject,
    envRefType,
    requestEnvironmentId,
    projectEnvironments,
    activeEnvironmentId,
  ]);

  // resolveTextWithEnv 将请求文本中的 {{var}} 占位符替换为环境变量，
  // 保证地址、请求体、metadata 能在发送前统一解析。
  const resolveTextWithEnv = useCallback(
    (input: string): string => {
      if (!resolvedRequestEnvironment || envRefType === 'none') {
        return input;
      }

      const variableMap = resolvedRequestEnvironment.variables.reduce<Record<string, string>>(
        (acc, variable) => {
          acc[variable.key] = variable.value;
          return acc;
        },
        {}
      );

      return input.replace(/\{\{\s*([\w.-]+)\s*\}\}/g, (_match, variableName: string) => {
        return variableMap[variableName] ?? _match;
      });
    },
    [resolvedRequestEnvironment, envRefType]
  );

  const toMetadataEntries = useCallback((raw: Record<string, string>): MetadataEntry[] => {
    return Object.entries(raw).map(([key, value]) => ({
      id: crypto.randomUUID(),
      key,
      value,
      enabled: true,
    }));
  }, []);

  // loadMethodInputSchema 会在方法选中后拉取入参结构，
  // 供请求面板渲染“字段化编辑器”，减少手写 JSON 的负担。
  const loadMethodInputSchema = useCallback(
    async (
      serviceName: string,
      methodName: string,
      options?: {
        hydrateBody?: boolean;
      }
    ) => {
      const token = schemaLoadTokenRef.current + 1;
      schemaLoadTokenRef.current = token;

      try {
        const schema = await tauriApi.grpcGetMethodInputSchema(serviceName, methodName);
        if (schemaLoadTokenRef.current !== token) {
          return;
        }

        setMethodInputSchema(schema);

        if (options?.hydrateBody) {
          setRequestBody(JSON.stringify(buildDefaultBodyFromSchema(schema), null, 2));
        }
      } catch (err) {
        if (schemaLoadTokenRef.current !== token) {
          return;
        }

        console.warn('[grpcui] 获取方法入参 schema 失败，将回退到 JSON 编辑模式', err);
        setMethodInputSchema(null);

        if (options?.hydrateBody) {
          setRequestBody('{}');
        }
      }
    },
    []
  );

  // persistProjectProtoFiles 将指定项目的 proto 配置持久化到项目实体。
  // 使用显式 projectId 可避免异步导入期间项目切换导致“写入到错误项目”的风险。
  const persistProjectProtoFiles = useCallback(
    async (projectId: string, protoFiles: string[]) => {
      await updateProject(projectId, {
        protoFiles,
      });
    },
    [updateProject]
  );

  const handleMethodSelect = useCallback(
    (service: Service, method: Method) => {
      if (grpcStream.isConnected) {
        void grpcStream.close();
      }

      setSelectedMethod({
        service: service.fullName,
        method: method.name,
        type: method.type,
      });
      void loadMethodInputSchema(service.fullName, method.name, { hydrateBody: true });
      setResponse(undefined);
      setStreamMessages([]);
      setStreamInputClosed(false);
      grpcStream.clearMessages();
    },
    [grpcStream, loadMethodInputSchema]
  );

  const handleProjectCreate = useCallback(async (name: string) => {
    try {
      const project = await createProject({ name: name.trim() });
      await selectProject(project.id);
      setSelectedMethod(undefined);
      setMethodInputSchema(null);
      setRequestBody('{}');
      setMetadata([]);
      setIsProjectDialogOpen(false);
    } catch (err) {
      console.error('Failed to create project:', err);
      setError(err instanceof Error ? err.message : 'Failed to create project');
    }
  }, [createProject, selectProject]);

  const handleProjectClone = useCallback(async (name: string) => {
    if (!currentProject) {
      return;
    }
    const project = await cloneProject(currentProject.id, name.trim());
    await selectProject(project.id);
    setSelectedMethod(undefined);
    setMethodInputSchema(null);
    setRequestBody('{}');
    setMetadata([]);
    setIsProjectDialogOpen(false);
  }, [cloneProject, currentProject, selectProject]);

  const openProjectCreateDialog = useCallback(() => {
    setProjectDialogMode('create');
    setIsProjectDialogOpen(true);
  }, []);

  const openProjectCloneDialog = useCallback(() => {
    if (!currentProject) return;
    setProjectDialogMode('clone');
    setIsProjectDialogOpen(true);
  }, [currentProject]);

  const handleProjectDelete = useCallback(async () => {
    if (!currentProject) {
      return;
    }

    if (!window.confirm(t('project.deleteConfirm', { name: currentProject.name }))) {
      return;
    }

    await deleteProject(currentProject.id);

    const next = useProjectStore.getState().projects[0];
    if (next) {
      await selectProject(next.id);
      setSelectedMethod(undefined);
      setMethodInputSchema(null);
      setRequestBody('{}');
      setMetadata([]);
    }
  }, [currentProject, deleteProject, selectProject, t]);

  const handleProjectSelect = useCallback(
    async (id: string) => {
      if (!id) {
        return;
      }

      if (grpcStream.isConnected) {
        await grpcStream.close();
      }

      // 切项目前先让后端显式断连并重置 parser，
      // 保证新项目恢复服务时不会夹带上一个项目的 Proto 描述。
      try {
        const disconnectResult = await tauriApi.grpcDisconnect();
        if (!disconnectResult.success) {
          console.warn(
            '[grpcui] 切项目前重置后端状态返回失败',
            disconnectResult.error || 'unknown error'
          );
        }
      } catch (err) {
        console.warn('[grpcui] 切项目前重置后端状态失败，将继续执行项目切换', err);
      }

      // 切项目前先清空当前项目视图状态，保证新项目以全新视图渲染，
      // 避免用户在异步切换期间看到上一个项目的服务/方法残留。
      setServices([]);
      setProtoGroupRoot(null);
      setSelectedMethod(undefined);
      setMethodInputSchema(null);
      setRequestBody('{}');
      setMetadata([]);
      setResponse(undefined);
      setStreamMessages([]);
      setStreamInputClosed(false);
      setConnectionState('disconnected');
      setError(undefined);

      await selectProject(id);
    },
    [grpcStream, selectProject]
  );

  // handleSubmitEnvironment 负责统一处理环境新建与编辑，
  // 让环境地址、TLS、变量与请求头在同一提交路径中保持一致。
  const handleSubmitEnvironment = useCallback(async (formData: EnvironmentFormData) => {
    if (!currentProject) {
      setError(t('project.selectFirst'));
      return;
    }

    try {
      if (editingEnvironment) {
        await updateEnvironment({
          ...editingEnvironment,
          name: formData.name.trim(),
          baseUrl: formData.baseUrl.trim(),
          variables: formData.variables,
          metadata: formData.metadata,
          tls: formData.tls,
        });
      } else {
        await createEnvironment(currentProject.id, {
          name: formData.name.trim(),
          baseUrl: formData.baseUrl.trim(),
          variables: formData.variables,
          metadata: formData.metadata,
          tls: formData.tls,
          isDefault: projectEnvironments.length === 0,
        });
      }

      setIsEnvDialogOpen(false);
      setEditingEnvironment(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('environment.createFailed'));
    }
  }, [
    currentProject,
    editingEnvironment,
    updateEnvironment,
    createEnvironment,
    projectEnvironments.length,
    t,
  ]);

  // handleEditEnvironment 负责把目标环境载入编辑弹窗，
  // 以便用户直接修改已有环境配置。
  const handleEditEnvironment = useCallback((environment: ProjectEnvironment) => {
    setEditingEnvironment(environment);
    setIsEnvDialogOpen(true);
  }, []);

  // handleDeleteEnvironment 负责删除环境并同步关闭编辑态，
  // 避免被删环境仍在弹窗中继续编辑。
  const handleDeleteEnvironment = useCallback(async (environment: ProjectEnvironment) => {
    if (!currentProject) {
      setError(t('project.selectFirst'));
      return;
    }

    const confirmed = window.confirm(`确认删除环境“${environment.name}”吗？`);
    if (!confirmed) {
      return;
    }

    try {
      await deleteEnvironment(currentProject.id, environment.id);
      if (editingEnvironment?.id === environment.id) {
        setEditingEnvironment(null);
        setIsEnvDialogOpen(false);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除环境失败');
    }
  }, [currentProject, deleteEnvironment, editingEnvironment, t]);

  const handleConnect = useCallback(async () => {
    const connectionAddress = resolveTextWithEnv(
      resolveConnectionAddress(address, activeProjectEnvironment?.baseUrl)
    );

    if (!connectionAddress) {
      setError(t('connection.addressRequired'));
      return;
    }

    setConnectionState('connecting');
    setError(undefined);

    try {
      const result = await tauriApi.grpcConnect(connectionAddress, activeProjectEnvironment?.tls, {
        useReflection: true,
      });
      if (result.success) {
        setAddress(connectionAddress);
        setConnectionState('connected');
        const servicesResult = await tauriApi.grpcListServices();
        setServices(servicesResult.services);
        setSelectedMethod(undefined);
        setMethodInputSchema(null);
        setRequestBody('{}');
        setProtoGroupRoot(null);
      } else {
        setConnectionState('error');
        setError(result.error || '连接失败');
      }
    } catch (err) {
      setConnectionState('error');
      setError(err instanceof Error ? err.message : '连接失败');
    }
  }, [address, activeProjectEnvironment, resolveTextWithEnv]);

  const handleDisconnect = useCallback(() => {
    setConnectionState('disconnected');
    setServices([]);
    setSelectedMethod(undefined);
    setMethodInputSchema(null);
    setRequestBody('{}');
    setError(undefined);
    setStreamInputClosed(false);
    if (grpcStream.isConnected) {
      void grpcStream.close();
    }
    // 主动断开时同步清理后端 parser/连接缓存，避免后续再次连接出现旧描述符残留。
    void tauriApi.grpcDisconnect()
      .then((disconnectResult) => {
        if (!disconnectResult.success) {
          console.warn(
            '[grpcui] 断开连接时重置后端状态返回失败',
            disconnectResult.error || 'unknown error'
          );
        }
      })
      .catch((err) => {
        console.warn('[grpcui] 断开连接时重置后端状态失败', err);
      });
  }, [grpcStream]);

  // normalizeProtoSourcePath 统一 sourcePath 比较格式：
  // 把不同平台分隔符折叠为 `/`，并去除重复斜杠与首尾空白，避免删除匹配遗漏。
  const normalizeProtoSourcePath = useCallback((path: string): string => {
    return path.replace(/\\/g, '/').replace(/\/+/g, '/').trim();
  }, []);

  // handleRemoveProtoSources 负责在当前项目中删除一组 proto 来源路径：
  // 1) 计算删除后的项目 proto 配置；2) 持久化到当前项目；
  // 3) 必要时清理已选方法，避免界面仍指向已删除来源。
  const handleRemoveProtoSources = useCallback(
    async (sourcePaths: string[], targetLabel: string) => {
      if (!currentProject?.id) {
        setError(t('project.selectFirst'));
        return;
      }

      const normalizedSourcePaths = Array.from(
        new Set(
          sourcePaths
            .map((path) => normalizeProtoSourcePath(path))
            .filter((path) => path.length > 0)
        )
      );

      if (!normalizedSourcePaths.length) {
        setError(t('service.deleteSourceMissing'));
        return;
      }

      const { nextStoredProtoFiles, removedCount } = removeProjectProtoFilesBySourcePaths(
        currentProject.protoFiles,
        normalizedSourcePaths
      );

      if (removedCount === 0) {
        setError(t('service.deleteSourceMissing'));
        return;
      }

      const confirmed = window.confirm(
        t('service.deleteConfirm', {
          target: targetLabel,
          count: removedCount,
        })
      );
      if (!confirmed) {
        return;
      }

      const targetProjectId = currentProject.id;
      const removedSourceSet = new Set(normalizedSourcePaths);
      const selectedService = selectedMethod
        ? services.find((service) => service.fullName === selectedMethod.service)
        : undefined;
      const selectedServiceSource = selectedService?.sourcePath
        ? normalizeProtoSourcePath(selectedService.sourcePath)
        : null;

      try {
        await persistProjectProtoFiles(targetProjectId, nextStoredProtoFiles);

        // 若用户在删除期间切换了项目，只写入目标项目，不污染当前 UI。
        if (currentProjectIdRef.current !== targetProjectId) {
          return;
        }

        if (selectedServiceSource && removedSourceSet.has(selectedServiceSource)) {
          setSelectedMethod(undefined);
          setMethodInputSchema(null);
          setRequestBody('{}');
        }

        setError(undefined);
      } catch (err) {
        if (currentProjectIdRef.current !== targetProjectId) {
          return;
        }
        setError(err instanceof Error ? err.message : t('service.deleteFailed'));
      }
    },
    [
      currentProject?.id,
      currentProject?.protoFiles,
      normalizeProtoSourcePath,
      persistProjectProtoFiles,
      selectedMethod,
      services,
      t,
    ]
  );

  // handleDeleteFolderFromServices 负责删除目录分组内全部 proto 引用。
  // 分组会转成具体 sourcePath 列表后统一走删除管线，避免目录名解析歧义。
  const handleDeleteFolderFromServices = useCallback(
    (sourcePaths: string[], folderLabel: string) => {
      void handleRemoveProtoSources(sourcePaths, folderLabel);
    },
    [handleRemoveProtoSources]
  );

  // handleDeleteServiceFromServices 负责删除单个服务对应的 proto 引用。
  // 服务本身不直接持久化，删除动作本质是移除其 sourcePath 对应的 proto 文件。
  const handleDeleteServiceFromServices = useCallback(
    (service: Service) => {
      if (!service.sourcePath) {
        setError(t('service.deleteSourceMissing'));
        return;
      }

      void handleRemoveProtoSources([service.sourcePath], service.name);
    },
    [handleRemoveProtoSources, t]
  );

  // handleDeleteMethodFromServices 负责删除方法所属 proto 引用。
  // 由于方法粒度不单独持久化，删除方法会移除其所在 proto，从而同步移除同文件定义。
  const handleDeleteMethodFromServices = useCallback(
    (service: Service, method: Method) => {
      if (!service.sourcePath) {
        setError(t('service.deleteSourceMissing'));
        return;
      }

      const targetLabel = `${service.name}/${method.name}`;
      void handleRemoveProtoSources([service.sourcePath], targetLabel);
    },
    [handleRemoveProtoSources, t]
  );

  // importProtoFile 执行“单文件 proto 导入”：
  // 1) 基于当前地址/环境生成连接目标；2) 使用 protoFile 建立描述上下文；
  // 3) 仅在目标项目仍为当前项目时刷新服务树，再持久化到目标项目。
  // 该流程只更新描述，不改变连接态为 connected。
  const importProtoFile = useCallback(
    async (protoFile: string, targetProjectId: string) => {
      const normalizedPath = protoFile.trim();
      if (!normalizedPath) {
        throw new Error(t('connection.importProtoNoFile'));
      }
      if (!isProtoFilePath(normalizedPath)) {
        throw new Error(t('connection.importProtoOnlyProto'));
      }

      const connectionAddress = resolveTextWithEnv(
        resolveConnectionAddress(address, activeProjectEnvironment?.baseUrl)
      );

      setConnectionState('disconnected');
      const result = await tauriApi.grpcConnect(connectionAddress, activeProjectEnvironment?.tls, {
        protoFile: normalizedPath,
        useReflection: false,
      });

      if (!result.success) {
        setConnectionState('disconnected');
        throw new Error(result.error || t('connection.importProtoFailed'));
      }

      const servicesResult = await tauriApi.grpcListServices();
      const isActiveTargetProject = currentProjectIdRef.current === targetProjectId;
      if (isActiveTargetProject) {
        setAddress(connectionAddress);
        setConnectionState('disconnected');
        setServices(servicesResult.services);
        setSelectedMethod(undefined);
        setMethodInputSchema(null);
        setRequestBody('{}');
        setProtoGroupRoot(null);
      }
      await persistProjectProtoFiles(targetProjectId, [normalizedPath]);
    },
    [
      address,
      activeProjectEnvironment,
      persistProjectProtoFiles,
      resolveTextWithEnv,
      t,
    ]
  );

  // importProtoDirectory 执行“目录 proto 导入”：
  // 1) 使用预览阶段拿到的 rootDir + relativePaths；2) 计算 import path 候选；
  // 3) 仅在目标项目仍为当前项目时刷新服务树，并把目录根编码到目标项目配置。
  const importProtoDirectory = useCallback(
    async (preview: ProtoImportPreview, targetProjectId: string) => {
      if (!preview.rootDir || preview.relativePaths.length === 0) {
        throw new Error(t('connection.importProtoFolderNoProto'));
      }

      const projectProtoFiles = encodeProjectProtoFilesForFolderImport(
        preview.rootDir,
        preview.relativePaths
      );

      const connectionAddress = resolveTextWithEnv(
        resolveConnectionAddress(address, activeProjectEnvironment?.baseUrl)
      );

      setConnectionState('disconnected');
      const importPathCandidates = buildImportPathCandidates(preview.rootDir);
      const result = await tauriApi.grpcConnect(connectionAddress, activeProjectEnvironment?.tls, {
        protoFiles: preview.relativePaths,
        importPaths: importPathCandidates,
        useReflection: false,
      });

      if (!result.success) {
        setConnectionState('disconnected');
        throw new Error(result.error || t('connection.importProtoFolderFailed'));
      }

      console.info(
        `[grpcui] 已从目录导入 ${preview.relativePaths.length} 个 proto 文件，import paths: ${importPathCandidates.length}`
      );
      const servicesResult = await tauriApi.grpcListServices();
      const isActiveTargetProject = currentProjectIdRef.current === targetProjectId;
      if (isActiveTargetProject) {
        setAddress(connectionAddress);
        setConnectionState('disconnected');
        setServices(servicesResult.services);
        setSelectedMethod(undefined);
        setMethodInputSchema(null);
        setRequestBody('{}');
        setProtoGroupRoot(preview.rootDir);
      }
      await persistProjectProtoFiles(targetProjectId, projectProtoFiles);
    },
    [
      address,
      activeProjectEnvironment,
      persistProjectProtoFiles,
      resolveTextWithEnv,
      t,
    ]
  );

  // buildProtoImportPreviewFromFile 把文件路径转成“待确认导入”预览数据。
  // 对扩展名做严格校验，可在真正导入前就阻断非 proto 文件，减少无效连接调用。
  const buildProtoImportPreviewFromFile = useCallback(
    (protoFilePath: string): ProtoImportPreview => {
      const normalizedPath = protoFilePath.trim();
      if (!normalizedPath) {
        throw new Error(t('connection.importProtoNoFile'));
      }
      if (!isProtoFilePath(normalizedPath)) {
        throw new Error(t('connection.importProtoOnlyProto'));
      }

      const previewName = resolveProtoImportDisplayName(normalizedPath);

      return {
        sourceType: 'file',
        sourcePath: normalizedPath,
        rootDir: null,
        absolutePaths: [normalizedPath],
        relativePaths: [previewName],
      };
    },
    [t]
  );

  // buildProtoImportPreviewFromDirectory 负责目录预览构建：
  // 1) 调用后端扫描目录；2) 校验 proto 数量；3) 返回可直接展示与后续确认导入的数据模型。
  const buildProtoImportPreviewFromDirectory = useCallback(
    async (directoryPath: string): Promise<ProtoImportPreview> => {
      const normalizedPath = directoryPath.trim();
      if (!normalizedPath) {
        throw new Error(t('connection.importProtoFolderNoDir'));
      }

      const discovery = await tauriApi.discoverProtoFiles(normalizedPath);
      if (discovery.relativePaths.length === 0) {
        throw new Error(t('connection.importProtoFolderNoProto'));
      }

      return {
        sourceType: 'directory',
        sourcePath: discovery.rootDir,
        rootDir: discovery.rootDir,
        absolutePaths: discovery.absolutePaths,
        relativePaths: discovery.relativePaths,
      };
    },
    [t]
  );

  // prepareProtoImportPreview 根据来源路径准备预览：
  // - expectedType 已知时直接按文件/目录处理；
  // - 未知时优先按扩展名判定（`.proto` 走文件，其余走目录）。
  //
  // 之所以不再依赖前端 fs.stat：
  // - 某些平台/权限配置下，拖拽路径做 stat 可能被权限策略拦截；
  // - 使用“轻量判定 + 后端目录扫描”更稳定，且不影响文件选择按钮流程。
  // 预览准备完成后仅更新弹窗状态，不会立即执行真正导入。
  const prepareProtoImportPreview = useCallback(
    async (
      importPath: string,
      expectedType?: ProtoImportSourceType,
      droppedCount?: number
    ) => {
      const normalizedPath = importPath.trim();
      if (!normalizedPath) {
        setProtoImportPreview(null);
        setProtoImportError(t('connection.importTargetInvalid'));
        return;
      }

      setIsPreparingProtoImportPreview(true);
      setProtoImportError(null);

      try {
        const sourceType =
          expectedType || (isProtoFilePath(normalizedPath) ? 'file' : 'directory');

        const preview =
          sourceType === 'directory'
            ? await buildProtoImportPreviewFromDirectory(normalizedPath)
            : buildProtoImportPreviewFromFile(normalizedPath);

        setProtoImportPreview(preview);
        if (typeof droppedCount === 'number' && droppedCount > 1) {
          setProtoImportError(t('connection.importDropMultiple', { count: droppedCount }));
        }
      } catch (err) {
        setProtoImportPreview(null);
        setProtoImportError(
          err instanceof Error ? err.message : t('connection.importPreviewFailed')
        );
      } finally {
        setIsPreparingProtoImportPreview(false);
      }
    },
    [buildProtoImportPreviewFromDirectory, buildProtoImportPreviewFromFile, t]
  );

  // openProtoImportDialog 负责进入统一导入弹窗。
  // 每次打开都重置预览、错误与拖拽高亮状态，避免残留上一次操作结果干扰当前导入。
  const openProtoImportDialog = useCallback(() => {
    setIsProtoImportDialogOpen(true);
    setProtoImportPreview(null);
    setProtoImportError(null);
    setIsPreparingProtoImportPreview(false);
    setIsImportingProto(false);
    setIsProtoImportDragActive(false);
  }, []);

  // closeProtoImportDialog 关闭导入弹窗并清理临时状态。
  // 清理逻辑与打开时保持一致，确保用户下次进入时从“空白待选择”状态开始。
  const closeProtoImportDialog = useCallback(() => {
    setIsProtoImportDialogOpen(false);
    setProtoImportPreview(null);
    setProtoImportError(null);
    setIsPreparingProtoImportPreview(false);
    setIsImportingProto(false);
    setIsProtoImportDragActive(false);
  }, []);

  // handleSelectImportFile 触发“导入文件”按钮：
  // 使用系统文件选择器拿到单个 proto 路径，并仅生成预览，等待用户点击确认导入。
  const handleSelectImportFile = useCallback(async () => {
    const fileSelection = await open({
      multiple: false,
      filters: [{ name: 'Proto', extensions: ['proto'] }],
    });
    const protoFile = normalizeProtoDialogSelection(fileSelection);
    if (!protoFile) {
      return;
    }

    await prepareProtoImportPreview(protoFile, 'file');
  }, [prepareProtoImportPreview]);

  // handleSelectImportDirectory 触发“导入目录”按钮：
  // 选择目录后先扫描并展示预览，用户确认后才会真正把目录内 proto 加载到服务树。
  const handleSelectImportDirectory = useCallback(async () => {
    const dirSelection = await open({
      directory: true,
      multiple: false,
    });
    const protoRoot = normalizeProtoDialogSelection(dirSelection);
    if (!protoRoot) {
      return;
    }

    await prepareProtoImportPreview(protoRoot, 'directory');
  }, [prepareProtoImportPreview]);

  // handleConfirmProtoImport 负责执行“预览后的确认导入”：
  // 根据预览类型分流到文件/目录导入，实现“先预览后确认”的交互要求。
  const handleConfirmProtoImport = useCallback(async () => {
    if (!protoImportPreview) {
      setProtoImportError(t('connection.importNeedPreview'));
      return;
    }
    if (!currentProject?.id) {
      setProtoImportError(t('project.selectFirst'));
      return;
    }

    const targetProjectId = currentProject.id;

    setError(undefined);
    setProtoImportError(null);
    setIsImportingProto(true);

    try {
      if (protoImportPreview.sourceType === 'directory') {
        await importProtoDirectory(protoImportPreview, targetProjectId);
      } else {
        await importProtoFile(protoImportPreview.sourcePath, targetProjectId);
      }
      closeProtoImportDialog();
    } catch (err) {
      const message =
        err instanceof Error ? err.message : t('connection.importConfirmFailed');
      if (currentProjectIdRef.current === targetProjectId) {
        setError(message);
        setProtoImportError(message);
      }
    } finally {
      setIsImportingProto(false);
    }
  }, [
    closeProtoImportDialog,
    currentProject?.id,
    importProtoDirectory,
    importProtoFile,
    protoImportPreview,
    t,
  ]);

  // 监听 Tauri 原生拖拽事件，在导入弹窗打开时支持“拖入文件/目录自动识别并预览”。
  // 该监听绑定在窗口级别，避免浏览器层拖拽 API 在不同平台行为不一致的问题。
  useEffect(() => {
    if (!isProtoImportDialogOpen) {
      return;
    }

    let unlisten: (() => void) | undefined;
    let disposed = false;

    const bindDragDropListener = async () => {
      try {
        unlisten = await getCurrentWindow().onDragDropEvent((event) => {
          if (disposed) {
            return;
          }

          if (event.payload.type === 'enter' || event.payload.type === 'over') {
            setIsProtoImportDragActive(true);
            return;
          }

          if (event.payload.type === 'leave') {
            setIsProtoImportDragActive(false);
            return;
          }

          if (event.payload.type === 'drop') {
            setIsProtoImportDragActive(false);
            const droppedPath = event.payload.paths[0]?.trim();
            if (!droppedPath) {
              setProtoImportPreview(null);
              setProtoImportError(t('connection.importDropNoPath'));
              return;
            }

            void prepareProtoImportPreview(
              droppedPath,
              undefined,
              event.payload.paths.length
            );
          }
        });
      } catch (err) {
        console.warn('[grpcui] 监听拖拽导入事件失败', err);
      }
    };

    void bindDragDropListener();

    return () => {
      disposed = true;
      setIsProtoImportDragActive(false);
      if (unlisten) {
        unlisten();
      }
    };
  }, [isProtoImportDialogOpen, prepareProtoImportPreview, t]);

  // handleConnectionEnvironmentChange 负责处理连接栏环境切换：
  // 先同步项目级活跃环境，再把环境 baseUrl 自动回填到地址栏，保证“选环境后可直接连接”。
  const handleConnectionEnvironmentChange = useCallback(
    (envId: string) => {
      setActiveEnvironment(envId || null);
      const selectedAddress = resolveAddressForEnvironmentSelection(projectEnvironments, envId);
      if (selectedAddress) {
        setAddress(selectedAddress);
        setError(undefined);
      }
    },
    [projectEnvironments, setActiveEnvironment]
  );

  const handleEnvRefChange = useCallback((type: EnvRefType, envId?: string) => {
    setEnvRefType(type);
    if (type === 'specific') {
      setRequestEnvironmentId(envId);
    } else {
      setRequestEnvironmentId(undefined);
    }
  }, []);

  const handleSend = useCallback(async () => {
    if (!selectedMethod || !currentProject) {
      return;
    }

    setIsLoading(true);
    setError(undefined);

    const requestId = crypto.randomUUID();
    const effectiveAddress = resolveTextWithEnv(
      resolvedRequestEnvironment?.baseUrl || address
    );

    const metadataRecord = metadata.reduce(
      (acc, entry) => {
        if (entry.enabled && entry.key) {
          acc[resolveTextWithEnv(entry.key)] = resolveTextWithEnv(entry.value);
        }
        return acc;
      },
      {} as Record<string, string>
    );

    const mergedMetadata = {
      ...(envRefType === 'none' ? {} : resolvedRequestEnvironment?.metadata ?? {}),
      ...metadataRecord,
    };

    const resolvedBody = resolveTextWithEnv(requestBody || '{}');
    let parsedBody: unknown;
    try {
      parsedBody = JSON.parse(resolvedBody);
    } catch {
      setIsLoading(false);
      setError(t('request.invalidJson'));
      return;
    }

    const methodPath = `${selectedMethod.service}/${selectedMethod.method}`;

    if (selectedMethod.type !== 'unary') {
      const streamTypeMap: Record<Exclude<Method['type'], 'unary'>, 'server' | 'client' | 'bidi'> = {
        server_stream: 'server',
        client_stream: 'client',
        bidi_stream: 'bidi',
      };

      if (grpcStream.isConnected) {
        if (selectedMethod.type === 'server_stream') {
          setIsLoading(false);
          setError(t('request.serverStreamAlreadyRunning'));
          return;
        }

        if (streamInputClosed) {
          setIsLoading(false);
          setError(t('request.streamInputClosed'));
          return;
        }

        try {
          await grpcStream.sendMessage(parsedBody);
          setIsLoading(false);
        } catch (err) {
          setIsLoading(false);
          const errorMessage = err instanceof Error ? err.message : t('request.streamFailed');
          setError(errorMessage);
        }

        return;
      }

      setStreamMessages([]);
      grpcStream.clearMessages();
      setIsStreaming(true);
      setStreamInputClosed(false);
      setResponse({
        id: crypto.randomUUID(),
        requestId,
        status: 'streaming',
        body: '',
        metadata: {},
        trailers: {},
        duration: 0,
        timestamp: Date.now(),
      });

      try {
        const initialBody = selectedMethod.type === 'server_stream' ? resolvedBody : '';

        await grpcStream.connect(
          effectiveAddress,
          methodPath,
          initialBody,
          mergedMetadata,
          streamTypeMap[selectedMethod.type],
          resolvedRequestEnvironment?.tls
        );

        setIsLoading(false);
      } catch (err) {
        setIsLoading(false);
        setIsStreaming(false);
        const errorMessage =
          err instanceof Error ? err.message : t('request.streamFailed');
        setError(errorMessage);
        setResponse({
          id: crypto.randomUUID(),
          requestId,
          status: 'error',
          body: '',
          metadata: {},
          trailers: {},
          duration: 0,
          timestamp: Date.now(),
          error: errorMessage,
        });
      }

      return;
    }

    setResponse({
      id: crypto.randomUUID(),
      requestId,
      status: 'pending',
      body: '',
      metadata: {},
      trailers: {},
      duration: 0,
      timestamp: Date.now(),
    });

    try {
      const result = await tauriApi.grpcInvoke({
        method: methodPath,
        body: resolvedBody,
        metadata: mergedMetadata,
        address: effectiveAddress,
        tls: resolvedRequestEnvironment?.tls,
      });

      setIsLoading(false);

      if (result.error) {
        setResponse({
          id: crypto.randomUUID(),
          requestId,
          status: 'error',
          body: '',
          metadata: result.metadata || {},
          trailers: {},
          duration: result.duration,
          timestamp: Date.now(),
          error: result.error,
        });
      } else {
        setResponse({
          id: crypto.randomUUID(),
          requestId,
          status: 'success',
          statusCode: String(result.code) + " " + result.status,
          body: JSON.stringify(result.data, null, 2),
          metadata: result.metadata || {},
          trailers: {},
          duration: result.duration,
          timestamp: Date.now(),
        });
      }

      await tauriApi.addHistory({
        id: crypto.randomUUID(),
        projectId: currentProject.id,
        timestamp: Date.now(),
        service: selectedMethod.service,
        method: selectedMethod.method,
        address: effectiveAddress,
        status: result.error ? 'error' : 'success',
        responseCode: Number.isFinite(result.code) ? result.code : undefined,
        responseMessage: result.message || result.error,
        duration: result.duration,
        requestSnapshot: {
          id: requestId,
          name: `${selectedMethod.service}/${selectedMethod.method}`,
          type: selectedMethod.type,
          service: selectedMethod.service,
          method: selectedMethod.method,
          body: resolvedBody,
          metadata: mergedMetadata,
          envRefType,
          environmentId:
            envRefType === 'specific'
              ? requestEnvironmentId
              : resolvedRequestEnvironment?.id,
        },
      });

      const refreshedHistory = await tauriApi.getHistories(200);
      setHistories(refreshedHistory.filter((item) => !item.projectId || item.projectId === currentProject.id));
    } catch (err) {
      setIsLoading(false);
      const errorMessage = err instanceof Error ? err.message : 'Request failed';
      setError(errorMessage);
      setResponse({
        id: crypto.randomUUID(),
        requestId,
        status: 'error',
        body: '',
        metadata: {},
        trailers: {},
        duration: 0,
        timestamp: Date.now(),
        error: errorMessage,
      });
    }
  }, [
    selectedMethod,
    currentProject,
    resolveTextWithEnv,
    resolvedRequestEnvironment,
    address,
    metadata,
    envRefType,
    requestBody,
    grpcStream,
    streamInputClosed,
    requestEnvironmentId,
    t,
  ]);

  // handleEndStream 用于 client/bidi 流的 half-close。
  // 调用后不会立刻断开连接，而是等待服务端回包并自然结束。
  const handleEndStream = useCallback(async () => {
    if (!grpcStream.isConnected || !selectedMethod || selectedMethod.type === 'server_stream') {
      return;
    }

    setIsLoading(true);
    setError(undefined);

    try {
      await grpcStream.end();
      setStreamInputClosed(true);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : t('request.streamFailed');
      setError(errorMessage);
    } finally {
      setIsLoading(false);
    }
  }, [grpcStream, selectedMethod, t]);

  // handleCloseStream 主动中断当前流，常用于测试过程中的快速重置。
  const handleCloseStream = useCallback(async () => {
    if (!grpcStream.isConnected) {
      return;
    }

    setIsLoading(true);
    try {
      await grpcStream.close();
      setStreamInputClosed(false);
      setIsStreaming(false);
    } finally {
      setIsLoading(false);
    }
  }, [grpcStream]);

  const handleLoadSavedRequest = useCallback(
    (item: RequestItem | ProjectRequestItem) => {
      setSelectedMethod({
        service: item.service,
        method: item.method,
        type: item.type,
      });
      void loadMethodInputSchema(item.service, item.method, { hydrateBody: false });
      setRequestBody(item.body);
      setMetadata(toMetadataEntries(item.metadata));
      setEnvRefType(item.envRefType ?? 'inherit');
      setRequestEnvironmentId(item.environmentId);
      setStreamInputClosed(false);
      setActiveTab('services');
    },
    [loadMethodInputSchema, toMetadataEntries]
  );

  // handleDeleteHistoryItem 负责删除单条历史记录，
  // 删除成功后只更新本地列表，避免再次全量拉取带来的额外等待。
  const handleDeleteHistoryItem = useCallback(
    async (historyId: string) => {
      const confirmed = await confirmDialog(t('history.deleteConfirm'), {
        title: t('sidebar.history'),
        kind: 'warning',
        okLabel: t('history.deleteOne'),
        cancelLabel: t('common.cancel'),
      });
      if (!confirmed) {
        return;
      }

      setHistoryDeletingId(historyId);
      try {
        await tauriApi.deleteHistory(historyId);
        setHistories((prev) => prev.filter((item) => item.id !== historyId));
      } catch (err) {
        setError(err instanceof Error ? err.message : t('history.deleteFailed'));
      } finally {
        setHistoryDeletingId((current) => (current === historyId ? null : current));
      }
    },
    [t]
  );

  // handleClearHistories 负责清空全部历史记录，
  // 该操作是不可逆的，因此提交前会弹出确认提示避免误触。
  const handleClearHistories = useCallback(async () => {
    if (!currentProject || !histories.length) {
      return;
    }

    const confirmed = await confirmDialog(t('history.clearAllConfirm'), {
      title: t('sidebar.history'),
      kind: 'warning',
      okLabel: t('history.clearAll'),
      cancelLabel: t('common.cancel'),
    });
    if (!confirmed) {
      return;
    }

    setIsClearingHistory(true);
    try {
      await tauriApi.clearHistories(currentProject.id);
      setHistories([]);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('history.clearAllFailed'));
    } finally {
      setIsClearingHistory(false);
    }
  }, [currentProject, histories.length, t]);

  // handleSave 将当前请求落到项目收藏中，
  // 同时保留环境引用策略，便于后续复用同一调用上下文。
  const handleSave = useCallback(async () => {
    if (!selectedMethod || !currentProject) {
      return;
    }

    const collectionName = window.prompt(t('project.collectionNamePlaceholder'), t('collection.defaultName'));
    if (!collectionName?.trim()) {
      return;
    }

    const metadataRecord = metadata.reduce((acc, entry) => {
      if (entry.enabled && entry.key) {
        acc[entry.key] = entry.value;
      }
      return acc;
    }, {} as Record<string, string>);

    const requestItem: RequestItem = {
      id: crypto.randomUUID(),
      name: `${selectedMethod.service}/${selectedMethod.method}`,
      type: selectedMethod.type,
      service: selectedMethod.service,
      method: selectedMethod.method,
      body: requestBody,
      metadata: metadataRecord,
      envRefType,
      environmentId:
        envRefType === 'specific'
          ? requestEnvironmentId
          : resolvedRequestEnvironment?.id,
    };

    const existingCollection = projectCollections.find(
      (collection) => collection.name === collectionName.trim()
    );

    const now = new Date().toISOString();
    if (existingCollection) {
      await tauriApi.saveCollection({
        ...existingCollection,
        items: [...existingCollection.items, requestItem],
        updatedAt: now,
      });
    } else {
      await tauriApi.saveCollection({
        id: crypto.randomUUID(),
        projectId: currentProject.id,
        name: collectionName.trim(),
        folders: [],
        items: [requestItem],
        createdAt: now,
        updatedAt: now,
      });
    }

    await loadProjectCollections(currentProject.id);
    setActiveTab('collections');
  }, [
    selectedMethod,
    currentProject,
    metadata,
    requestBody,
    envRefType,
    requestEnvironmentId,
    resolvedRequestEnvironment,
    projectCollections,
    loadProjectCollections,
  ]);

  const renderCollections = () => {
    if (!projectCollections.length) {
      return (
        <div className="p-6 text-center text-[var(--color-text-muted)] text-sm">
          {t('collection.noCollections')}
        </div>
      );
    }

    return (
      <div className="p-3 space-y-3">
        {projectCollections.map((collection) => (
          <div key={collection.id} className="rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-1)]">
            <div className="px-3 py-2 text-sm text-[var(--color-text-primary)] border-b border-[var(--color-surface-3)]">
              {collection.name}
            </div>
            <div className="divide-y divide-[var(--color-surface-3)]">
              {collection.items.map((item) => (
                <button
                  key={item.id}
                  onClick={() => handleLoadSavedRequest(item)}
                  className="w-full text-left px-3 py-2 hover:bg-[var(--color-surface-hover)] transition-colors"
                >
                  <div className="text-sm text-[var(--color-text-secondary)] truncate">{item.name}</div>
                  <div className="text-xs text-[var(--color-text-muted)] truncate">
                    {item.service}/{item.method}
                  </div>
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    );
  };

  const renderEnvironments = () => {
    return (
      <div className="p-3 space-y-3">
        <button
          onClick={() => {
            if (!currentProject) {
              setError(t('project.selectFirst'));
              return;
            }
            setEditingEnvironment(null);
            setIsEnvDialogOpen(true);
          }}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          disabled={!currentProject}
        >
          <Plus size={14} />
          {t('environment.createNew')}
        </button>

        {!projectEnvironments.length ? (
          <div className="p-6 text-center text-[var(--color-text-muted)] text-sm">
            {t('environment.noEnvironmentsPrompt')}
          </div>
        ) : (
          <div className="space-y-2">
            {projectEnvironments.map((env) => (
              <div
                key={env.id}
                onClick={() => handleConnectionEnvironmentChange(env.id)}
                className={cn(
                  'rounded border p-3 cursor-pointer transition-colors',
                  env.id === activeEnvironmentId
                    ? 'border-[var(--color-primary)] bg-[var(--color-primary-soft-10)]'
                    : 'border-[var(--color-surface-3)] bg-[var(--color-surface-1)] hover:bg-[var(--color-surface-hover)]'
                )}
              >
                <div className="flex items-center justify-between gap-2">
                  <div>
                    <div className="text-sm text-[var(--color-text-primary)] flex items-center gap-2">
                      {env.name}
                      {env.isDefault && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300">
                          {t('environment.default')}
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-[var(--color-text-muted)] mt-1 font-mono">{env.baseUrl || '-'}</div>
                    <div className="text-[11px] text-[var(--color-text-muted)] mt-1">
                      {t('environment.variables')} {env.variables.length} · {t('environment.headers')} {Object.keys(env.metadata || {}).length}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={(event) => {
                        event.stopPropagation();
                        handleEditEnvironment(env);
                      }}
                      className="text-xs px-2 py-1 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-[var(--color-text-primary)]"
                    >
                      编辑
                    </button>
                    <button
                      onClick={(event) => {
                        event.stopPropagation();
                        void handleDeleteEnvironment(env);
                      }}
                      className="text-xs px-2 py-1 rounded bg-red-900/40 hover:bg-red-900/60 text-[var(--color-danger-text-weak)]"
                    >
                      删除
                    </button>
                    {currentProject && !env.isDefault && (
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          void setDefaultEnvironment(env.projectId || currentProject.id, env.id);
                        }}
                        className="text-xs px-2 py-1 rounded bg-[var(--color-info-soft)] hover:bg-[var(--color-info-soft-hover)] text-[var(--color-text-primary)]"
                      >
                        {t('environment.setAsDefault')}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  };

  const renderHistory = () => {
    if (!histories.length) {
      return <div className="p-6 text-center text-[var(--color-text-muted)] text-sm">{t('history.noHistory')}</div>;
    }

    return (
      <div className="p-3 space-y-2">
        <div className="flex justify-end">
          <button
            type="button"
            onClick={() => void handleClearHistories()}
            disabled={isClearingHistory || historyDeletingId !== null}
            className="text-xs px-2 py-1 rounded bg-red-900/40 hover:bg-red-900/60 disabled:opacity-50 disabled:cursor-not-allowed text-[var(--color-danger-text-weak)]"
          >
            {isClearingHistory ? t('history.clearing') : t('history.clearAll')}
          </button>
        </div>

        {histories.map((item) => (
          <div
            key={item.id}
            className="w-full text-left rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-1)] px-3 py-2 hover:bg-[var(--color-surface-hover)]"
          >
            <button
              type="button"
              onClick={() => handleLoadSavedRequest(item.requestSnapshot)}
              className="w-full text-left"
            >
              <div className="text-xs text-[var(--color-text-secondary)]">
                {new Date(item.timestamp).toLocaleString()}
              </div>
              <div className="text-sm text-[var(--color-text-primary)] truncate mt-1">
                {item.service}/{item.method}
              </div>
              <div className="text-xs text-[var(--color-text-muted)] mt-1 flex items-center justify-between">
                <span>{item.address}</span>
                <span>
                  {item.responseCode !== undefined ? 'code=' + item.responseCode + ' · ' : ''}
                  {item.duration} ms
                </span>
              </div>
            </button>

            <div className="mt-2 flex justify-end">
              <button
                type="button"
                onClick={(event) => {
                  event.stopPropagation();
                  void handleDeleteHistoryItem(item.id);
                }}
                disabled={isClearingHistory || historyDeletingId === item.id}
                className="text-xs px-2 py-1 rounded bg-red-900/40 hover:bg-red-900/60 disabled:opacity-50 disabled:cursor-not-allowed text-[var(--color-danger-text-weak)]"
              >
                {historyDeletingId === item.id ? t('history.deleting') : t('history.deleteOne')}
              </button>
            </div>
          </div>
        ))}
      </div>
    );
  };

  // Project Onboarding View - Show when no projects exist
  if (needsProjectOnboarding) {
    return (
      <div className="flex h-full bg-[var(--color-surface-0)] items-center justify-center">
        <div className="bg-[var(--color-surface-1)] border border-[var(--color-surface-3)] rounded-lg p-8 w-[480px]">
          <div className="text-center mb-8">
            <div className="w-16 h-16 rounded bg-gradient-to-br from-[var(--color-brand-start)] to-[var(--color-brand-end)] flex items-center justify-center mx-auto mb-4">
              <span className="text-2xl font-bold text-[var(--color-text-primary)]">g</span>
            </div>
            <h1 className="text-2xl font-bold text-[var(--color-text-primary)] mb-2">gRPC UI</h1>
            <p className="text-[var(--color-text-secondary)]">{t('onboarding.subtitle')}</p>
          </div>

          <div className="space-y-4">
            <button
              onClick={() => {
                setProjectDialogMode('create');
                setIsProjectDialogOpen(true);
              }}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded bg-[var(--color-primary)] hover:bg-[var(--color-primary-hover)] text-[var(--color-text-primary)] font-medium transition-colors"
            >
              <Plus size={20} />
              {t('onboarding.createFirstProject')}
            </button>

            <div className="text-center">
              <span className="text-[var(--color-text-muted)] text-sm">{t('onboarding.or')}</span>
            </div>

            <button
              onClick={() => setIsSettingsOpen(true)}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-[var(--color-text-primary)] font-medium transition-colors"
            >
              <Cog size={20} />
              {t('sidebar.settings')}
            </button>
          </div>
        </div>

        {/* Project Creation Dialog */}
        {isProjectDialogOpen && (
          <ProjectDialog
            mode="create"
            onClose={() => setIsProjectDialogOpen(false)}
            onCreate={handleProjectCreate}
            onClone={handleProjectClone}
          />
        )}

        {/* Settings Dialog */}
        {isSettingsOpen && (
          <SettingsDialog
            onClose={() => setIsSettingsOpen(false)}
            themeMode={themeMode}
            onThemeModeChange={handleThemeModeChange}
          />
        )}
      </div>
    );
  }

  // Project Selection View - Show when projects exist but none selected
  if (!currentProject && projects.length > 0) {
    return (
      <div className="flex h-full bg-[var(--color-surface-0)] items-center justify-center">
        <div className="bg-[var(--color-surface-1)] border border-[var(--color-surface-3)] rounded-lg p-8 w-[480px]">
          <div className="text-center mb-6">
            <h2 className="text-xl font-bold text-[var(--color-text-primary)] mb-2">{t('onboarding.selectProject')}</h2>
            <p className="text-[var(--color-text-secondary)]">{t('onboarding.selectProjectDesc')}</p>
          </div>

          <div className="space-y-2 max-h-[300px] overflow-auto mb-6">
            {projects.map((project) => (
              <button
                key={project.id}
                onClick={() => handleProjectSelect(project.id)}
                className="w-full text-left p-4 rounded bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] hover:border-[var(--color-primary)] hover:bg-[var(--color-surface-hover)] transition-colors"
              >
                <div className="font-medium text-[var(--color-text-primary)]">{project.name}</div>
                {project.description && (
                  <div className="text-sm text-[var(--color-text-muted)] mt-1">{project.description}</div>
                )}
              </button>
            ))}
          </div>

          <div className="border-t border-[var(--color-surface-3)] pt-4">
            <button
              onClick={() => {
                setProjectDialogMode('create');
                setIsProjectDialogOpen(true);
              }}
              className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-[var(--color-text-primary)] text-sm transition-colors"
            >
              <Plus size={16} />
              {t('project.create')}
            </button>
          </div>
        </div>

        {/* Project Creation Dialog */}
        {isProjectDialogOpen && (
          <ProjectDialog
            mode="create"
            onClose={() => setIsProjectDialogOpen(false)}
            onCreate={handleProjectCreate}
            onClone={handleProjectClone}
          />
        )}
      </div>
    );
  }

  // Main Application View - Show when project is selected
  return (
    <div className="flex h-full bg-[var(--color-surface-0)]">
      <div
        className={cn(
          'flex flex-col border-r border-[var(--color-surface-3)] bg-[var(--color-surface-1)] transition-all duration-200',
          sidebarCollapsed ? 'w-12' : 'w-72'
        )}
      >
        <div className="flex items-center h-12 px-3 border-b border-[var(--color-surface-3)]">
          {!sidebarCollapsed && (
            <>
              <div className="w-6 h-6 rounded bg-gradient-to-br from-[var(--color-brand-start)] to-[var(--color-brand-end)] flex items-center justify-center mr-2">
                <span className="text-xs font-bold text-[var(--color-text-primary)]">g</span>
              </div>
              <span className="font-semibold text-[var(--color-text-primary)]">gRPC UI</span>
            </>
          )}
          <button
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
            className="ml-auto p-1 hover:bg-[var(--color-surface-3)] rounded"
          >
            {sidebarCollapsed ? (
              <ChevronRight size={14} />
            ) : (
              <ChevronDown size={14} />
            )}
          </button>
        </div>

        {!sidebarCollapsed && (
          <ProjectSelector
            projects={projects}
            currentProjectId={currentProject?.id}
            isLoading={projectLoading}
            onSelect={handleProjectSelect}
            onCreate={openProjectCreateDialog}
            onClone={openProjectCloneDialog}
            onDelete={handleProjectDelete}
          />
        )}

        <div className="flex flex-col py-2">
          <SidebarButton
            icon={<Globe size={18} />}
            label={t('sidebar.services')}
            active={activeTab === 'services'}
            onClick={() => setActiveTab('services')}
            collapsed={sidebarCollapsed}
          />
          <SidebarButton
            icon={<FolderOpen size={18} />}
            label={t('sidebar.collections')}
            active={activeTab === 'collections'}
            onClick={() => setActiveTab('collections')}
            collapsed={sidebarCollapsed}
          />
          <SidebarButton
            icon={<Settings size={18} />}
            label={t('sidebar.environments')}
            active={activeTab === 'environments'}
            onClick={() => setActiveTab('environments')}
            collapsed={sidebarCollapsed}
          />
          <SidebarButton
            icon={<History size={18} />}
            label={t('sidebar.history')}
            active={activeTab === 'history'}
            onClick={() => setActiveTab('history')}
            collapsed={sidebarCollapsed}
          />
        </div>

        {/* Settings Button at Bottom */}
        <div className="mt-auto py-2 border-t border-[var(--color-surface-3)]">
          <SidebarButton
            icon={<Cog size={18} />}
            label={t('sidebar.settings')}
            active={isSettingsOpen}
            onClick={() => setIsSettingsOpen(true)}
            collapsed={sidebarCollapsed}
          />
        </div>
      </div>

      <div className="flex-1 flex flex-col min-w-0" key={currentProject?.id || 'no-project'}>
        <ConnectionBar
          address={address}
          connectionState={connectionState}
          environments={projectEnvironments}
          selectedEnvironmentId={activeEnvironmentId || ''}
          showConnectionAction={selectedMethod?.type === 'bidi_stream'}
          onAddressChange={setAddress}
          onConnect={handleConnect}
          onDisconnect={handleDisconnect}
          onOpenImportDialog={openProtoImportDialog}
          onEnvironmentChange={handleConnectionEnvironmentChange}
          error={error || projectError || undefined}
        />

        <div className="flex-1 flex overflow-hidden">
          <div className="w-72 border-r border-[var(--color-surface-3)] bg-[var(--color-surface-2)] overflow-auto">
            {activeTab === 'services' && (
              <ServiceTree
                services={services}
                selectedMethod={selectedMethod?.method}
                groupRootPath={protoGroupRoot}
                onMethodSelect={handleMethodSelect}
                onDeleteFolder={handleDeleteFolderFromServices}
                onDeleteService={handleDeleteServiceFromServices}
                onDeleteMethod={handleDeleteMethodFromServices}
              />
            )}
            {activeTab === 'collections' && renderCollections()}
            {activeTab === 'environments' && renderEnvironments()}
            {activeTab === 'history' && renderHistory()}
          </div>

          <div className="flex-1 flex flex-col min-w-0">
            <RequestPanel
              selectedMethod={selectedMethod}
              methodInputSchema={methodInputSchema}
              body={requestBody}
              metadata={metadata}
              project={currentProject}
              environments={projectEnvironments}
              envRefType={envRefType}
              selectedEnvironmentId={requestEnvironmentId}
              onBodyChange={setRequestBody}
              onMetadataChange={setMetadata}
              onEnvRefChange={handleEnvRefChange}
              onSend={handleSend}
              isStreamConnected={grpcStream.isConnected}
              isStreamInputClosed={streamInputClosed}
              onEndStream={handleEndStream}
              onCloseStream={handleCloseStream}
              onSave={handleSave}
              isLoading={isLoading}
            />
          </div>

          <div className="w-1/2 border-l border-[var(--color-surface-3)] flex flex-col">
            <ResponsePanel
              response={response}
              streamMessages={streamMessages}
              isStreaming={isStreaming}
            />
          </div>
        </div>
      </div>

      {/* Environment Creation Dialog */}
      {isEnvDialogOpen && (
        <EnvironmentDialog
          mode={editingEnvironment ? 'edit' : 'create'}
          initialEnvironment={editingEnvironment}
          onClose={() => {
            setIsEnvDialogOpen(false);
            setEditingEnvironment(null);
          }}
          onSubmit={handleSubmitEnvironment}
        />
      )}

      {/* Project Creation/Clone Dialog */}
      {isProjectDialogOpen && (
        <ProjectDialog
          mode={projectDialogMode}
          currentProjectName={currentProject?.name}
          onClose={() => setIsProjectDialogOpen(false)}
          onCreate={handleProjectCreate}
          onClone={handleProjectClone}
        />
      )}

      {/* Settings Dialog */}
      {isSettingsOpen && (
        <SettingsDialog
          onClose={() => setIsSettingsOpen(false)}
          themeMode={themeMode}
          onThemeModeChange={handleThemeModeChange}
        />
      )}

      {/* Proto Import Dialog */}
      {isProtoImportDialogOpen && (
        <ProtoImportDialog
          preview={protoImportPreview}
          error={protoImportError}
          isPreparingPreview={isPreparingProtoImportPreview}
          isImporting={isImportingProto}
          isDragActive={isProtoImportDragActive}
          onImportFile={handleSelectImportFile}
          onImportDirectory={handleSelectImportDirectory}
          onConfirmImport={handleConfirmProtoImport}
          onClose={closeProtoImportDialog}
        />
      )}
    </div>
  );
}

interface ProtoImportDialogProps {
  preview: ProtoImportPreview | null;
  error: string | null;
  isPreparingPreview: boolean;
  isImporting: boolean;
  isDragActive: boolean;
  onImportFile: () => void | Promise<void>;
  onImportDirectory: () => void | Promise<void>;
  onConfirmImport: () => void | Promise<void>;
  onClose: () => void;
}

// ProtoImportDialog 提供“统一入口 + 先预览后确认”的导入体验：
// - 顶部拖拽区提示用户可直接拖入文件/目录；
// - 左下角固定三个操作按钮（导入文件、导入目录、关闭）；
// - 右下角确认导入按钮仅在预览完成后可点击，确保用户先看结果再导入。
function ProtoImportDialog({
  preview,
  error,
  isPreparingPreview,
  isImporting,
  isDragActive,
  onImportFile,
  onImportDirectory,
  onConfirmImport,
  onClose,
}: ProtoImportDialogProps) {
  const { t } = useTranslation();
  const previewPaths = preview?.relativePaths ?? [];
  const visiblePreviewPaths = previewPaths.slice(0, 12);
  const hiddenPreviewCount = Math.max(previewPaths.length - visiblePreviewPaths.length, 0);

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
    >
      <div className="bg-[var(--color-surface-1)] border border-[var(--color-surface-3)] rounded-lg p-5 w-[760px] max-h-[80vh] flex flex-col gap-4">
        <div className="flex items-center justify-between">
          <h3 className="text-lg font-semibold text-[var(--color-text-primary)]">
            {t('connection.importDialogTitle')}
          </h3>
          <span className="text-xs text-[var(--color-text-muted)]">
            {t('connection.importDialogHint')}
          </span>
        </div>

        <div
          className={cn(
            'rounded-lg border border-dashed px-4 py-8 text-center transition-colors',
            isDragActive
              ? 'border-[var(--color-primary)] bg-[var(--color-surface-hover)]'
              : 'border-[var(--color-surface-4)] bg-[var(--color-surface-0)]'
          )}
        >
          <div className="flex items-center justify-center gap-2 text-[var(--color-text-primary)]">
            <Upload size={18} />
            <span className="text-sm font-medium">{t('connection.importDropAreaTitle')}</span>
          </div>
          <p className="mt-2 text-xs text-[var(--color-text-muted)]">
            {t('connection.importDropAreaSubtitle')}
          </p>
        </div>

        {isPreparingPreview && (
          <div className="rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-0)] px-3 py-2 text-xs text-[var(--color-text-secondary)]">
            {t('connection.importPreviewLoading')}
          </div>
        )}

        {error && (
          <div className="rounded border border-red-700 bg-red-900/30 px-3 py-2 text-xs text-[var(--color-danger-text)]">
            {error}
          </div>
        )}

        {preview ? (
          <div className="rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-0)] px-4 py-3 space-y-3 min-h-[180px]">
            <div className="flex items-center gap-2 text-sm text-[var(--color-text-primary)]">
              <FileCode2 size={16} />
              <span>{t('connection.importPreviewTitle')}</span>
            </div>

            <div className="grid grid-cols-2 gap-2 text-xs text-[var(--color-text-secondary)]">
              <div>
                {t('connection.importPreviewType')}：
                <span className="text-[var(--color-text-primary)] ml-1">
                  {preview.sourceType === 'directory'
                    ? t('connection.importTypeDirectory')
                    : t('connection.importTypeFile')}
                </span>
              </div>
              <div>
                {t('connection.importPreviewCount')}：
                <span className="text-[var(--color-text-primary)] ml-1">
                  {preview.relativePaths.length}
                </span>
              </div>
            </div>

            <div className="text-xs text-[var(--color-text-secondary)] break-all">
              {preview.sourceType === 'directory'
                ? t('connection.importPreviewRoot')
                : t('connection.importPreviewPath')}
              ：
              <span className="text-[var(--color-text-primary)] ml-1">{preview.sourcePath}</span>
            </div>

            <div className="rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-1)] p-2 max-h-40 overflow-auto text-xs text-[var(--color-text-secondary)] space-y-1">
              {visiblePreviewPaths.map((path) => (
                <p key={path} className="break-all">
                  {path}
                </p>
              ))}
              {hiddenPreviewCount > 0 && (
                <p className="text-[var(--color-text-muted)]">
                  {t('connection.importPreviewMore', { count: hiddenPreviewCount })}
                </p>
              )}
            </div>
          </div>
        ) : (
          <div className="rounded border border-[var(--color-surface-3)] bg-[var(--color-surface-0)] px-4 py-6 text-xs text-[var(--color-text-muted)]">
            {t('connection.importPreviewEmpty')}
          </div>
        )}

        <div className="flex items-center justify-between gap-3 pt-1">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void onImportFile()}
              className="px-3 py-1.5 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors"
            >
              {t('connection.importDialogImportFile')}
            </button>
            <button
              type="button"
              onClick={() => void onImportDirectory()}
              className="px-3 py-1.5 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors"
            >
              {t('connection.importDialogImportDirectory')}
            </button>
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors"
            >
              {t('common.close')}
            </button>
          </div>

          <button
            type="button"
            onClick={() => void onConfirmImport()}
            disabled={!preview || isPreparingPreview || isImporting}
            className="px-4 py-1.5 rounded bg-[var(--color-primary)] hover:bg-[var(--color-primary-hover)] disabled:opacity-50 disabled:cursor-not-allowed text-sm text-[var(--color-text-primary)] transition-colors"
          >
            {isImporting
              ? t('connection.importDialogImporting')
              : t('connection.importDialogConfirm')}
          </button>
        </div>
      </div>
    </div>
  );
}

interface EnvironmentDialogProps {
  mode: 'create' | 'edit';
  initialEnvironment: ProjectEnvironment | null;
  onClose: () => void;
  onSubmit: (formData: EnvironmentFormData) => void;
}

interface KeyValueDraft {
  id: string;
  key: string;
  value: string;
  secret?: boolean;
}

function EnvironmentDialog({ mode, initialEnvironment, onClose, onSubmit }: EnvironmentDialogProps) {
  const [name, setName] = useState(initialEnvironment?.name ?? '');
  const [baseUrl, setBaseUrl] = useState(initialEnvironment?.baseUrl ?? 'localhost:50051');
  const [variables, setVariables] = useState<KeyValueDraft[]>(() =>
    (initialEnvironment?.variables ?? []).map((variable) => ({
      id: crypto.randomUUID(),
      key: variable.key,
      value: variable.value,
      secret: variable.secret,
    }))
  );
  const [headers, setHeaders] = useState<KeyValueDraft[]>(() =>
    Object.entries(initialEnvironment?.metadata ?? {}).map(([key, value]) => ({
      id: crypto.randomUUID(),
      key,
      value,
    }))
  );
  const [tlsDraft, setTlsDraft] = useState<TlsDraft>(() =>
    buildTlsDraftFromConfig(initialEnvironment?.tls)
  );
  const [tlsError, setTlsError] = useState('');

  // syncEnvironmentDialogState 负责在切换编辑目标时重置表单，
  // 防止上一次编辑残留到下一次环境创建或编辑。
  useEffect(() => {
    setName(initialEnvironment?.name ?? '');
    setBaseUrl(initialEnvironment?.baseUrl ?? 'localhost:50051');
    setVariables(
      (initialEnvironment?.variables ?? []).map((variable) => ({
        id: crypto.randomUUID(),
        key: variable.key,
        value: variable.value,
        secret: variable.secret,
      }))
    );
    setHeaders(
      Object.entries(initialEnvironment?.metadata ?? {}).map(([key, value]) => ({
        id: crypto.randomUUID(),
        key,
        value,
      }))
    );
    setTlsDraft(buildTlsDraftFromConfig(initialEnvironment?.tls));
    setTlsError('');
  }, [initialEnvironment, mode]);

  // updateTlsDraft 负责按 patch 更新 TLS 草稿，
  // 便于在多个开关与路径输入之间共享同一份状态更新逻辑。
  const updateTlsDraft = (patch: Partial<TlsDraft>) => {
    setTlsDraft((prev) => ({ ...prev, ...patch }));
  };

  // addVariableRow 负责追加一个变量输入行，
  // 支持用户在环境中维护多个可替换占位符。
  const addVariableRow = () => {
    setVariables((prev) => [
      ...prev,
      { id: crypto.randomUUID(), key: '', value: '', secret: false },
    ]);
  };

  // updateVariableRow 负责按行更新变量 key/value/secret，
  // 避免多行编辑时互相覆盖，保证表单状态稳定。
  const updateVariableRow = (
    rowId: string,
    patch: Partial<Pick<KeyValueDraft, 'key' | 'value' | 'secret'>>
  ) => {
    setVariables((prev) => prev.map((row) => (row.id === rowId ? { ...row, ...patch } : row)));
  };

  // removeVariableRow 负责删除指定变量行，
  // 用于清理误填或不再需要的环境变量。
  const removeVariableRow = (rowId: string) => {
    setVariables((prev) => prev.filter((row) => row.id !== rowId));
  };

  // addHeaderRow 负责追加一个请求头输入行，
  // 让环境可统一注入鉴权 token 或租户标识等头部信息。
  const addHeaderRow = () => {
    setHeaders((prev) => [...prev, { id: crypto.randomUUID(), key: '', value: '' }]);
  };

  // updateHeaderRow 负责按行更新请求头键值，
  // 保证多请求头编辑时每一行都能独立变更。
  const updateHeaderRow = (
    rowId: string,
    patch: Partial<Pick<KeyValueDraft, 'key' | 'value'>>
  ) => {
    setHeaders((prev) => prev.map((row) => (row.id === rowId ? { ...row, ...patch } : row)));
  };

  // removeHeaderRow 负责删除指定请求头行，
  // 便于快速移除过期头部配置。
  const removeHeaderRow = (rowId: string) => {
    setHeaders((prev) => prev.filter((row) => row.id !== rowId));
  };

  // buildVariablesFromDraft 将变量草稿转换成可持久化模型，
  // 自动过滤空 key，避免无效变量污染环境配置。
  const buildVariablesFromDraft = (): Variable[] => {
    return variables
      .map((row) => ({
        key: row.key.trim(),
        value: row.value,
        secret: Boolean(row.secret),
      }))
      .filter((row) => row.key.length > 0);
  };

  // buildHeadersFromDraft 将请求头草稿折叠为对象结构，
  // key 冲突时后者覆盖前者，符合用户最后一次输入直觉。
  const buildHeadersFromDraft = (): Record<string, string> => {
    return headers.reduce<Record<string, string>>((acc, row) => {
      const headerKey = row.key.trim();
      if (!headerKey) {
        return acc;
      }

      acc[headerKey] = row.value;
      return acc;
    }, {});
  };

  // handleSubmit 负责统一校验并提交环境表单，
  // 让地址、变量和请求头一次性保存到项目环境中。
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      return;
    }

    const validationError = validateTlsDraft(tlsDraft);
    if (validationError) {
      setTlsError(validationError);
      return;
    }

    setTlsError('');

    onSubmit({
      name: name.trim(),
      baseUrl: baseUrl.trim(),
      variables: buildVariablesFromDraft(),
      metadata: buildHeadersFromDraft(),
      tls: buildTlsConfigFromDraft(tlsDraft),
    });
  };

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="bg-[var(--color-surface-1)] border border-[var(--color-surface-3)] rounded-lg p-6 w-[560px] max-h-[82vh] overflow-y-auto">
        <h3 className="text-lg font-semibold text-[var(--color-text-primary)] mb-4">{mode === 'edit' ? '编辑环境' : '新建环境'}</h3>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-[var(--color-text-secondary)] mb-1">环境名称</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如: 开发环境"
              className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-3 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
              autoFocus
            />
          </div>

          <div>
            <label className="block text-sm text-[var(--color-text-secondary)] mb-1">服务器地址</label>
            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="localhost:50051"
              className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-3 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
            />
          </div>

          <div>
            <label className="block text-sm text-[var(--color-text-secondary)] mb-1">Server Name / Authority（可选）</label>
            <input
              type="text"
              value={tlsDraft.authority}
              onChange={(e) => updateTlsDraft({ authority: e.target.value })}
              placeholder="例如: api.example.com"
              className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-3 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
            />
            <p className="mt-1 text-xs text-[var(--color-text-tertiary)]">
              等价于 openssl 的 -servername，用于 TLS SNI，并同步作为 HTTP/2 :authority。
            </p>
          </div>

          <div className="rounded border border-[var(--color-surface-3)] p-3 space-y-3">
            <p className="text-sm text-[var(--color-text-secondary)]">连接安全</p>

            <label className="text-xs text-[var(--color-text-secondary)] flex items-center gap-2">
              <input
                type="checkbox"
                checked={tlsDraft.useGrpcs}
                onChange={(e) => {
                  const enabled = e.target.checked;
                  updateTlsDraft({
                    useGrpcs: enabled,
                    useMtls: enabled ? tlsDraft.useMtls : false,
                  });
                  setTlsError('');
                }}
              />
              使用 grpcs（TLS）连接
            </label>

            {tlsDraft.useGrpcs && (
              <>
                <label className="text-xs text-[var(--color-text-secondary)] flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={tlsDraft.useMtls}
                    onChange={(e) => {
                      updateTlsDraft({ useMtls: e.target.checked });
                      setTlsError('');
                    }}
                  />
                  启用 mTLS（双向证书）
                </label>

                <div>
                  <label className="block text-xs text-[var(--color-text-secondary)] mb-1">信任 CA 证书路径（可选）</label>
                  <input
                    type="text"
                    value={tlsDraft.caCertPath}
                    onChange={(e) => updateTlsDraft({ caCertPath: e.target.value })}
                    placeholder="例如: /etc/ssl/my-ca.pem"
                    className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                  />
                </div>

                {tlsDraft.useMtls && (
                  <div className="grid grid-cols-1 gap-2">
                    <div>
                      <label className="block text-xs text-[var(--color-text-secondary)] mb-1">Client Cert 路径</label>
                      <input
                        type="text"
                        value={tlsDraft.clientCertPath}
                        onChange={(e) => {
                          updateTlsDraft({ clientCertPath: e.target.value });
                          setTlsError('');
                        }}
                        placeholder="例如: /etc/ssl/client.crt"
                        className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-[var(--color-text-secondary)] mb-1">Client Key 路径</label>
                      <input
                        type="text"
                        value={tlsDraft.clientKeyPath}
                        onChange={(e) => {
                          updateTlsDraft({ clientKeyPath: e.target.value });
                          setTlsError('');
                        }}
                        placeholder="例如: /etc/ssl/client.key"
                        className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                      />
                    </div>
                  </div>
                )}

                <label className="text-xs text-[var(--color-text-secondary)] flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={tlsDraft.skipVerify}
                    onChange={(e) => updateTlsDraft({ skipVerify: e.target.checked })}
                  />
                  跳过服务端证书校验（仅测试环境建议使用）
                </label>
              </>
            )}

            {tlsError && <p className="text-xs text-[var(--color-danger-text)]">{tlsError}</p>}
          </div>

          <div className="rounded border border-[var(--color-surface-3)] p-3 space-y-3">
            <div className="flex items-center justify-between">
              <p className="text-sm text-[var(--color-text-secondary)]">环境变量</p>
              <button
                type="button"
                onClick={addVariableRow}
                className="text-xs px-2 py-1 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-[var(--color-text-primary)]"
              >
                添加变量
              </button>
            </div>

            {!variables.length ? (
              <p className="text-xs text-[var(--color-text-muted)]">暂无变量，发送请求时不会做变量替换。</p>
            ) : (
              <div className="space-y-2">
                {variables.map((row) => (
                  <div key={row.id} className="rounded border border-[var(--color-surface-3)] p-2 space-y-2">
                    <div className="grid grid-cols-2 gap-2">
                      <input
                        type="text"
                        value={row.key}
                        onChange={(e) => updateVariableRow(row.id, { key: e.target.value })}
                        placeholder="变量名，例如 host"
                        className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                      />
                      <input
                        type="text"
                        value={row.value}
                        onChange={(e) => updateVariableRow(row.id, { value: e.target.value })}
                        placeholder="变量值"
                        className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                      />
                    </div>
                    <div className="flex items-center justify-between">
                      <label className="text-xs text-[var(--color-text-secondary)] flex items-center gap-2">
                        <input
                          type="checkbox"
                          checked={Boolean(row.secret)}
                          onChange={(e) => updateVariableRow(row.id, { secret: e.target.checked })}
                        />
                        敏感变量
                      </label>
                      <button
                        type="button"
                        onClick={() => removeVariableRow(row.id)}
                        className="text-xs text-[var(--color-danger-text)] hover:text-[var(--color-danger-text)]"
                      >
                        删除
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="rounded border border-[var(--color-surface-3)] p-3 space-y-3">
            <div className="flex items-center justify-between">
              <p className="text-sm text-[var(--color-text-secondary)]">默认请求头</p>
              <button
                type="button"
                onClick={addHeaderRow}
                className="text-xs px-2 py-1 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-[var(--color-text-primary)]"
              >
                添加请求头
              </button>
            </div>

            {!headers.length ? (
              <p className="text-xs text-[var(--color-text-muted)]">暂无默认请求头。</p>
            ) : (
              <div className="space-y-2">
                {headers.map((row) => (
                  <div key={row.id} className="rounded border border-[var(--color-surface-3)] p-2">
                    <div className="grid grid-cols-[1fr_1fr_auto] gap-2 items-center">
                      <input
                        type="text"
                        value={row.key}
                        onChange={(e) => updateHeaderRow(row.id, { key: e.target.value })}
                        placeholder="Header 名，例如 authorization"
                        className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                      />
                      <input
                        type="text"
                        value={row.value}
                        onChange={(e) => updateHeaderRow(row.id, { value: e.target.value })}
                        placeholder="Header 值"
                        className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-2 py-1.5 text-xs text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
                      />
                      <button
                        type="button"
                        onClick={() => removeHeaderRow(row.id)}
                        className="text-xs text-[var(--color-danger-text)] hover:text-[var(--color-danger-text)]"
                      >
                        删除
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="flex gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="flex-1 px-3 py-2 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors"
            >
              取消
            </button>
            <button
              type="submit"
              disabled={!name.trim()}
              className="flex-1 px-3 py-2 rounded bg-[var(--color-primary)] hover:bg-[var(--color-primary-hover)] disabled:opacity-50 disabled:cursor-not-allowed text-sm text-[var(--color-text-primary)] transition-colors"
            >
              {mode === 'edit' ? '保存' : '创建'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

interface ProjectDialogProps {
  mode: 'create' | 'clone';
  currentProjectName?: string;
  onClose: () => void;
  onCreate: (name: string) => void;
  onClone: (name: string) => void;
}

function ProjectDialog({ mode, currentProjectName, onClose, onCreate, onClone }: ProjectDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(mode === 'clone' ? `${currentProjectName || ''}-副本` : '');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (name.trim()) {
      if (mode === 'create') {
        onCreate(name.trim());
      } else {
        onClone(name.trim());
      }
    }
  };

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="bg-[var(--color-surface-1)] border border-[var(--color-surface-3)] rounded-lg p-6 w-96">
        <h3 className="text-lg font-semibold text-[var(--color-text-primary)] mb-4">
          {mode === 'create' ? t('project.create') : t('project.clone')}
        </h3>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm text-[var(--color-text-secondary)] mb-1">
              {t('project.name')}
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={mode === 'create' ? t('project.namePlaceholder') : t('project.cloneNamePlaceholder')}
              className="w-full bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] rounded px-3 py-2 text-sm text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-primary)]"
              autoFocus
            />
          </div>
          <div className="flex gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="flex-1 px-3 py-2 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors"
            >
              {t('common.cancel')}
            </button>
            <button
              type="submit"
              disabled={!name.trim()}
              className="flex-1 px-3 py-2 rounded bg-[var(--color-primary)] hover:bg-[var(--color-primary-hover)] disabled:opacity-50 disabled:cursor-not-allowed text-sm text-[var(--color-text-primary)] transition-colors"
            >
              {mode === 'create' ? t('common.create') : t('common.clone')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

interface SettingsDialogProps {
  onClose: () => void;
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
}

function SettingsDialog({ onClose, themeMode, onThemeModeChange }: SettingsDialogProps) {
  const { t, i18n } = useTranslation();
  const currentLang = i18n.language;

  // handleThemeChange 负责转发主题模式选择，统一由上层 App 管理全局主题状态，
  // 避免弹窗关闭后主题设置丢失或出现局部状态不一致。
  const handleThemeChange = (mode: ThemeMode) => {
    onThemeModeChange(mode);
  };

  const handleLanguageChange = (lang: string) => {
    i18n.changeLanguage(lang);
  };

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="bg-[var(--color-surface-1)] border border-[var(--color-surface-3)] rounded-lg p-6 w-[480px] max-h-[80vh] overflow-auto">
        <h3 className="text-lg font-semibold text-[var(--color-text-primary)] mb-6">{t('settings.title')}</h3>

        {/* Theme Section */}
        <div className="mb-6">
          <h4 className="text-sm font-medium text-[var(--color-text-secondary)] mb-3">{t('settings.theme')}</h4>
          <div className="space-y-2">
            <label className="flex items-center gap-3 p-3 rounded bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] cursor-pointer hover:border-[var(--color-surface-4)] transition-colors">
              <input
                type="radio"
                name="theme"
                value="system"
                checked={themeMode === 'system'}
                onChange={() => handleThemeChange('system')}
                className="w-4 h-4 accent-[var(--color-primary)]"
              />
              <span className="text-sm text-[var(--color-text-primary)]">{t('settings.themeSystem')}</span>
            </label>
            <label className="flex items-center gap-3 p-3 rounded bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] cursor-pointer hover:border-[var(--color-surface-4)] transition-colors">
              <input
                type="radio"
                name="theme"
                value="dark"
                checked={themeMode === 'dark'}
                onChange={() => handleThemeChange('dark')}
                className="w-4 h-4 accent-[var(--color-primary)]"
              />
              <span className="text-sm text-[var(--color-text-primary)]">{t('settings.themeDark')}</span>
            </label>
            <label className="flex items-center gap-3 p-3 rounded bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] cursor-pointer hover:border-[var(--color-surface-4)] transition-colors">
              <input
                type="radio"
                name="theme"
                value="light"
                checked={themeMode === 'light'}
                onChange={() => handleThemeChange('light')}
                className="w-4 h-4 accent-[var(--color-primary)]"
              />
              <span className="text-sm text-[var(--color-text-primary)]">{t('settings.themeLight')}</span>
            </label>
          </div>
        </div>

        {/* Language Section */}
        <div className="mb-6">
          <h4 className="text-sm font-medium text-[var(--color-text-secondary)] mb-3">{t('settings.language')}</h4>
          <div className="space-y-2">
            <label className="flex items-center gap-3 p-3 rounded bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] cursor-pointer hover:border-[var(--color-surface-4)] transition-colors">
              <input
                type="radio"
                name="language"
                value="zh"
                checked={currentLang.startsWith('zh')}
                onChange={() => handleLanguageChange('zh')}
                className="w-4 h-4 accent-[var(--color-primary)]"
              />
              <span className="text-sm text-[var(--color-text-primary)]">简体中文</span>
            </label>
            <label className="flex items-center gap-3 p-3 rounded bg-[var(--color-surface-0)] border border-[var(--color-surface-3)] cursor-pointer hover:border-[var(--color-surface-4)] transition-colors">
              <input
                type="radio"
                name="language"
                value="en"
                checked={currentLang.startsWith('en')}
                onChange={() => handleLanguageChange('en')}
                className="w-4 h-4 accent-[var(--color-primary)]"
              />
              <span className="text-sm text-[var(--color-text-primary)]">English</span>
            </label>
          </div>
        </div>

        {/* About Section */}
        <div className="border-t border-[var(--color-surface-3)] pt-4">
          <h4 className="text-sm font-medium text-[var(--color-text-secondary)] mb-2">{t('settings.about')}</h4>
          <p className="text-xs text-[var(--color-text-muted)]">
            gRPC UI v0.1.0
          </p>
        </div>

        {/* Close Button */}
        <div className="mt-6 flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-2 rounded bg-[var(--color-surface-3)] hover:bg-[var(--color-surface-4)] text-sm text-[var(--color-text-primary)] transition-colors"
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>
  );
}

interface SidebarButtonProps {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
  collapsed: boolean;
}

function SidebarButton({
  icon,
  label,
  active,
  onClick,
  collapsed,
}: SidebarButtonProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'flex items-center px-3 py-2 mx-2 rounded transition-colors',
        active
          ? 'bg-[var(--color-primary-soft)] text-[var(--color-primary)]'
          : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-3)] hover:text-[var(--color-text-primary)]',
        collapsed && 'justify-center px-2 mx-1'
      )}
      title={collapsed ? label : undefined}
    >
      {icon}
      {!collapsed && <span className="ml-3 text-sm">{label}</span>}
    </button>
  );
}

export default App;
