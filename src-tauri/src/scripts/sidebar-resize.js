;(() => {
  // Only run on the Messenger conversation UI. If the root element does not
  // exist yet, the on_page_load fallback will install this later.
  const host = location.hostname
  const isFacebookHost = host === 'facebook.com' || host.endsWith('.facebook.com')
  if (!isFacebookHost || !location.pathname.startsWith('/messages')) return
  if (!document.documentElement) return
  if (window.__mesaSidebarInstalled) return
  window.__mesaSidebarInstalled = true

  var SIDEBAR_SELECTOR = "div[aria-label='Thread list'][role='navigation']"
  var WIDTH_KEY = 'mesa:sidebar-width'
  var COLLAPSED_KEY = 'mesa:sidebar-collapsed'
  var HANDLE_WIDTH = 6
  var MIN_WIDTH = 180
  var DEFAULT_EXPANDED_WIDTH = 280
  var SNAP_DURATION = 140

  var sidebar = null
  var handle = null
  var startX = 0
  var startWidth = 0
  var isDragging = false
  var lastTargetWidth = 0
  var lastCollapsed = false
  var snapToken = 0
  var injectedState = window.__mesaSidebarState || null
  var resizeObserver = new ResizeObserver(function () {
    syncHandlePosition()
  })

  // The chat document owns the live state. The injected copy is a durable
  // fallback for the first paint after logout clears site data.
  var storedCollapsed = localStorage.getItem(COLLAPSED_KEY)
  var initialCollapsed =
    storedCollapsed !== null
      ? storedCollapsed === 'true'
      : !!(injectedState && injectedState.collapsed)
  if (initialCollapsed) {
    document.documentElement.classList.add('sidebar-collapsed')
  }
  lastCollapsed = isCollapsed()

  function isCollapsed() {
    return document.documentElement.classList.contains('sidebar-collapsed')
  }

  function persistCollapsed(collapsed) {
    try {
      localStorage.setItem(COLLAPSED_KEY, String(collapsed))
    } catch (_) {}
  }

  function getSavedWidth() {
    var w = localStorage.getItem(WIDTH_KEY)
    if (w) return parseFloat(w)

    if (
      injectedState &&
      typeof injectedState.width === 'number' &&
      injectedState.width > 0
    ) {
      return injectedState.width
    }

    return null
  }

  function saveWidth(w) {
    localStorage.setItem(WIDTH_KEY, String(Math.round(w)))
  }

  function getMaxWidth() {
    if (document.documentElement.classList.contains('narrow-window')) {
      return window.innerWidth
    }
    return Math.round(window.innerWidth / 2)
  }

  function getCollapsedWidth() {
    var rootSize = parseFloat(window.getComputedStyle(document.documentElement).fontSize)
    return Math.round((isNaN(rootSize) ? 16 : rootSize) * 6)
  }

  function getExpandedWidth() {
    var max = getMaxWidth()
    var saved = getSavedWidth()
    var min = Math.min(MIN_WIDTH, max)
    if (saved) return Math.max(Math.min(saved, max), min)
    return Math.min(DEFAULT_EXPANDED_WIDTH, max)
  }

  function setSidebarWidth(w) {
    if (!sidebar) return
    lastTargetWidth = w
    sidebar.style.setProperty('width', w + 'px', 'important')
    sidebar.style.setProperty('flex', 'none', 'important')
  }

  function clearSidebarWidth() {
    lastTargetWidth = 0
    if (!sidebar) return
    sidebar.style.removeProperty('width')
    sidebar.style.removeProperty('flex')
  }

  function animateSidebarWidth(target, done) {
    var token = ++snapToken

    if (!sidebar) {
      lastTargetWidth = target
      if (done) done()
      return
    }

    lastTargetWidth = target
    if (!sidebar.style.getPropertyValue('width')) {
      // Give the width an explicit start value so transitions from `auto` animate.
      sidebar.style.setProperty(
        'width',
        sidebar.getBoundingClientRect().width + 'px',
        'important',
      )
      sidebar.style.setProperty('flex', 'none', 'important')
    }

    sidebar.style.setProperty(
      'transition',
      'width ' + SNAP_DURATION + 'ms cubic-bezier(0.2, 0, 0, 1)',
      'important',
    )
    sidebar.style.setProperty('flex', 'none', 'important')

    requestAnimationFrame(function () {
      if (token !== snapToken) return
      setSidebarWidth(target)
      syncHandlePosition()
      window.setTimeout(function () {
        if (token !== snapToken) return
        if (sidebar) sidebar.style.removeProperty('transition')
        if (done) done()
      }, SNAP_DURATION + 20)
    })
  }

  function collapseSidebarAnimated() {
    animateSidebarWidth(getCollapsedWidth())
  }

  function expandSidebarAnimated() {
    var target = getExpandedWidth()
    animateSidebarWidth(target, function () {
      // If the user started resizing again while the snap animation ran, the
      // drag's own mouseup already saved the newer width.
      if (!isCollapsed() && Math.abs(lastTargetWidth - target) < 1) saveWidth(target)
    })
  }

  function syncHandlePosition() {
    // Re-query in case React replaced the DOM node
    var current = document.querySelector(SIDEBAR_SELECTOR)
    if (current && current !== sidebar) {
      resizeObserver.unobserve(sidebar)
      sidebar = current
      resizeObserver.observe(sidebar)
      applySavedWidth()
    }
    if (!handle || !sidebar || !sidebar.isConnected) return
    var rect = sidebar.getBoundingClientRect()
    handle.style.top = rect.top + 12 + 'px'
    handle.style.left = rect.left + rect.width - HANDLE_WIDTH + 'px'
    handle.style.height = rect.height - 12 + 'px'
  }

  function createHandle() {
    if (handle) return
    sidebar = document.querySelector(SIDEBAR_SELECTOR)
    if (!sidebar) return

    // Re-apply the saved width whenever the handle is re-created.
    applySavedWidth()

    handle = document.createElement('div')
    handle.id = 'mesa-sidebar-resize-handle'
    Object.assign(handle.style, {
      position: 'fixed',
      width: HANDLE_WIDTH + 'px',
      cursor: 'col-resize',
      zIndex: '999',
      borderRight: '1px solid rgba(128, 128, 128, 0.15)',
      pointerEvents: 'auto',
    })
    document.body.appendChild(handle)
    resizeObserver.observe(sidebar)
    requestAnimationFrame(syncHandlePosition)
    window.addEventListener('scroll', syncHandlePosition, { passive: true })
    window.addEventListener('resize', syncHandlePosition, { passive: true })

    handle.addEventListener('mousedown', onMouseDown)
  }

  function removeHandle() {
    if (!handle && !isDragging) return
    cleanupDrag()
    if (sidebar) {
      resizeObserver.unobserve(sidebar)
    }
    if (handle) {
      window.removeEventListener('scroll', syncHandlePosition)
      window.removeEventListener('resize', syncHandlePosition)
      handle.removeEventListener('mousedown', onMouseDown)
      handle.remove()
      handle = null
    }
  }

  function cleanupDrag() {
    isDragging = false
    document.removeEventListener('mousemove', onMouseMove)
    document.removeEventListener('mouseup', onMouseUp)
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
  }

  function onMouseDown(e) {
    if (!sidebar) return
    e.preventDefault()
    isDragging = true
    startX = e.clientX
    startWidth = sidebar.offsetWidth
    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
  }

  function onMouseMove(e) {
    if (!isDragging || !sidebar) return

    // Track the pointer 1:1 for the whole gesture, including through the
    // icon-rail region. The collapsed class only controls whether labels are
    // visible; the inline width keeps following the cursor.
    var width = Math.min(
      getMaxWidth(),
      Math.max(getCollapsedWidth(), startWidth + (e.clientX - startX)),
    )
    setSidebarWidth(width)

    var shouldCollapse = width < MIN_WIDTH
    if (shouldCollapse !== isCollapsed()) {
      document.documentElement.classList.toggle('sidebar-collapsed', shouldCollapse)
    }

    syncHandlePosition()
  }

  function onMouseUp() {
    if (!isDragging) return

    var width = lastTargetWidth > 0 ? lastTargetWidth : sidebar.offsetWidth
    cleanupDrag()

    if (width < MIN_WIDTH) {
      // Released inside the icon-rail region: settle all the way to the rail.
      if (isCollapsed()) {
        collapseSidebarAnimated()
      } else {
        document.documentElement.classList.add('sidebar-collapsed')
      }
    } else {
      saveWidth(width)
      if (isCollapsed()) {
        document.documentElement.classList.remove('sidebar-collapsed')
      }
    }
  }

  function applySavedWidth() {
    if (isCollapsed()) {
      setSidebarWidth(getCollapsedWidth())
      return
    }

    var saved = getSavedWidth()
    if (saved) {
      var max = getMaxWidth()
      setSidebarWidth(Math.max(Math.min(saved, max), Math.min(MIN_WIDTH, max)))
    } else {
      clearSidebarWidth()
    }
  }

  function updateHandle() {
    if (document.documentElement.classList.contains('narrow-window')) {
      // The narrow layout is full-width, so the inline width has to go.
      removeHandle()
      clearSidebarWidth()
    } else if (isOverlayOpen()) {
      // Menus and overlays sit above the handle; drop the handle but keep
      // the sidebar's width so nothing shifts around underneath.
      removeHandle()
    } else {
      createHandle()
    }
  }

  function applySidebarState(state) {
    if (!state) return

    // Existing chat storage is the live state and is always at least as new as
    // the app-level copy. Only fill in values that were cleared (e.g. logout).
    var storedWidth = localStorage.getItem(WIDTH_KEY)
    var storedCollapsed = localStorage.getItem(COLLAPSED_KEY)
    var width = storedWidth !== null ? parseFloat(storedWidth) : Number(state.width)
    var collapsed =
      storedCollapsed !== null ? storedCollapsed === 'true' : !!state.collapsed

    try {
      if (storedWidth === null && Number.isFinite(width) && width > 0) {
        localStorage.setItem(WIDTH_KEY, String(Math.round(width)))
      }
      if (storedCollapsed === null) {
        localStorage.setItem(COLLAPSED_KEY, String(collapsed))
      }
    } catch (_) {}

    document.documentElement.classList.toggle('sidebar-collapsed', collapsed)
    applySavedWidth()
    updateHandle()
  }

  // Called from Rust after page load so the durable app-level settings win
  // over any stale injected/localStorage values.
  window.__mesaApplySidebarState = applySidebarState

  function isOverlayOpen() {
    // Check for visible [role="menu"] dropdowns (sidebar menu, etc.)
    var menus = document.querySelectorAll('[role="menu"]')
    for (var i = 0; i < menus.length; i++) {
      if (menus[i].offsetParent !== null) return true
    }
    // Check for full-viewport backdrops (photo viewer, preferences, etc.)
    if (document.querySelector('div.xixxii4.x1h0vfkc')) return true
    // Fallback: if an opaque fixed-position element covers the viewport
    // centre, treat it as an overlay.
    var el = document.elementFromPoint(
      Math.round(window.innerWidth / 2),
      Math.round(window.innerHeight / 2),
    )
    while (el && el !== document.body && el !== document.documentElement) {
      var cs = window.getComputedStyle(el)
      if (cs.position === 'fixed') {
        var bg = cs.backgroundColor
        return !!(bg && bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent')
      }
      el = el.parentElement
    }
    return false
  }

  function init() {
    sidebar = document.querySelector(SIDEBAR_SELECTOR)
    if (sidebar) {
      applySavedWidth()
      updateHandle()
      return
    }

    var observer = new MutationObserver(function () {
      sidebar = document.querySelector(SIDEBAR_SELECTOR)
      if (sidebar) {
        observer.disconnect()
        applySavedWidth()
        updateHandle()
      }
    })
    observer.observe(document.body || document.documentElement, {
      childList: true,
      subtree: true,
    })
  }

  var classObserver = new MutationObserver(function () {
    // In Messenger's narrow/mobile layout the icon rail is not usable.
    if (
      document.documentElement.classList.contains('narrow-window') &&
      isCollapsed()
    ) {
      document.documentElement.classList.remove('sidebar-collapsed')
    }

    var collapsed = isCollapsed()
    if (collapsed !== lastCollapsed) {
      lastCollapsed = collapsed
      persistCollapsed(collapsed)
      // While dragging, the width is controlled by the pointer. Don't animate
      // to the rail/saved width here; the mouseup settle handles that.
      if (!isDragging) {
        if (collapsed) collapseSidebarAnimated()
        else expandSidebarAnimated()
      }
    }
    updateHandle()
  })
  classObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })

  // Watch for popovers (photo viewer, preferences, etc.) appearing or
  // disappearing. Coalesced: the boot sequence produces dense mutation
  // batches.
  var handleDebounce = 0
  var mediaObserver = new MutationObserver(function () {
    if (handleDebounce) return
    handleDebounce = window.setTimeout(function () {
      handleDebounce = 0
      updateHandle()
    }, 150)
  })
  mediaObserver.observe(document.body || document.documentElement, {
    childList: true,
    subtree: true,
  })

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init)
  } else {
    init()
  }
})()
