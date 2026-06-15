(() => {
  const styleId = {STYLE_ID};
  const css = {CSS};

  const ensureStyleLast = () => {
    const head = document.head || document.documentElement;
    let style = document.getElementById(styleId);

    if (!style) {
      style = document.createElement('style');
      style.id = styleId;
      style.type = 'text/css';
      style.textContent = css;
      head.appendChild(style);
      return;
    }

    if (style.textContent !== css) {
      style.textContent = css;
    }

    if (style.parentNode !== head || head.lastElementChild !== style) {
      head.appendChild(style);
    }
  };

  ensureStyleLast();

  if (!window.__mesaCssObserver) {
    window.__mesaCssObserver = new MutationObserver(() => ensureStyleLast());
    window.__mesaCssObserver.observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
  }
})();
