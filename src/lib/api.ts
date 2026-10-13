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

export type ChangeKind = 'new' | 'balance' | 'fix'

export type Change = {
  kind: ChangeKind
  text: string
}

export type Release = {
  version: string
  published: string
  title?: string | null
  image?: string | null
  changes: Change[]
}

export type PackStatus = {
  target: Target
  version: string | null
  published: string | null
  release: Release | null
  history: Release[]
  state: 'unpublished' | 'install' | 'update' | 'ready'
  downloadBytes: number
  downloadFiles: number
  profileExists: boolean
}

export type LunarState = 'missing' | 'neverOpened' | 'ready'

export type LauncherKind = 'lunar' | 'official' | 'prism' | 'modrinth' | 'curseforge' | 'custom'

export type LauncherState = {
  kind: LauncherKind
  name: string
  setup: 'missing' | 'neverOpened' | 'needsProfile' | 'ready'
  gameDir: string | null
  syncsItself: boolean
}

export type LauncherInfo = {
  kind: LauncherKind
  name: string
  installed: boolean
}

export type ProfileImport = {
  opened: boolean
  path: string
}

export type Crash = {
  kind: 'game' | 'java' | 'loader'
  path: string
  file: string
  at: number
  summary: string
  suspects: string[]
  hints: string[]
  extraMods: string[]
}

export type Announcement = {
  id: string
  kind: 'info' | 'event' | 'maintenance'
  title: string
  text: string
  until?: string | null
  audience: 'all' | 'prod' | 'dev'
  url?: string | null
  image?: string | null
}

export type Live = {
  channel: string
  url: string
  preview: string
}

export type News = {
  announcements: Announcement[]
  live: Live[]
}

export type NewsFile = {
  updated?: string | null
  twitch: string[]
  announcements: Announcement[]
}

export type ReleaseNotes = {
  title: string
  image: string
  changes: Change[]
}

export type PromotePreview = {
  devVersion: string
  prodVersion: string | null
  nothing: boolean
  pending: Release[]
}

export type StagedImage = {
  url: string
  preview: string
}

export type SyncProgress = {
  file: string
  index: number
  total: number
  bytesDone: number
  bytesTotal: number
}

export type PlayOutcome = 'launched' | 'opened' | 'alreadyRunning' | 'synced'

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

export const getLauncherState = () => invoke<LauncherState>('launcher_state')
export const getLaunchers = () => invoke<LauncherInfo[]>('launchers')
export const setLauncher = (kind: LauncherKind) => invoke<LauncherState>('set_launcher', { kind })
export const downloadLauncher = (kind: LauncherKind) => invoke<void>('download_launcher', { kind })
export const chooseFolder = (kind: LauncherKind) => invoke<LauncherState | null>('choose_folder', { kind })
export const importProfile = () => invoke<ProfileImport>('import_profile')
export const getCrash = () => invoke<Crash | null>('crash_status')
export const dismissCrash = () => invoke<void>('crash_dismiss')
export const shareLog = (path: string | null) => invoke<string>('share_log', { path })
export const repair = (disableExtra: boolean) => invoke<{ moved: number }>('repair', { disableExtra })
export const openGameFolder = () => invoke<void>('open_game_folder')
export const getNews = () => invoke<News>('news')
export const openLink = (url: string) => invoke<void>('open_link', { url })

export const getSettings = () => invoke<LauncherSettings>('get_settings')
export const setDeveloper = (enabled: boolean) => invoke<LauncherSettings>('set_developer', { enabled })
export const setDevChannel = (enabled: boolean) => invoke<LauncherSettings>('set_dev_channel', { enabled })
export const getPackStatus = () => invoke<PackStatus>('pack_status')
export const getRepositories = () => invoke<Repository[]>('repositories')
export const commitAndPush = (name: string, message: string) =>
  invoke<string>('commit_and_push', { name, message })
export const getPackPreview = () => invoke<PackPreview>('pack_preview')
export const publishPack = (notes: ReleaseNotes) => invoke<void>('publish_pack', { notes })
export const getPromotePreview = () => invoke<PromotePreview>('promote_preview')
export const promotePack = (notes: ReleaseNotes) => invoke<void>('promote_pack', { notes })
export const getNewsDraft = () => invoke<NewsFile>('news_draft')
export const publishNews = (news: NewsFile) => invoke<void>('publish_news', { news })
export const stageNewsImage = () => invoke<StagedImage | null>('stage_news_image')
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

export const CHANGE_KINDS: { kind: ChangeKind; label: string; title: string }[] = [
  { kind: 'new', label: 'Nouveau', title: 'Nouveautés' },
  { kind: 'balance', label: 'Équilibrage', title: 'Équilibrage' },
  { kind: 'fix', label: 'Correctif', title: 'Correctifs' },
]

export const longDate = (iso: string) =>
  new Date(iso).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' })

/** What several dev publications add up to, without repeats: by category, then oldest first. */
export function mergeChanges(releases: Release[]): Change[] {
  const seen = new Set<string>()
  const merged: Change[] = []
  for (const release of [...releases].reverse()) {
    for (const change of release.changes ?? []) {
      const key = change.text.trim().toLowerCase()
      if (!key || seen.has(key)) continue
      seen.add(key)
      merged.push({ kind: change.kind, text: change.text.trim() })
    }
  }
  const rank = (change: Change) => CHANGE_KINDS.findIndex((item) => item.kind === change.kind)
  return merged.sort((a, b) => rank(a) - rank(b))
}

/** Browser storage can be missing or refuse writes: never let that break the launcher. */
export const stored = {
  get(key: string): string | null {
    try {
      return localStorage.getItem(key)
    } catch {
      return null
    }
  },
  set(key: string, value: string) {
    try {
      localStorage.setItem(key, value)
    } catch {
      // Not remembered, that's all.
    }
  },
}

export const megabytes = (bytes: number) =>
  `${(bytes / 1_000_000).toLocaleString('fr-FR', { maximumFractionDigits: 1 })} Mo`
