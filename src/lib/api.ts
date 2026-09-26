import { invoke } from '@tauri-apps/api/core'

export type ServerStatus = {
  online: number
  max: number
  motd: string
  latencyMs: number
}

export type Target = {
  channel: string
  profilePath: string
  profileName: string
}

export type PackStatus = {
  target: Target
  version: string | null
  published: string | null
  notes: string[]
  state: 'unpublished' | 'install' | 'update' | 'ready'
  downloadBytes: number
  downloadFiles: number
  profileExists: boolean
}

export type LunarState = 'missing' | 'neverOpened' | 'ready'

export type SyncProgress = {
  file: string
  index: number
  total: number
  bytesDone: number
  bytesTotal: number
}

export type PlayOutcome = 'launched' | 'openedLunar' | 'alreadyRunning'

export type LauncherSettings = {
  version: string
  developer: boolean
  admin: boolean
  target: Target
}

export type Repository = {
  name: string
  tracked: boolean
  branch: string | null
  owned: boolean
  changes: string[]
  changeCount: number
  unpushed: number
}

export type PackPreview = {
  version: string
  nothing: boolean
  added: string[]
  updated: string[]
  removed: string[]
  blocked: string[]
  blockedChanged: boolean
  uploadFiles: number
  uploadBytes: number
  devVersion: string | null
  prodVersion: string | null
}

export type ServerName = 'prod' | 'dev'

export type ServerInfo = {
  state: 'running' | 'offline' | 'starting' | 'stopping' | 'unknown'
  online: number | null
  max: number | null
  memoryMb: number | null
  memoryLimitMb: number | null
  version: string | null
  deployedAt: string | null
  backups: number
  packVersion: string | null
}

export type UpdateCheck = {
  current: string
  available: string | null
}

export type UpdateProgress = {
  done: number
  total: number
}

export type LauncherRelease = {
  source: string | null
  online: string | null
  next: string | null
  unreleased: number | null
  keyFound: boolean
}

export async function getServerStatus(): Promise<ServerStatus | null> {
  try {
    return await invoke<ServerStatus>('server_status')
  } catch {
    return null
  }
}

export const getLunarState = () => invoke<LunarState>('lunar_state')
export const openLunar = () => invoke<void>('open_lunar')
export const downloadLunar = () => invoke<void>('download_lunar')

export const getSettings = () => invoke<LauncherSettings>('get_settings')
export const setDeveloper = (enabled: boolean) => invoke<LauncherSettings>('set_developer', { enabled })
export const setDevChannel = (enabled: boolean) => invoke<LauncherSettings>('set_dev_channel', { enabled })
export const getPackStatus = () => invoke<PackStatus>('pack_status')
export const getRepositories = () => invoke<Repository[]>('repositories')
export const commitAndPush = (name: string, message: string) =>
  invoke<string>('commit_and_push', { name, message })
export const getPackPreview = () => invoke<PackPreview>('pack_preview')
export const publishPack = (notes: string[]) => invoke<void>('publish_pack', { notes })
export const promotePack = () => invoke<void>('promote_pack')
export const getServersStatus = () => invoke<Record<ServerName, ServerInfo>>('servers_status')
export const serverPower = (server: ServerName, action: 'start' | 'stop' | 'restart', force: boolean) =>
  invoke<void>('server_power', { server, action, force })
export const serverDeploy = (server: ServerName, force: boolean) => invoke<void>('server_deploy', { server, force })
export const serverRollback = (server: ServerName, force: boolean) =>
  invoke<void>('server_rollback', { server, force })
export const checkUpdate = () => invoke<UpdateCheck>('check_update')
export const installUpdate = () => invoke<void>('install_update')
export const getLauncherRelease = () => invoke<LauncherRelease>('launcher_release')
export const publishLauncher = (version: string) => invoke<void>('publish_launcher', { version })
export const play = () => invoke<PlayOutcome>('play')

export const megabytes = (bytes: number) =>
  `${(bytes / 1_000_000).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} Mo`
