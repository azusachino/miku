/* ============================================================================
   miku.js — optional progressive enhancement. No bundler, no framework.
   Add with a single  <script defer src="/static/miku.js"></script>  in the shell.
   Everything here is OPTIONAL: the server-rendered UI works without it. This
   only adds theme/accent persistence and a couple of keyboard shortcuts.

   NOTE: live side-by-side Markdown preview is intentionally NOT here — per
   docs/architecture.md it is roadmap and pairs with CodeMirror 6. The edit
   view ships as the classic textarea until then.
   ============================================================================ */
(function () {
  'use strict';
  var root = document.documentElement;
  var LS_THEME = 'miku:theme';
  var LS_ACCENT = 'miku:accent';

  /* ---- Restore persisted theme + accent (FOUC-safe if you also inline the
     two getItem lines in <head>; see README) ---------------------------- */
  function apply(attr, key, fallback) {
    var v = null;
    try { v = localStorage.getItem(key); } catch (e) {}
    root.setAttribute(attr, v || fallback);
  }
  apply('data-theme', LS_THEME, 'dark');
  apply('data-accent', LS_ACCENT, 'miku');

  function setTheme(mode) {
    root.setAttribute('data-theme', mode);
    try { localStorage.setItem(LS_THEME, mode); } catch (e) {}
    syncActive();
  }
  function setAccent(name) {
    root.setAttribute('data-accent', name);
    try { localStorage.setItem(LS_ACCENT, name); } catch (e) {}
    syncActive();
  }

  /* ---- Code-block copy buttons ---------------------------------------------
     Rendered code blocks (comrak -> <pre><code>) ship no copy affordance, and
     Prism loads core+autoloader only (no toolbar plugin). This is a tiny
     vanilla injector — no extra CDN — that decorates each <pre> once. It is
     re-run wherever Prism highlights: initial render, htmx swaps, and the live
     preview panes (see the highlightAllUnder call sites in base/page/edit). */
  function injectCopyButtons(scope) {
    var container = scope || document;
    var pres = container.querySelectorAll('pre');
    pres.forEach(function (pre) {
      if (pre.dataset.mkCopy) return; // idempotent — safe to re-run on every swap
      var code = pre.querySelector('code');
      if (!code) return;
      pre.dataset.mkCopy = '1';
      pre.classList.add('mk-has-copy');
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'mk-copy-btn';
      btn.setAttribute('aria-label', 'Copy code');
      btn.textContent = '⧉ Copy';
      btn.addEventListener('click', function () {
        var text = code.innerText;
        var done = function () {
          btn.textContent = '✓ Copied';
          btn.classList.add('is-copied');
          setTimeout(function () { btn.textContent = '⧉ Copy'; btn.classList.remove('is-copied'); }, 1600);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done).catch(function () {});
        }
      });
      pre.appendChild(btn);
    });
  }
  window.mikuInjectCopyButtons = injectCopyButtons;
  document.addEventListener('htmx:afterSwap', function (e) {
    injectCopyButtons(e.detail && e.detail.target ? e.detail.target : document);
  });

  /* ---- Reflect current state onto controls (data-set-theme / data-set-accent) */
  function syncActive() {
    var theme = root.getAttribute('data-theme');
    var accent = root.getAttribute('data-accent');
    document.querySelectorAll('[data-set-theme]').forEach(function (el) {
      el.classList.toggle('is-active', el.getAttribute('data-set-theme') === theme);
    });
    document.querySelectorAll('[data-set-accent]').forEach(function (el) {
      el.classList.toggle('is-active', el.getAttribute('data-set-accent') === accent);
    });
  }

  document.addEventListener('click', function (e) {
    var t = e.target.closest('[data-set-theme]');
    if (t) { setTheme(t.getAttribute('data-set-theme')); return; }
    var a = e.target.closest('[data-set-accent]');
    if (a) { setAccent(a.getAttribute('data-set-accent')); return; }
  });

  /* ---- Keyboard: Cmd/Ctrl-N → new page --------------------------------
     Cmd/Ctrl-K (palette), Cmd-/ and Cmd-E are owned by the Alpine shell in
     base.html; only Cmd-N lives here to avoid a double-bound handler.        */
  document.addEventListener('keydown', function (e) {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.key.toLowerCase() === 'n') {
      var n = document.querySelector('[data-go-new]');
      if (n) { e.preventDefault(); (n.href ? (location.href = n.href) : n.click()); }
    }
  });

  /* ---- Create-page dialog: live-slug the path as the title is typed ----
     Markup contract: an input[data-create-title], a folder selector group of
     [data-folder] elements (data-folder = "" | "guides/" ...), and a
     [data-create-path] element to render "<folder><slug>.md" into.          */
  function slug(s) {
    return (s || '').trim().toLowerCase()
      .replace(/[^a-z0-9\u00C0-\uFFFF]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'untitled-page';
  }
  var titleEl = document.querySelector('[data-create-title]');
  if (titleEl) {
    var pathEl = document.querySelector('[data-create-path]');
    var folder = '';
    function repaint() { if (pathEl) pathEl.textContent = folder + slug(titleEl.value) + '.md'; }
    titleEl.addEventListener('input', repaint);
    document.querySelectorAll('[data-folder]').forEach(function (el) {
      el.addEventListener('click', function () {
        folder = el.getAttribute('data-folder') || '';
        document.querySelectorAll('[data-folder]').forEach(function (x) { x.classList.toggle('is-active', x === el); });
        repaint();
      });
    });
    repaint();
  }

  /* ---- Mermaid diagram zoom-magnify lightbox --------------------------- */
  function initMermaidZoom() {
    document.addEventListener('click', function (e) {
      var pre = e.target.closest('pre.mermaid');
      if (!pre) return;
      
      var svg = pre.querySelector('svg');
      if (!svg) return;
      
      e.preventDefault();
      e.stopPropagation();
      
      var overlay = document.createElement('div');
      overlay.className = 'mk-mermaid-lightbox';
      
      var closeBtn = document.createElement('button');
      closeBtn.className = 'mk-mermaid-lightbox-close';
      closeBtn.type = 'button';
      closeBtn.innerHTML = '&times;';
      closeBtn.setAttribute('aria-label', 'Close zoom view');
      
      var container = document.createElement('div');
      container.className = 'mk-mermaid-lightbox-container';
      
      var clonedSvg = svg.cloneNode(true);
      clonedSvg.removeAttribute('width');
      clonedSvg.removeAttribute('height');
      clonedSvg.style.width = 'auto';
      clonedSvg.style.height = 'auto';
      clonedSvg.style.maxWidth = '100%';
      clonedSvg.style.maxHeight = '100%';
      
      container.appendChild(clonedSvg);
      overlay.appendChild(closeBtn);
      overlay.appendChild(container);
      document.body.appendChild(overlay);
      
      requestAnimationFrame(function () {
        overlay.classList.add('is-active');
      });
      
      var scale = 1;
      var startX = 0, startY = 0;
      var translateX = 0, translateY = 0;
      var isDragging = false;
      
      function updateTransform() {
        container.style.transform = 'translate(' + translateX + 'px, ' + translateY + 'px) scale(' + scale + ')';
      }
      
      function wheelHandler(we) {
        we.preventDefault();
        var factor = 1.15;
        if (we.deltaY < 0) {
          scale *= factor;
        } else {
          scale /= factor;
        }
        scale = Math.min(Math.max(0.4, scale), 8.0);
        updateTransform();
      }
      overlay.addEventListener('wheel', wheelHandler, { passive: false });
      
      function dragStart(de) {
        de.stopPropagation();
        isDragging = true;
        startX = de.clientX - translateX;
        startY = de.clientY - translateY;
      }
      container.addEventListener('mousedown', dragStart);
      
      function dragMove(de) {
        if (!isDragging) return;
        translateX = de.clientX - startX;
        translateY = de.clientY - startY;
        updateTransform();
      }
      window.addEventListener('mousemove', dragMove);
      
      function dragEnd() {
        isDragging = false;
      }
      window.addEventListener('mouseup', dragEnd);
      
      function closeLightbox() {
        overlay.classList.remove('is-active');
        setTimeout(function () {
          if (overlay.parentNode) {
            overlay.parentNode.removeChild(overlay);
          }
        }, 200);
        
        window.removeEventListener('mousemove', dragMove);
        window.removeEventListener('mouseup', dragEnd);
        window.removeEventListener('keydown', keyHandler);
      }
      
      overlay.addEventListener('click', function (ce) {
        if (ce.target === overlay || ce.target === closeBtn || ce.target === container) {
          closeLightbox();
        }
      });
      
      function keyHandler(ke) {
        if (ke.key === 'Escape') {
          closeLightbox();
        }
      }
      window.addEventListener('keydown', keyHandler);
    });
  }
  initMermaidZoom();

  /* ---- Tree controller: the single owner of file-tree mutations -----------
     Replaces the old per-row inline move/trash forms (see the resolved pitfall
     miku:pitfall:inline-tree-forms). Nodes stay declarative and delegate every
     action here via `$store.tree`. Drag supports two hit modes only — into a
     folder, or to the root — because a filesystem folder has no sibling order
     (children render alphabetically), so before/after reordering is out.

     JSON contract (see src/main.rs):
       POST /api/v1/move          { from, to }  -> 200 {ok,path} | 409 {error:"exists"} | 404
       POST /api/v1/trash         { path }      -> 200 {ok,id,original_path} | 404
       GET  /api/v1/trash                       -> [{ id, original_path, title, trashed_at }]
       POST /api/v1/trash/restore { id }        -> 200 {ok,path} | 409 | 404
       POST /api/v1/trash/purge   { id }        -> 200 {ok}                                  */
  function postJSON(url, payload) {
    return fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
  }
  function relTime(secs) {
    var diff = Math.max(0, Math.floor(Date.now() / 1000) - secs);
    if (diff < 60) return 'just now';
    if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
    if (diff < 86400) return Math.floor(diff / 3600) + 'h ago';
    return Math.floor(diff / 86400) + 'd ago';
  }

  document.addEventListener('alpine:init', function () {
    window.Alpine.store('tree', {
      dragging: null,   // source path currently being dragged
      dropTarget: null, // folder path highlighted as the drop target ('' = root)
      menu: { open: false, x: 0, y: 0, path: '' },
      toast: { show: false, message: '', undo: null },
      _toastTimer: null,
      trashItems: [],
      trashLoaded: false,
      trashLoading: false,

      relTime: relTime,

      /* A successful move reloads (so the page shows in its new home), which
         would drop any in-memory toast — so the reverse move is stashed in
         sessionStorage and re-surfaced as an Undo toast after the reload.
         Mirrors trash's undo affordance; guards miku:pitfall:silent-move-to-root. */
      init: function () {
        var raw;
        try { raw = sessionStorage.getItem('miku:moveUndo'); } catch (e) { return; }
        if (!raw) return;
        try { sessionStorage.removeItem('miku:moveUndo'); } catch (e) {}
        var u;
        try { u = JSON.parse(raw); } catch (e) { return; }
        if (!u || !u.from || !u.to) return;
        this.showToast('Moved to “' + u.from + '”.', function () {
          // Reverse move runs directly (not via move()) so it doesn't re-stash.
          postJSON('/api/v1/move', { from: u.from, to: u.to }).then(function () { window.location.reload(); });
        });
      },

      /* drag ------------------------------------------------------------- */
      startDrag: function (path, ev) {
        this.dragging = path;
        if (ev && ev.dataTransfer) {
          ev.dataTransfer.effectAllowed = 'move';
          ev.dataTransfer.setData('text/plain', path);
        }
      },
      endDrag: function () { this.dragging = null; this.dropTarget = null; },
      // Drop `from` into `folder` ('' = root): keep the basename, swap the parent.
      dropInto: function (folder, ev) {
        this.dropTarget = null;
        var from = this.dragging || (ev && ev.dataTransfer && ev.dataTransfer.getData('text/plain'));
        this.dragging = null;
        if (!from) return;
        var base = from.split('/').pop();
        var to = folder ? folder + '/' + base : base;
        this.move(from, to);
      },

      /* move / rename ---------------------------------------------------- */
      move: function (from, to) {
        if (!to || from === to) return;
        var self = this;
      postJSON('/api/v1/move', { from: from, to: to })
          .then(function (r) {
            if (r.status === 409) {
              self.showToast('A page already exists at “' + to + '”.', null);
              return null;
            }
            if (r.status === 404) {
              self.showToast('That page no longer exists.', null);
              return null;
            }
            if (!r.ok) { window.location.reload(); return null; }
            return r.json();
          })
          .then(function (data) {
            // Success reloads so the moved page appears in its new home. The
            // native-feel optimistic version is tracked separately (ux-2.0).
            // Stash the reverse move so init() can offer an Undo post-reload.
            if (data && data.ok) {
              try { sessionStorage.setItem('miku:moveUndo', JSON.stringify({ from: to, to: from })); } catch (e) {}
              window.location.reload();
            }
          })
          .catch(function () { window.location.reload(); });
      },
      // Prompt-based rename/move (a styled modal is a follow-up).
      renamePrompt: function (path) {
        this.closeMenu();
        var next = window.prompt('Rename or move page (full path, no .md):', path);
        if (next === null) return;
        next = next.trim().replace(/^\/+|\/+$/g, '');
        if (next) this.move(path, next);
      },

      /* trash ------------------------------------------------------------ */
      trash: function (path) {
        this.closeMenu();
        var self = this;
      postJSON('/api/v1/trash', { path: path })
          .then(function (r) { return r.ok ? r.json() : null; })
          .then(function (data) {
            if (!data || !data.ok) { window.location.reload(); return; }
            // Remove the row in place and offer an Undo — no reload needed.
            document.querySelectorAll('[data-tree-path="' + (window.CSS && CSS.escape ? CSS.escape(path) : path) + '"]').forEach(function (el) { el.remove(); });
            self.trashLoaded = false; // force the Trash view to refetch next open
            var id = data.id;
            self.showToast('Moved “' + path + '” to Trash.', function () {
              postJSON('/api/v1/trash/restore', { id: id }).then(function () { window.location.reload(); });
            });
          })
          .catch(function () { window.location.reload(); });
      },

      /* trash view ------------------------------------------------------- */
      loadTrash: function () {
        if (this.trashLoaded || this.trashLoading) return;
        this.trashLoading = true;
        var self = this;
      fetch('/api/v1/trash')
          .then(function (r) { return r.ok ? r.json() : []; })
          .then(function (items) { self.trashItems = items || []; self.trashLoaded = true; })
          .catch(function () { self.trashItems = []; })
          .finally(function () { self.trashLoading = false; });
      },
      restore: function (id) {
        var self = this;
      postJSON('/api/v1/trash/restore', { id: id })
          .then(function (r) {
            if (r.status === 409) { self.showToast('A page already exists at that path.', null); return; }
            window.location.reload();
          });
      },
      purge: function (id) {
        if (!window.confirm('Delete this page forever? This cannot be undone.')) return;
        var self = this;
      postJSON('/api/v1/trash/purge', { id: id }).then(function () {
          self.trashItems = self.trashItems.filter(function (it) { return it.id !== id; });
        });
      },

      /* context menu ----------------------------------------------------- */
      openMenu: function (ev, path) {
        ev.preventDefault();
        ev.stopPropagation();
        this.menu = { open: true, x: ev.clientX, y: ev.clientY, path: path };
      },
      closeMenu: function () { this.menu.open = false; },

      /* toast ------------------------------------------------------------ */
      showToast: function (message, undo) {
        var self = this;
        if (this._toastTimer) clearTimeout(this._toastTimer);
        this.toast = { show: true, message: message, undo: undo };
        this._toastTimer = setTimeout(function () { self.toast.show = false; }, undo ? 8000 : 4000);
      },
      runUndo: function () {
        var undo = this.toast.undo;
        this.toast.show = false;
        if (undo) undo();
      },
    });
  });

  syncActive();
  injectCopyButtons(document);
})();
