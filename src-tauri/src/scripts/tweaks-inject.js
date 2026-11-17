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
  let emptyChatFrame = 0

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

  const scheduleEmptyChat = () => {
    if (emptyChatFrame) return
    emptyChatFrame = requestAnimationFrame(() => {
      emptyChatFrame = 0
      ensureEmptyChat()
    })
  }

  const ensureStyleLast = () => {
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

    if (style.parentNode !== head || head.lastElementChild !== style) {
      head.appendChild(style)
    }

    return true
  }

  const install = () => {
    if (!ensureStyleLast()) return false

    const root = document.documentElement
    if (root) {
      root.classList.toggle('mesa-standard-theme', !!window.__mesaStandardTheme)
    }

    if (!window.__mesaCssObserver) {
      window.__mesaCssObserver = new MutationObserver(() => {
        ensureStyleLast()
        scheduleEmptyChat()
      })
      window.__mesaCssObserver.observe(document.documentElement, {
        childList: true,
        subtree: true,
      })
      console.log('[mesa] tweaks: custom CSS installed at', location.href)
    }

    scheduleEmptyChat()
    return true
  }

  if (install()) return

  console.log('[mesa] tweaks: waiting for document root at', location.href)
  const rootWatcher = new MutationObserver(() => {
    if (install()) rootWatcher.disconnect()
  })
  rootWatcher.observe(document, { childList: true })
})()
