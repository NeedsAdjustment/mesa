import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { listen } from '@tauri-apps/api/event'
import { check, Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

const TITLEBAR_HEIGHT = 32

console.info('Mesa frontend initialized')

// Show the actual app version from tauri.conf.json
getVersion().then((v) => {
  const el = document.getElementById('app-version')
  if (el) el.textContent = `mesa v${v}`
})

const appWindow = getCurrentWindow()

// Set by the tray mode section below; read here so the minimize button can
// hide to the tray instead of minimising with no taskbar entry to restore from.
let trayModeEnabled = false

document.getElementById('titlebar-minimize')?.addEventListener('click', () => {
  if (trayModeEnabled) appWindow.hide()
  else appWindow.minimize()
})
document.getElementById('titlebar-maximize')?.addEventListener('click', () => appWindow.toggleMaximize())
document.getElementById('titlebar-close')?.addEventListener('click', () => appWindow.close())

document.addEventListener('contextmenu', (e) => e.preventDefault())

function closeAllMenus() {
  document.querySelectorAll('.menu.open').forEach((m) => m.classList.remove('open'))
  invoke('resize_titlebar', { height: TITLEBAR_HEIGHT })
}

function closeMenusAndFocusChat() {
  closeAllMenus()
  invoke('focus_chat')
}

document.querySelectorAll<HTMLDivElement>('.menu').forEach((menu) => {
  const trigger = menu.querySelector<HTMLButtonElement>('.menu-trigger')!
  const dropdown = menu.querySelector<HTMLElement>('.dropdown')!

  trigger.addEventListener('click', async (e) => {
    e.stopPropagation()
    const wasOpen = menu.classList.contains('open')
    closeAllMenus()
    if (!wasOpen) {
      if (trigger.textContent === 'View') {
        syncZoomDisplay()
      } else if (trigger.textContent === 'File') {
        const loggedIn = await invoke<boolean>('check_logged_in')
        document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
          if (btn.textContent?.trim() === 'Log Out' && btn.closest('.menu')?.querySelector('.menu-trigger')?.textContent === 'File') {
            btn.classList.toggle('hidden', !loggedIn)
          }
        })
      }
      menu.classList.add('open')
      const height = dropdown.offsetHeight || 300
      invoke('resize_titlebar', { height: TITLEBAR_HEIGHT + height })
    }
  })
})

document.addEventListener('click', () => {
  closeMenusAndFocusChat()
})

// Capture mousedown to close menus even on drag regions that swallow click events
document.addEventListener(
  'mousedown',
  (e) => {
    if (!(e.target as Element)?.closest?.('.menu.open')) {
      closeAllMenus()
    }
  },
  { capture: true },
)

// Close menus when the titlebar webview loses focus (e.g. clicking the chat webview)
window.addEventListener('blur', () => {
  closeMenusAndFocusChat()
})

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    closeMenusAndFocusChat()
  }
})

// ---- Interface Zoom ----

const ZOOM_MIN = 0.5
const ZOOM_MAX = 2.0
const ZOOM_STEP = 0.1

let currentZoom = 1.0

const zoomValueEl = document.querySelector<HTMLElement>('.zoom-value')
const zoomBtns = document.querySelectorAll<HTMLButtonElement>('.icon-btn')

function syncZoomDisplay() {
  invoke<number>('get_chat_zoom').then((z) => {
    currentZoom = z
    if (zoomValueEl) zoomValueEl.textContent = `${Math.round(currentZoom * 100)}%`
  })
}

function updateZoom(delta: number) {
  const next = Math.round((currentZoom + delta) * 10) / 10
  currentZoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, next))
  invoke('set_chat_zoom', { zoom: currentZoom })
  if (zoomValueEl) zoomValueEl.textContent = `${Math.round(currentZoom * 100)}%`
}

zoomBtns.forEach((btn) => {
  btn.addEventListener('click', (e) => {
    e.stopPropagation()
    const title = btn.getAttribute('title')
    if (title === 'zoom in') updateZoom(ZOOM_STEP)
    else if (title === 'zoom out') updateZoom(-ZOOM_STEP)
  })
})

// ---- Edit menu actions ----

const EDIT_ACTIONS: Record<string, string> = {
  Undo: 'undo',
  Redo: 'redo',
  Cut: 'cut',
  Copy: 'copy',
  Paste: 'paste',
  Delete: 'delete',
  'Select All': 'selectAll',
}

document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
  const action = EDIT_ACTIONS[btn.textContent?.trim() ?? '']
  if (action && btn.closest('.menu')?.querySelector('.menu-trigger')?.textContent === 'Edit') {
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      invoke('simulate_shortcut', { action })
      closeMenusAndFocusChat()
    })
  }
})

// ---- Hide Window Controls toggle ----

const HIDE_CONTROLS_KEY = 'mesa:controls-hidden'

function getControlsHidden(): boolean {
  return localStorage.getItem(HIDE_CONTROLS_KEY) === 'true'
}

function setControlsHidden(hidden: boolean) {
  localStorage.setItem(HIDE_CONTROLS_KEY, String(hidden))
  document.querySelector('.titlebar')?.classList.toggle('controls-hidden', hidden)
}

// Initialize from saved state
setControlsHidden(getControlsHidden())

// Find and wire up the Hide Window Controls button
document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
  if (btn.textContent?.trim() === 'Hide Window Controls' && btn.closest('.menu')?.querySelector('.menu-trigger')?.textContent === 'View') {
    btn.classList.toggle('checked', getControlsHidden())

    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      const hidden = !getControlsHidden()
      setControlsHidden(hidden)
      btn.classList.toggle('checked', hidden)
      closeMenusAndFocusChat()
    })
  }
})

// ---- Theme ----

const THEME_KEY = 'mesa:theme'

function getTheme(): string {
  return localStorage.getItem(THEME_KEY) || 'dark'
}

function resolveTheme(theme: string): string {
  if (theme === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }
  return theme
}

function applyTheme(theme: string) {
  const effective = resolveTheme(theme)
  document.documentElement.classList.toggle('theme-light', effective === 'light')
  invoke('set_theme', { theme: effective })
}

let systemThemeCleanup: (() => void) | null = null

function setTheme(theme: string) {
  localStorage.setItem(THEME_KEY, theme)

  document.querySelectorAll('.theme-btn').forEach((btn) => {
    btn.classList.toggle('selected', btn.getAttribute('title') === theme)
  })

  applyTheme(theme)

  systemThemeCleanup?.()
  systemThemeCleanup = null

  if (theme === 'system') {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = () => {
      applyTheme('system')
      setBackdropBlur(getBackdropBlur())
    }
    mq.addEventListener('change', handler)
    systemThemeCleanup = () => mq.removeEventListener('change', handler)
  }
}

// Initialize theme
setTheme(getTheme())

// Wire up theme radio buttons
document.querySelectorAll('.theme-btn').forEach((btn) => {
  btn.addEventListener('click', (e) => {
    e.stopPropagation()
    const chosenTheme = btn.getAttribute('title') || 'dark'
    // Non-system themes are incompatible with backdrop blur — turn it off
    if (chosenTheme !== 'system' && getBackdropBlur()) {
      setBackdropBlur(false)
    }
    setTheme(chosenTheme)
    closeMenusAndFocusChat()
  })
})

// ---- Backdrop Blur ----

const BACKDROP_KEY = 'mesa:backdrop-blur'

function getBackdropBlur(): boolean {
  return localStorage.getItem(BACKDROP_KEY) === 'true'
}

function setBackdropBlur(enabled: boolean) {
  localStorage.setItem(BACKDROP_KEY, String(enabled))

  document.querySelectorAll('.menu-entry').forEach((btn) => {
    if (btn.textContent?.trim() === 'Backdrop Blur') {
      btn.classList.toggle('checked', enabled)
    }
  })

  document.documentElement.classList.toggle('backdrop-blur', enabled)

  if (enabled) {
    // Acrylic follows the OS theme, can't be themed independently
    setTheme('system')
  }

  invoke('set_backdrop_blur', { enabled })
}

// Initialize from saved state
setBackdropBlur(getBackdropBlur())

// Wire up Backdrop Blur button
document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
  if (btn.textContent?.trim() === 'Backdrop Blur') {
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      const enabled = !getBackdropBlur()
      setBackdropBlur(enabled)
      closeMenusAndFocusChat()
    })
  }
})

// ---- Global Theme ----

function applyGlobalThemeUi(enabled: boolean) {
  document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
    if (btn.textContent?.trim() === 'Global Theme') {
      btn.classList.toggle('checked', enabled)
    }
  })
}

function setGlobalTheme(enabled: boolean) {
  applyGlobalThemeUi(enabled)
  invoke('set_global_theme', { enabled })
}

// The durable value lives in Rust, so hydrate the menu from there.
invoke<boolean>('get_global_theme').then(applyGlobalThemeUi)

document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
  if (btn.textContent?.trim() === 'Global Theme') {
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      setGlobalTheme(!btn.classList.contains('checked'))
      closeMenusAndFocusChat()
    })
  }
})

// ---- Tray mode ----

const trayModeBtn = document.getElementById('tray-mode-toggle')

function applyTrayModeUi(enabled: boolean) {
  trayModeEnabled = enabled
  document.documentElement.classList.toggle('tray-mode', enabled)
  trayModeBtn?.classList.toggle('checked', enabled)
}

function setTrayMode(enabled: boolean) {
  applyTrayModeUi(enabled)
  invoke('set_tray_mode', { enabled }).catch((error) =>
    console.error('failed to set tray mode', error),
  )
}

trayModeBtn?.addEventListener('click', (e) => {
  e.stopPropagation()
  setTrayMode(!trayModeBtn.classList.contains('checked'))
  closeMenusAndFocusChat()
})

// The durable value lives in Rust, so hydrate the menu from there.
invoke<boolean>('get_tray_mode').then(applyTrayModeUi)

// ---- Docked text size ----

const DOCK_SHRINK_KEY = 'mesa:dock-shrink'

function getDockShrink(): boolean {
  return localStorage.getItem(DOCK_SHRINK_KEY) === 'true'
}

const dockShrinkBtn = document.getElementById('dock-shrink-toggle') as HTMLButtonElement | null

function applyDockShrinkUi(enabled: boolean) {
  dockShrinkBtn?.classList.toggle('checked', enabled)
}

function setDockShrink(enabled: boolean) {
  localStorage.setItem(DOCK_SHRINK_KEY, String(enabled))
  applyDockShrinkUi(enabled)
  // Apply the step immediately when the panel is currently docked.
  if (windowNarrow) stepChatZoom(enabled ? -1 : 1)
}

dockShrinkBtn?.addEventListener('click', (e) => {
  e.stopPropagation()
  setDockShrink(!dockShrinkBtn.classList.contains('checked'))
  closeMenusAndFocusChat()
})

applyDockShrinkUi(getDockShrink())

// ---- File menu actions ----

document.querySelectorAll<HTMLButtonElement>('.menu-entry').forEach((btn) => {
  if (btn.textContent?.trim() === 'Show DevTools' && btn.closest('.menu')?.querySelector('.menu-trigger')?.textContent === 'File') {
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      invoke('show_devtools')
      closeMenusAndFocusChat()
    })
  }

  if (btn.textContent?.trim() === 'Log Out' && btn.closest('.menu')?.querySelector('.menu-trigger')?.textContent === 'File') {
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      invoke('logout')
      closeMenusAndFocusChat()
    })
  }
})

// ---- Updates ----

const UPDATE_CHECK_DELAY_MS = 10 * 1000
const UPDATE_CHECK_INTERVAL_MS = 8 * 60 * 60 * 1000
const CHECK_UPDATES_LABEL = 'Check for Updates…'

const checkUpdatesBtn = document.getElementById('check-updates') as HTMLButtonElement | null
const autoUpdatesBtn = document.getElementById('auto-updates-toggle')
const updatePillEl = document.getElementById('update-pill')
const updatePillTextEl = document.getElementById('update-pill-text')
const updatePillIconEl = updatePillEl?.querySelector('i') ?? null

let pendingUpdate: Update | null = null
let pendingUpdateDownloaded = false
let updateInstalling = false
let manualCheckRunning = false
let checkUpdatesTimer: number | null = null
let autoCheckTimeout: number | null = null
let autoCheckInterval: number | null = null

// Non-actionable states render dimmed and inert in the single File-menu
// entry; `revertMs` restores the actionable label.
function setCheckUpdatesLabel(text: string, revertMs = 0) {
  if (!checkUpdatesBtn) return

  if (checkUpdatesTimer !== null) {
    window.clearTimeout(checkUpdatesTimer)
    checkUpdatesTimer = null
  }

  checkUpdatesBtn.textContent = text
  const actionable = text === CHECK_UPDATES_LABEL
  checkUpdatesBtn.classList.toggle('dim', !actionable)
  checkUpdatesBtn.disabled = !actionable

  if (revertMs > 0) {
    checkUpdatesTimer = window.setTimeout(() => {
      checkUpdatesTimer = null
      resetCheckUpdatesLabel()
    }, revertMs)
  }
}

function resetCheckUpdatesLabel() {
  setCheckUpdatesLabel(CHECK_UPDATES_LABEL)
}

function showUpdatePill(state: 'downloading' | 'ready' | 'installing', version: string, detail = '') {
  if (!updatePillEl) return

  updatePillEl.classList.remove('hidden')
  updatePillEl.classList.toggle('ready', state !== 'downloading')

  if (updatePillIconEl) {
    updatePillIconEl.className =
      state === 'downloading' ? 'ph ph-spinner-gap' : 'ph ph-arrow-clockwise'
  }

  const label = state === 'installing' ? 'Installing…' : `v${version}`
  if (updatePillTextEl) updatePillTextEl.textContent = detail ? `${label} ${detail}` : label

  updateTitleVisibility()
}

function hideUpdatePill() {
  updatePillEl?.classList.add('hidden')
  updateTitleVisibility()
}

function downloadPendingUpdate() {
  const update = pendingUpdate
  if (!update || pendingUpdateDownloaded) return

  let contentLength = 0
  let downloaded = 0

  update
    .download((event) => {
      switch (event.event) {
        case 'Started':
          contentLength = event.data.contentLength ?? 0
          break
        case 'Progress':
          downloaded += event.data.chunkLength
          if (contentLength > 0) {
            showUpdatePill(
              'downloading',
              update.version,
              `${Math.round((downloaded / contentLength) * 100)}%`,
            )
          }
          break
        case 'Finished':
          pendingUpdateDownloaded = true
          showUpdatePill('ready', update.version)
          break
      }
    })
    .then(() => {
      pendingUpdateDownloaded = true
      showUpdatePill('ready', update.version)
    })
    .catch((error) => {
      console.error('failed to download update', error)
      // Drop the pending update so a later check can retry from scratch.
      update.close().catch(() => {})
      pendingUpdate = null
      pendingUpdateDownloaded = false
      hideUpdatePill()
    })
}

async function runUpdateCheck(manual: boolean) {
  if (pendingUpdate) {
    if (manual) {
      setCheckUpdatesLabel(
        pendingUpdateDownloaded
          ? `Mesa v${pendingUpdate.version} is ready to install`
          : `Downloading Mesa v${pendingUpdate.version}…`,
        4000,
      )
    }
    return
  }

  if (manual) {
    if (manualCheckRunning) return
    manualCheckRunning = true
    setCheckUpdatesLabel('Checking…')
  }

  try {
    const update = await check()

    if (!update) {
      if (manual) setCheckUpdatesLabel('Mesa is up to date', 4000)
      return
    }

    pendingUpdate = update
    pendingUpdateDownloaded = false
    showUpdatePill('downloading', update.version)
    downloadPendingUpdate()

    if (manual) resetCheckUpdatesLabel()
  } catch (error) {
    console.error('failed to check for updates', error)
    if (manual) setCheckUpdatesLabel('Update check failed', 4000)
  } finally {
    manualCheckRunning = false
  }
}

async function installPendingUpdate() {
  const update = pendingUpdate
  if (!update || !pendingUpdateDownloaded || updateInstalling) return

  updateInstalling = true
  showUpdatePill('installing', update.version)

  try {
    // On Windows the installer runs passively, restarts the app itself and
    // exits this process; relaunch() is the fallback for other platforms.
    await update.install()
    await relaunch()
  } catch (error) {
    console.error('failed to install update', error)
    updateInstalling = false
    showUpdatePill('ready', update.version)
  }
}

function stopAutoUpdateChecks() {
  if (autoCheckTimeout !== null) {
    window.clearTimeout(autoCheckTimeout)
    autoCheckTimeout = null
  }
  if (autoCheckInterval !== null) {
    window.clearInterval(autoCheckInterval)
    autoCheckInterval = null
  }
}

function startAutoUpdateChecks() {
  stopAutoUpdateChecks()
  autoCheckTimeout = window.setTimeout(() => void runUpdateCheck(false), UPDATE_CHECK_DELAY_MS)
  autoCheckInterval = window.setInterval(() => void runUpdateCheck(false), UPDATE_CHECK_INTERVAL_MS)
}

function applyAutoUpdatesUi(enabled: boolean) {
  autoUpdatesBtn?.classList.toggle('checked', enabled)
}

function setAutoUpdates(enabled: boolean) {
  applyAutoUpdatesUi(enabled)
  invoke('set_auto_updates', { enabled }).catch((error) =>
    console.error('failed to persist automatic updates setting', error),
  )
  if (enabled) startAutoUpdateChecks()
  else stopAutoUpdateChecks()
}

autoUpdatesBtn?.addEventListener('click', (e) => {
  e.stopPropagation()
  setAutoUpdates(!autoUpdatesBtn.classList.contains('checked'))
  closeMenusAndFocusChat()
})

checkUpdatesBtn?.addEventListener('click', (e) => {
  e.stopPropagation()
  void runUpdateCheck(true)
})

updatePillEl?.addEventListener('click', (e) => {
  e.stopPropagation()
  void installPendingUpdate()
})

// The durable value lives in Rust, so hydrate the menu from there.
invoke<boolean>('get_auto_updates').then((enabled) => {
  applyAutoUpdatesUi(enabled)
  if (enabled) startAutoUpdateChecks()
})

// ---- Narrow window ----

let windowNarrow: boolean | null = null

function stepChatZoom(direction: number) {
  invoke<number>('get_chat_zoom')
    .then((zoom) => {
      const next = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom + direction * ZOOM_STEP))
      return invoke('set_chat_zoom', { zoom: next })
    })
    .catch((error) => console.error('failed to adjust docked zoom', error))
}

// Messenger switches to its full-width mobile layout around 663px. The chat
// script expands the sidebar and removes the resize handle when this is set.
// With shrink on, docking steps the chat zoom one tick down and undocking
// steps it back up.
function updateNarrowWindow() {
  const narrow = window.innerWidth <= 663
  invoke('set_window_narrow', { narrow }).catch((error) =>
    console.error('failed to update narrow window state', error),
  )

  if (windowNarrow !== null && narrow !== windowNarrow && getDockShrink()) {
    stepChatZoom(narrow ? -1 : 1)
  }
  windowNarrow = narrow
}
window.addEventListener('resize', updateNarrowWindow)
updateNarrowWindow()

// ---- Title text visibility ----

const titleTextEl = document.querySelector<HTMLElement>('.title-text')
const appTitleTextEl = document.getElementById('app-title-text')
const menuBarEl = document.querySelector<HTMLElement>('.menu-bar')
const controlsEl = document.querySelector<HTMLElement>('.controls')

function updateTitleVisibility() {
  if (!titleTextEl || !appTitleTextEl) return

  const text = appTitleTextEl.getBoundingClientRect()
  const menu = menuBarEl?.getBoundingClientRect()
  const downloadsMenu = document.querySelector<HTMLElement>('.downloads-menu:not(.hidden)')
  const updatePill = document.getElementById('update-pill')
  const pill = updatePill?.classList.contains('hidden') ? null : updatePill
  const right = (pill ?? downloadsMenu ?? controlsEl)?.getBoundingClientRect()
  const overlaps =
    (menu && text.left < menu.right + 6) ||
    (right && text.right > right.left - 6)

  titleTextEl.classList.toggle('hidden', window.innerWidth <= 399 || overlaps)
}
window.addEventListener('resize', updateTitleVisibility)
controlsEl?.addEventListener('transitionend', updateTitleVisibility)
updateTitleVisibility()

// ---- Downloads menu ----

interface DownloadStartedPayload {
  url: string
  filename: string
  path: string
}

interface DownloadFinishedPayload {
  url: string
  filename: string
  path: string | null
  success: boolean
}

type DownloadStatus = 'downloading' | 'saved' | 'failed'

interface SessionDownload {
  url: string
  filename: string
  path: string | null
  status: DownloadStatus
}

const downloadsMenuEl = document.querySelector<HTMLElement>('.downloads-menu')
const downloadsListEl = document.getElementById('downloads-list')
const downloadsDropdownEl =
  downloadsMenuEl?.querySelector<HTMLElement>('.downloads-dropdown') ?? null
const showDownloadsFolderEl = document.getElementById('show-downloads-folder')

const sessionDownloads: SessionDownload[] = []

function middleTruncate(value: string, maxLength = 34): string {
  if (value.length <= maxLength) return value

  const dot = value.lastIndexOf('.')
  const extension = dot > 0 ? value.slice(dot) : ''
  const stem = dot > 0 ? value.slice(0, dot) : value
  const available = Math.max(4, maxLength - extension.length - 1)
  const front = Math.ceil(available * 0.6)
  const back = Math.floor(available * 0.4)
  const tail = back > 0 ? stem.slice(-back) : ''

  return `${stem.slice(0, front)}…${tail}${extension}`
}

function resizeDownloadsDropdown() {
  if (!downloadsMenuEl?.classList.contains('open') || !downloadsDropdownEl) return
  invoke('resize_titlebar', {
    height: TITLEBAR_HEIGHT + downloadsDropdownEl.offsetHeight,
  })
}

function renderDownloads() {
  if (!downloadsListEl) return

  // The titlebar entry only exists once something has been downloaded this
  // session; a fresh launch starts with no Downloads button.
  downloadsMenuEl?.classList.toggle('hidden', sessionDownloads.length === 0)

  downloadsListEl.replaceChildren()

  if (sessionDownloads.length === 0) {
    const empty = document.createElement('div')
    empty.className = 'menu-entry text download-empty'
    empty.textContent = 'No downloads this session'
    downloadsListEl.appendChild(empty)
    resizeDownloadsDropdown()
    updateTitleVisibility()
    return
  }

  for (const download of sessionDownloads) {
    const entry = document.createElement('button')
    entry.type = 'button'
    entry.className = 'menu-entry download-entry'
    entry.disabled = download.status !== 'saved' || !download.path

    const name = document.createElement('span')
    name.className = 'download-entry-name'
    name.textContent = middleTruncate(download.filename || 'download')
    name.title = download.filename || 'download'
    entry.appendChild(name)

    const state = document.createElement('span')
    state.className = 'download-entry-state'
    const icon = document.createElement('i')

    if (download.status === 'downloading') {
      icon.className = 'ph ph-spinner-gap'
    } else if (download.status === 'failed') {
      icon.className = 'ph ph-warning'
      state.classList.add('failed')
    } else {
      icon.className = 'ph ph-check'
    }

    state.appendChild(icon)
    entry.appendChild(state)

    const path = download.path
    if (download.status === 'saved' && path) {
      entry.addEventListener('click', (event) => {
        event.stopPropagation()
        invoke('open_download', { path }).catch((error) =>
          console.error('failed to open download', error),
        )
        closeAllMenus()
      })
    }

    downloadsListEl.appendChild(entry)
  }

  resizeDownloadsDropdown()
  updateTitleVisibility()
}

listen<DownloadStartedPayload>('download-started', (event) => {
  sessionDownloads.unshift({
    url: event.payload.url,
    filename: event.payload.filename || 'download',
    path: event.payload.path || null,
    status: 'downloading',
  })
  renderDownloads()
}).catch((error) => console.error('failed to listen for download-started', error))

listen<DownloadFinishedPayload>('download-finished', (event) => {
  const payload = event.payload
  const existing =
    sessionDownloads.find(
      (download) =>
        download.status === 'downloading' &&
        download.url === payload.url &&
        download.filename === payload.filename,
    ) ??
    sessionDownloads.find(
      (download) => download.status === 'downloading' && download.url === payload.url,
    )

  if (existing) {
    existing.status = payload.success ? 'saved' : 'failed'
    existing.path = payload.path ?? existing.path
    if (payload.filename) existing.filename = payload.filename
  } else {
    sessionDownloads.unshift({
      url: payload.url,
      filename: payload.filename || 'download',
      path: payload.path,
      status: payload.success ? 'saved' : 'failed',
    })
  }

  renderDownloads()
}).catch((error) => console.error('failed to listen for download-finished', error))

showDownloadsFolderEl?.addEventListener('click', (event) => {
  event.stopPropagation()
  invoke('show_downloads').catch((error) => console.error('failed to open downloads folder', error))
  closeAllMenus()
})

renderDownloads()

