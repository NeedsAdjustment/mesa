;(() => {
  // Only run on the Messenger conversation UI. If the root element does not
  // exist yet, the on_page_load fallback will install this later.
  const host = location.hostname
  const isFacebookHost = host === 'facebook.com' || host.endsWith('.facebook.com')
  if (!isFacebookHost || !location.pathname.startsWith('/messages')) return
  if (!document.documentElement) return
  if (window.__mesaContextMenuInstalled) return
  window.__mesaContextMenuInstalled = true

  var menu = null

  function isEditable(target) {
    var el = target
    while (el && el !== document.body) {
      if (el.nodeType === 1) {
        if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') return true
        if (el.isContentEditable) return true
      }
      el = el.parentElement
    }
    return false
  }

  function hasSelection() {
    var selection = window.getSelection()
    return !!(selection && !selection.isCollapsed && String(selection).trim().length > 0)
  }

  document.addEventListener(
    'contextmenu',
    function (e) {
      // Editable contexts keep the native menu (no Inspect entry: devtools are
      // disabled for the webview and re-enabled only through the menu item).
      if (isEditable(e.target)) return
      if (!hasSelection()) {
        e.preventDefault()
        return
      }
      e.preventDefault()
      openMenu(e.clientX, e.clientY)
    },
    true,
  )

  function closeMenu() {
    if (!menu) return
    menu.remove()
    menu = null
    document.removeEventListener('mousedown', onDismiss, true)
    document.removeEventListener('keydown', onKey, true)
  }

  function onDismiss(e) {
    if (menu && menu.contains(e.target)) return
    closeMenu()
  }

  function onKey(e) {
    if (e.key === 'Escape') closeMenu()
  }

  function addItem(label, action) {
    var button = document.createElement('button')
    button.type = 'button'
    button.textContent = label
    button.addEventListener(
      'click',
      function () {
        closeMenu()
        action()
      },
      true,
    )
    menu.appendChild(button)
  }

  function openMenu(x, y) {
    closeMenu()
    menu = document.createElement('div')
    menu.id = 'mesa-context-menu'
    addItem('Copy', function () {
      document.execCommand('copy')
    })

    document.documentElement.appendChild(menu)
    // Clamp after layout so the menu never leaves the viewport.
    var rect = menu.getBoundingClientRect()
    menu.style.left = Math.min(x, window.innerWidth - rect.width - 8) + 'px'
    menu.style.top = Math.min(y, window.innerHeight - rect.height - 8) + 'px'

    document.addEventListener('mousedown', onDismiss, true)
    document.addEventListener('keydown', onKey, true)
  }
})()
