;(() => {
  // Keep this logic in sync with the canonical Rust source:
  // src-tauri/src/modules/navigation.rs
  // Note: when IS_LOGGED_OUT is true, all Facebook/Messenger URLs are allowed.

  const ALLOWED_HOSTS = [
    'facebook.com',
    'www.facebook.com',
    'web.facebook.com',
    'messenger.com',
    'www.messenger.com',
  ];

  const ALLOWED_PATHS = [
    '/messages',
    '/messenger_media',
    '/login',
    '/logout',
    '/checkpoint',
    '/two_step_verification',
    '/two_factor',
    '/',
  ];

  function isInternalHost(host) {
    return ALLOWED_HOSTS.some(h => host === h || host.endsWith('.' + h));
  }

  function isAllowedPath(path) {
    return ALLOWED_PATHS.some(prefix => path === prefix || path.startsWith(prefix + '/'));
  }

  // Will be set from Rust via on_page_load
  window.__mesaLoggedOut = true;

  function isInternalUrl(url) {
    // Always allow the core Messenger page
    if (url.startsWith('https://facebook.com/messages')) {
      return true;
    }

    try {
      const parsed = new URL(url);
      const host = parsed.hostname;
      const path = parsed.pathname;

      if (!isInternalHost(host)) {
        return false;
      }

      if (isAllowedPath(path)) {
        return true;
      }

      // When logged out, allow any Facebook URL so login flows work
      if (window.__mesaLoggedOut) {
        return true;
      }

      return false;
    } catch {
      return false;
    }
  }

  // ---- Log SPA navigations (history.pushState / replaceState) ----
  const origPushState = history.pushState;
  history.pushState = function (state, title, url) {
    console.log('[mesa-nav] pushState:', url, 'title:', title);
    return origPushState.apply(this, arguments);
  };
  const origReplaceState = history.replaceState;
  history.replaceState = function (state, title, url) {
    console.log('[mesa-nav] replaceState:', url, 'title:', title);
    return origReplaceState.apply(this, arguments);
  };

  // ---- Log hash changes ----
  window.addEventListener('hashchange', function () {
    console.log('[mesa-nav] hashchange:', location.href);
  });

  // ---- Intercept window.open ----
  const origOpen = window.open;
  window.open = function (url, target, features) {
    // about:blank / about:blank#blocked are used by Messenger for call popups
    if (url === 'about:blank' || url === 'about:blank#blocked') {
      return origOpen.call(window, url, target, features);
    }

    const verdict = url && isInternalUrl(url) ? 'internal' : 'external';
    console.log('[mesa-nav] window.open:', url, 'target:', target, verdict);
    if (url && !isInternalUrl(url)) {
      // Navigate the main page to trigger on_navigation which will open in browser
      window.location.href = url;
      return null;
    }
    return origOpen.call(window, url, target, features);
  };

  // ---- Intercept link clicks ----
  document.addEventListener('click', function (e) {
    // Walk up to find the anchor
    let el = e.target;
    while (el && el.tagName !== 'A') el = el.parentElement;
    if (!el || el.tagName !== 'A') return;

    const href = el.getAttribute('href');
    if (!href) return;
    if (href.startsWith('#') || href.toLowerCase().startsWith('javascript')) return;

    // Resolve relative URLs
    const fullUrl = href.startsWith('http') ? href : new URL(href, window.location.href).href;
    const text = (el.innerText || '').trim().slice(0, 80);
    const verdict = isInternalUrl(fullUrl) ? 'internal' : 'external';

    console.log('[mesa-nav] link click:', fullUrl, verdict, 'text:', text);

    if (verdict === 'external') {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      // Navigate page to trigger on_navigation, which will open externally
      window.location.href = fullUrl;
    }
  }, true); // capture phase

  console.log('[mesa-nav] navigation handler installed');
})();
