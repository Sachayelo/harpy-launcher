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
}

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
  keyFound: boolean
}

export async function getServerStatus(): Promise<ServerStatus | null> {
  try {
    return await invoke<ServerStatus>('server_status')
  } catch {
    return null
  }
}

export async function findLunar(): Promise<string | null> {
  try {
    return await invoke<string | null>('find_lunar')
  } catch {
    return null
  }
}

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
export const checkUpdate = () => invoke<UpdateCheck>('check_update')
export const installUpdate = () => invoke<void>('install_update')
export const getLauncherRelease = () => invoke<LauncherRelease>('launcher_release')
export const publishLauncher = (version: string) => invoke<void>('publish_launcher', { version })
export const syncPack = () => invoke<void>('sync_pack')
export const play = () => invoke<PlayOutcome>('play')

export const megabytes = (bytes: number) =>
  `${(bytes / 1_000_000).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} Mo`
