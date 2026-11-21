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

  const markDraftLabels = () => {
    const sidebar = document.querySelector("div[aria-label='Thread list'][role='navigation']")
    if (!sidebar) return

    for (const label of sidebar.querySelectorAll('span.mesa-draft-label')) {
      if (!/^Draft:?$/i.test(label.textContent.trim())) {
        label.classList.remove('mesa-draft-label')
      }
    }

    for (const span of sidebar.querySelectorAll('span')) {
      if (
        span.childElementCount === 0 &&
        /^Draft:?$/i.test(span.textContent.trim()) &&
        !span.classList.contains('mesa-draft-label')
      ) {
        span.classList.add('mesa-draft-label')
      }
    }
  }

  const runMaintenance = () => {
    ensureStyle()
    markDraftLabels()
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
