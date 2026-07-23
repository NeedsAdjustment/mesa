;(() => {
  // Intercept window.open to log all popup/new-window requests
  const origOpen = window.open
  window.open = function (url, target, features) {
    console.log('[mesa-nav] window.open called:', url, 'target:', target, 'features:', features)
    return origOpen.call(window, url, target, features)
  }

  // Intercept click events on anchor tags to log external links
  document.addEventListener(
    'click',
    function (e) {
      let el = e.target
      while (el && el.tagName !== 'A') el = el.parentElement
      if (el && el.tagName === 'A') {
        const href = el.getAttribute('href')
        const target = el.getAttribute('target')
        if (href) {
          console.log('[mesa-nav] link clicked:', href, 'target:', target, 'text:', el.innerText?.trim()?.slice(0, 80))
        }
      }
    },
    true,
  )

  // Log history.pushState / replaceState (SPA navigations)
  const origPushState = history.pushState
  history.pushState = function (state, title, url) {
    console.log('[mesa-nav] pushState:', url, 'title:', title)
    return origPushState.apply(this, arguments)
  }
  const origReplaceState = history.replaceState
  history.replaceState = function (state, title, url) {
    console.log('[mesa-nav] replaceState:', url, 'title:', title)
    return origReplaceState.apply(this, arguments)
  }

  // Log hash changes
  window.addEventListener('hashchange', function () {
    console.log('[mesa-nav] hashchange:', location.href)
  })

  console.log('[mesa-nav] navigation logger installed')
})()
