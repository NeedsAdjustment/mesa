;(() => {
  var SIDEBAR_SELECTOR = "div[aria-label='Thread list'][role='navigation']"
  var STORAGE_KEY = 'mesa:sidebar-width'
  var HANDLE_WIDTH = 6
  var MIN_WIDTH = 180

  function getMaxWidth() {
    if (document.documentElement.classList.contains('narrow-window')) {
      return window.innerWidth
    }
    return Math.round(window.innerWidth / 2)
  }

  var sidebar = null
  var handle = null
  var startX = 0
  var startWidth = 0
  var isDragging = false
  var resizeObserver = new ResizeObserver(function () {
    syncHandlePosition()
  })

  function getSavedWidth() {
    var w = localStorage.getItem(STORAGE_KEY)
    return w ? parseFloat(w) : null
  }

  function saveWidth(w) {
    localStorage.setItem(STORAGE_KEY, String(Math.round(w)))
  }

  function setSidebarWidth(w) {
    sidebar.style.setProperty('width', w + 'px', 'important')
    sidebar.style.setProperty('flex', 'none', 'important')
  }

  function clearSidebarWidth() {
    sidebar.style.removeProperty('width')
    sidebar.style.removeProperty('flex')
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
    cleanupDrag()
    if (sidebar) {
      resizeObserver.unobserve(sidebar)
      clearSidebarWidth()
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
    if (!isDragging) return
    var newWidth = Math.min(getMaxWidth(), Math.max(MIN_WIDTH, startWidth + (e.clientX - startX)))
    setSidebarWidth(newWidth)
    syncHandlePosition()
  }

  function onMouseUp() {
    if (!isDragging) return
    saveWidth(sidebar.offsetWidth)
    cleanupDrag()
  }

  function applySavedWidth() {
    var saved = getSavedWidth()
    if (saved) {
      setSidebarWidth(Math.min(saved, getMaxWidth()))
    } else {
      clearSidebarWidth()
    }
  }

  function updateHandle() {
    if (document.documentElement.classList.contains('sidebar-collapsed') || document.documentElement.classList.contains('narrow-window')) {
      removeHandle()
    } else {
      createHandle()
    }
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
    // Only act if the class change actually affects handle visibility
    updateHandle()
  })
  classObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init)
  } else {
    init()
  }
})()
