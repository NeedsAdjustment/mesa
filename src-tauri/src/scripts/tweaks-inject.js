;(() => {
  // Runs at document-start. The root element may not exist yet at that point,
  // so install once it does instead of returning.
  const host = location.hostname
  const isFacebookHost = host === 'facebook.com' || host.endsWith('.facebook.com')
  const isMessagesPage = location.pathname.startsWith('/messages')
  if (!isFacebookHost || !isMessagesPage) {
    if (isFacebookHost) {
      console.log('[mesa] tweaks: not a messages page, skipping CSS:', location.href)
    }
    return
  }

  const styleId = __STYLE_ID__
  const css = __CSS__

  const ensureEmptyChat = () => {
    const main = document.querySelector("div[role='main']")
    if (!main) return

    const icon = main.querySelector("i[data-visualcompletion='css-img']")
    const heading = main.querySelector('h3')
    const log = main.querySelector("[role='log']")
    const isEmpty =
      !log &&
      !!icon &&
      !!heading &&
      /no chats? selected|select a chat|choose a chat/i.test(heading.innerText || '')

    if (!isEmpty) {
      main.querySelectorAll('.mesa-empty-chat').forEach((el) => {
        el.classList.remove('mesa-empty-chat')
      })
      main.querySelectorAll('i.mesa-empty-icon').forEach((el) => {
        el.classList.remove('mesa-empty-icon')
      })
      return
    }

    const marked = main.querySelector('.mesa-empty-chat')
    if (icon.classList.contains('mesa-empty-icon') && marked) return

    icon.classList.add('mesa-empty-icon')

    let node = icon.parentElement
    while (node && node !== main) {
      const background = window.getComputedStyle(node).backgroundColor
      if (background && background !== 'rgba(0, 0, 0, 0)' && background !== 'transparent') {
        node.classList.add('mesa-empty-chat')
        break
      }
      node = node.parentElement
    }
  }

  const runMaintenance = () => {
    ensureStyle()
    ensureEmptyChat()
  }

  const startMaintenance = () => {
    if (window.__mesaMaintenanceTimer) return
    window.__mesaMaintenanceTimer = setInterval(runMaintenance, 500)
  }

  const ensureStyle = () => {
    const root = document.documentElement
    if (!root) return false

    const head = document.head || root
    let style = document.getElementById(styleId)

    if (!style) {
      style = document.createElement('style')
      style.id = styleId
      style.type = 'text/css'
      style.textContent = css
      head.appendChild(style)
      return true
    }

    if (style.textContent !== css) {
      style.textContent = css
    }

    if (style.parentNode !== head) {
      head.appendChild(style)
    }

    return true
  }

  const install = () => {
    if (!ensureStyle()) return false

    const root = document.documentElement
    if (root) {
      root.classList.toggle('mesa-global-theme', !!window.__mesaGlobalTheme)
    }

    // Older injections used a document-wide MutationObserver. It can stall
    // Messenger's E2EE conversation renderer, so remove it if still present.
    if (window.__mesaCssObserver) {
      window.__mesaCssObserver.disconnect()
      window.__mesaCssObserver = null
    }

    startMaintenance()
    runMaintenance()

    if (!window.__mesaTweaksInstalled) {
      window.__mesaTweaksInstalled = true
      console.log('[mesa] tweaks: custom CSS installed at', location.href)
    }

    return true
  }

  if (install()) return

  console.log('[mesa] tweaks: waiting for document root at', location.href)
  const rootWatcher = new MutationObserver(() => {
    if (install()) rootWatcher.disconnect()
  })
  rootWatcher.observe(document, { childList: true })
})()
