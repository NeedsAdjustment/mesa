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

    if (!window.__mesaCssObserver) {
      window.__mesaCssObserver = new MutationObserver(() => ensureStyleLast())
      window.__mesaCssObserver.observe(document.documentElement, {
        childList: true,
        subtree: true,
      })
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
