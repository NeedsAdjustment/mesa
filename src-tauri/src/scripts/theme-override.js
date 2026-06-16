window.__mesa_theme = 'dark'
window.__mesaThemeListeners = []
const __origMatchMedia = window.matchMedia
window.matchMedia = function (query) {
  if (query.startsWith('(prefers-color-scheme:')) {
    const isDarkQuery = query === '(prefers-color-scheme: dark)'
    const matches = isDarkQuery ? window.__mesa_theme === 'dark' : window.__mesa_theme === 'light'
    const result = {
      matches,
      media: query,
      onchange: null,
      addListener(fn) {
        this.addEventListener('change', fn)
      },
      removeListener(fn) {
        this.removeEventListener('change', fn)
      },
      addEventListener(type, fn) {
        if (type === 'change') window.__mesaThemeListeners.push(fn)
      },
      removeEventListener(type, fn) {
        if (type === 'change')
          window.__mesaThemeListeners = window.__mesaThemeListeners.filter(function (l) {
            return l !== fn
          })
      },
      dispatchEvent() {
        return false
      },
    }
    return result
  }
  return __origMatchMedia.apply(this, arguments)
}
