// ============ Project Types ============

export interface Project {
  id: string;
  name: string;
  description: string;
  defaultEnvironmentId?: string;
  protoFiles?: string[];
  createdAt?: string;
  updatedAt?: string;
}

export interface CreateProjectData {
  name: string;
  description?: string;
  defaultEnvironmentId?: string;
}

export interface UpdateProjectData {
  name?: string;
  description?: string;
  defaultEnvironmentId?: string;
  protoFiles?: string[];
}

// ============ Environment Types (Extended) ============

export interface ProjectEnvironment {
  id: string;
  projectId: string;
  name: string;
  baseUrl: string;
  tls: TLSConfig;
  metadata: Record<string, string>;
  variables: Variable[];
  isDefault: boolean;
  createdAt?: string;
  updatedAt?: string;
}

export interface TLSConfig {
  mode: 'insecure' | 'system' | 'custom';
  authority?: string;
  caCert?: string;
  clientCert?: string;
  clientKey?: string;
  skipVerify?: boolean;
}

export interface Variable {
  key: string;
  value: string;
  secret: boolean;
}

// ============ Collection Types (Extended) ============

export interface ProjectCollection {
  id: string;
  projectId: string;
  name: string;
  folders: Folder[];
  items: RequestItem[];
  createdAt: string;
  updatedAt: string;
}

export interface Folder {
  id: string;
  name: string;
  items: RequestItem[];
}

export type EnvRefType = 'inherit' | 'specific' | 'none';

export interface RequestItem {
  id: string;
  name: string;
  type: 'unary' | 'server_stream' | 'client_stream' | 'bidi_stream';
  service: string;
  method: string;
  body: string;
  metadata: Record<string, string>;
  envRefType: EnvRefType;
  environmentId?: string;
}
