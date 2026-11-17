;(() => {
  // Navigation and popup handling lives in Rust:
  // src-tauri/src/modules/navigation.rs and window.rs.
  // This script only logs SPA navigations to make debugging easier. It must
  // never prevent default or rewrite window.open, otherwise downloads and
  // Messenger call popups break.

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

  window.addEventListener('hashchange', function () {
    console.log('[mesa-nav] hashchange:', location.href)
  })

  // Capture-phase logger only. Do not call preventDefault here: the Rust
  // navigation handler decides whether a link stays in-app or opens elsewhere.
  document.addEventListener(
    'click',
    function (e) {
      const anchor = e.target instanceof Element ? e.target.closest('a[href]') : null
      if (!anchor) return
      console.log(
        '[mesa-nav] link click:',
        anchor.href,
        'target:',
        anchor.target || '_self',
        'download:',
        anchor.hasAttribute('download'),
      )
    },
    true,
  )

  console.log('[mesa-nav] navigation logger installed')
})()
