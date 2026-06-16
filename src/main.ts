import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'

const TITLEBAR_HEIGHT = 32

console.info('Mesa frontend initialized')

const appWindow = getCurrentWindow()

document.getElementById('titlebar-minimize')?.addEventListener('click', () => appWindow.minimize())
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

