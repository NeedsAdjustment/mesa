import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'

const TITLEBAR_HEIGHT = 32

console.info('Mesa frontend initialized')

const appWindow = getCurrentWindow()

document.getElementById('titlebar-minimize')?.addEventListener('click', () => appWindow.minimize())
document.getElementById('titlebar-maximize')?.addEventListener('click', () => appWindow.toggleMaximize())
document.getElementById('titlebar-close')?.addEventListener('click', () => appWindow.close())

function closeAllMenus() {
  document.querySelectorAll('.menu.open').forEach((m) => m.classList.remove('open'))
  invoke('resize_titlebar', { height: TITLEBAR_HEIGHT })
}

document.querySelectorAll<HTMLDivElement>('.menu').forEach((menu) => {
  const trigger = menu.querySelector<HTMLButtonElement>('.menu-trigger')!
  const dropdown = menu.querySelector<HTMLElement>('.dropdown')!

  trigger.addEventListener('click', (e) => {
    e.stopPropagation()
    const wasOpen = menu.classList.contains('open')
    closeAllMenus()
    if (!wasOpen) {
      menu.classList.add('open')
      const height = dropdown.offsetHeight || 300
      invoke('resize_titlebar', { height: TITLEBAR_HEIGHT + height })
    }
  })
})

document.addEventListener('click', () => {
  closeAllMenus()
})
