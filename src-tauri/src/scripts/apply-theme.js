window.__mesa_theme = __THEME_JSON__;
document.documentElement.style.colorScheme = __THEME_JSON__;
window.__mesaThemeListeners.forEach(function(fn) {
  try { fn({ matches: __THEME_JSON__ === 'dark', media: '(prefers-color-scheme: dark)' }); } catch(e) {}
});
