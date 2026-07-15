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
  "use strict";
  var root = document.documentElement;
  var storage = window.mikuStorage;

  /* ---- Restore persisted theme + accent (FOUC-safe if you also inline the
     two getItem lines in <head>; see README) ---------------------------- */
  function apply(attr, key, fallback) {
    var v = storage ? storage.get(key, null) : null;
    root.setAttribute(attr, v || fallback);
  }
  apply("data-theme", "theme", "dark");
  apply("data-accent", "accent", "miku");

  function syncBrandAssets() {
    var suffix = root.getAttribute("data-theme") === "light" ? "light" : "dark";
    var asset = "/static/miku-icon-" + suffix + ".svg?theme=" + suffix;
    document.querySelectorAll("[data-miku-brand-icon]").forEach(function (image) {
      if (image.getAttribute("src") !== asset) image.setAttribute("src", asset);
    });
    var favicon = document.getElementById("miku-favicon");
    if (favicon) {
      // Keep the tab icon local to the document. Chromium may revalidate a
      // network favicon after history.pushState, even when the reader shell
      // remains mounted and only the page fragment changes.
      var fill = suffix === "light" ? "%23e6faf6" : "%23232d35";
      var faviconAsset = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'%3E%3Crect width='100' height='100' rx='22' fill='" + fill + "'/%3E%3Ccircle cx='50' cy='48' r='28' fill='%236fdacf'/%3E%3Ccircle cx='41' cy='47' r='4' fill='%233e4e50'/%3E%3Ccircle cx='59' cy='47' r='4' fill='%233e4e50'/%3E%3Cpath d='M43 57 Q50 63 57 57' fill='none' stroke='%233e4e50' stroke-width='3' stroke-linecap='round'/%3E%3C/svg%3E";
      if (favicon.getAttribute("href") !== faviconAsset) favicon.setAttribute("href", faviconAsset);
    }
  }

  function initContentSearch() {
    var form = document.querySelector("[data-content-search]");
    if (!form || form.dataset.initialized) return;
    form.dataset.initialized = "1";
    var input = form.querySelector("[data-content-search-input]");
    var regex = form.querySelector("[data-content-search-regex]");
    var results = document.querySelector("[data-content-search-results]");
    var sentinel = document.querySelector("[data-content-search-sentinel]");
    if (!input || !results || !sentinel) return;
    var offset = 0;
    var loading = false;
    var request = null;
    var timer = null;

    function escapeHtml(value) {
      return String(value).replace(/[&<>\"']/g, function (char) {
        return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char];
      });
    }
    function pageHref(path) {
      return "/p/" + path.split("/").map(encodeURIComponent).join("/");
    }
    function appendFiles(files) {
      files.forEach(function (file) {
        var matches = file.matches.map(function (match) {
          return '<a class="mk-content-match" href="' + pageHref(file.path) + '"><span class="mk-content-line">' + match.line + '</span><code>' + escapeHtml(match.text) + '</code></a>';
        }).join("");
        results.insertAdjacentHTML("beforeend", '<article class="mk-content-file mk-card"><a class="mk-content-file-title" href="' + pageHref(file.path) + '">' + escapeHtml(file.title) + '</a><span class="mk-content-file-path">' + escapeHtml(file.path) + '.md</span><div class="mk-content-matches">' + matches + '</div></article>');
      });
    }
    async function load(reset) {
      var query = input.value.trim();
      if (loading || !query) {
        if (!query && reset) results.replaceChildren();
        return;
      }
      if (reset) {
        offset = 0;
        results.replaceChildren();
      }
      if (request) request.abort();
      request = new AbortController();
      loading = true;
      sentinel.textContent = "Searching…";
      try {
        var url = "/api/v1/content-search?q=" + encodeURIComponent(query) + "&offset=" + offset + "&limit=10&regex=" + (regex && regex.checked ? "true" : "false");
        var response = await fetch(url, { headers: { Accept: "application/json" }, signal: request.signal });
        if (!response.ok) throw new Error("content search failed");
        var payload = await response.json();
        appendFiles(payload.files || []);
        offset = payload.next_offset || offset;
        sentinel.textContent = payload.has_more ? "Scroll for more matches" : ((payload.files || []).length ? "End of results" : "No matching content found.");
        sentinel.dataset.done = payload.has_more ? "false" : "true";
      } catch (error) {
        if (error.name !== "AbortError") sentinel.textContent = "Content search failed.";
      } finally {
        loading = false;
      }
    }
    form.addEventListener("submit", function (event) { event.preventDefault(); load(true); });
    input.addEventListener("input", function () {
      window.clearTimeout(timer);
      timer = window.setTimeout(function () { load(true); }, 220);
    });
    new IntersectionObserver(function (entries) {
      if (entries[0].isIntersecting && sentinel.dataset.done !== "true") load(false);
    }, { rootMargin: "360px 0px" }).observe(sentinel);
    if (input.value.trim()) load(true);
  }

  function setTheme(mode) {
    root.setAttribute("data-theme", mode);
    if (storage) storage.set("theme", mode);
    syncBrandAssets();
    syncActive();
  }
  function setAccent(name) {
    root.setAttribute("data-accent", name);
    if (storage) storage.set("accent", name);
    syncActive();
  }

  /* ---- Code-block copy buttons ---------------------------------------------
     Rendered code blocks (comrak -> <pre><code>) ship no copy affordance, and
     Prism is loaded lazily only when a reader fragment contains a code block.
     This is a tiny vanilla injector — no extra CDN — that decorates each
     <pre> once. It is re-run after fragment swaps and live preview updates. */
  function injectCopyButtons(scope) {
    var container = scope || document;
    var pres = container.querySelectorAll("pre");
    pres.forEach(function (pre) {
      if (pre.dataset.mkCopy) return; // idempotent — safe to re-run on every swap
      var code = pre.querySelector("code");
      if (!code) return;
      pre.dataset.mkCopy = "1";
      pre.classList.add("mk-has-copy");
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "mk-copy-btn";
      btn.setAttribute("aria-label", "Copy code");
      btn.textContent = "⧉ Copy";
      btn.addEventListener("click", function () {
        var text = code.innerText;
        var done = function () {
          btn.textContent = "✓ Copied";
          btn.classList.add("is-copied");
          setTimeout(function () {
            btn.textContent = "⧉ Copy";
            btn.classList.remove("is-copied");
          }, 1600);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard
            .writeText(text)
            .then(done)
            .catch(function () {});
        }
      });
      pre.appendChild(btn);
    });
  }
  window.mikuInjectCopyButtons = injectCopyButtons;
  document.addEventListener("htmx:afterSwap", function (e) {
    injectCopyButtons(e.detail && e.detail.target ? e.detail.target : document);
  });

  /* ---- Optional reader enhancements ------------------------------------
     The shell must remain cheap. Syntax highlighting and Mermaid are loaded
     only when the current content actually needs them, and each CDN script is
     requested at most once for the lifetime of this page. */
  function loadScriptOnce(src, key) {
    var existing = document.querySelector('script[data-miku-loader="' + key + '"]');
    if (existing && existing._mikuPromise) return existing._mikuPromise;
    var script = existing || document.createElement("script");
    script.dataset.mikuLoader = key;
    script.src = src;
    script.async = true;
    script._mikuPromise = new Promise(function (resolve, reject) {
      script.addEventListener("load", resolve, { once: true });
      script.addEventListener("error", reject, { once: true });
    });
    if (!existing) document.head.appendChild(script);
    return script._mikuPromise;
  }

  function loadStylesheetOnce(href, key) {
    var existing = document.querySelector('link[data-miku-loader="' + key + '"]');
    if (existing && existing._mikuPromise) return existing._mikuPromise;
    var link = existing || document.createElement("link");
    link.dataset.mikuLoader = key;
    link.rel = "stylesheet";
    link.href = href;
    link._mikuPromise = new Promise(function (resolve, reject) {
      link.addEventListener("load", resolve, { once: true });
      link.addEventListener("error", reject, { once: true });
    });
    if (!existing) document.head.appendChild(link);
    return link._mikuPromise;
  }

  function ensurePrism(scope) {
    var container = scope || document;
    var blocks = Array.from(container.querySelectorAll('pre code[class*="language-"]')).filter(function (code) {
      return !code.classList.contains("language-mermaid");
    });
    if (!blocks.length) return Promise.resolve();
    var themeHref =
      root.getAttribute("data-theme") === "dark"
        ? "https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/themes/prism-tomorrow.min.css"
        : "https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/themes/prism.min.css";
    var theme = loadStylesheetOnce(themeHref, "prism-theme");
    var themeLink = document.querySelector('link[data-miku-loader="prism-theme"]');
    themeLink.id = "prism-theme";
    return theme
      .then(function () {
        return loadScriptOnce("https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/components/prism-core.min.js", "prism-core");
      })
      .then(function () {
        return loadScriptOnce("https://cdnjs.cloudflare.com/ajax/libs/prism/1.29.0/plugins/autoloader/prism-autoloader.min.js", "prism-autoloader");
      })
      .then(function () {
        if (window.Prism) window.Prism.highlightAllUnder(container);
      });
  }

  function ensureMermaid(scope) {
    var container = scope || document;
    var diagrams = container.querySelectorAll("pre.mermaid, code.language-mermaid");
    if (!diagrams.length) return Promise.resolve();
    var ready = window.mermaid ? Promise.resolve() : loadScriptOnce("https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js", "mermaid");
    return ready.then(function () {
      if (!window.mermaid) return;
      window.mermaid.initialize({
        startOnLoad: false,
        theme: root.getAttribute("data-theme") === "dark" ? "dark" : "default"
      });
      return window.mermaid.run({
        nodes: Array.from(container.querySelectorAll("pre.mermaid, .mermaid:not(svg)"))
      });
    });
  }

  function ensureKatex(scope) {
    var container = scope || document;
    var formulas = container.querySelectorAll("[data-math-style]");
    if (!formulas.length) return Promise.resolve();
    var css = loadStylesheetOnce("https://cdn.jsdelivr.net/npm/katex@0.16.22/dist/katex.min.css", "katex-css");
    var js = window.katex ? Promise.resolve() : loadScriptOnce("https://cdn.jsdelivr.net/npm/katex@0.16.22/dist/katex.min.js", "katex");
    return Promise.all([css, js]).then(function () {
      if (!window.katex) return;
      Array.from(formulas).forEach(function (formula) {
        if (formula.dataset.mathRendered === "true") return;
        window.katex.render(formula.textContent || "", formula, {
          displayMode: formula.getAttribute("data-math-style") === "display",
          throwOnError: false,
          trust: false
        });
        formula.dataset.mathRendered = "true";
      });
    });
  }

  function enhanceReaderContent(scope) {
    var container = scope || document;
    return Promise.all([ensurePrism(container), ensureMermaid(container), ensureKatex(container)]).then(function () {
      injectCopyButtons(container);
    });
  }
  window.mikuEnhanceReaderContent = enhanceReaderContent;
  document.addEventListener("DOMContentLoaded", function () {
    enhanceReaderContent(document);
  });

  /* ---- Reader freshness --------------------------------------------------
     Reading mode does not hold an idle SSE connection open. Check the active
     page occasionally and immediately when the tab becomes visible again. */
  function readerApiPath(path) {
    return "/api/v1/pages/" + path.split("/").map(encodeURIComponent).join("/");
  }

  function refreshCurrentReaderPage() {
    var article = document.querySelector(".mk-article[data-page-path]");
    if (!article || document.hidden) return Promise.resolve();
    var path = article.getAttribute("data-page-path");
    return fetch(readerApiPath(path), {
      headers: { Accept: "application/json", "Cache-Control": "no-cache" },
      cache: "no-store"
    })
      .then(function (response) {
        if (!response.ok) return null;
        return response.json();
      })
      .then(function (payload) {
        if (!payload || !payload.html || payload.updated === article.dataset.pageUpdated) return;
        var fresh = document.createElement("div");
        fresh.innerHTML = payload.html;
        var freshArticle = fresh.querySelector(".mk-article");
        if (!freshArticle || freshArticle.getAttribute("data-page-path") !== path) return;
        article.innerHTML = freshArticle.innerHTML;
        article.dataset.pageUpdated = freshArticle.dataset.pageUpdated || payload.updated || "";
        return enhanceReaderContent(article);
      })
      .catch(function () {
        /* A failed freshness check must not disturb reading. */
      });
  }
  window.setInterval(refreshCurrentReaderPage, 60 * 1000);
  document.addEventListener("visibilitychange", function () {
    if (!document.hidden) refreshCurrentReaderPage();
  });

  /* ---- Paginated tag index ---------------------------------------------- */
  function appendTagCard(cloud, tag) {
    var link = document.createElement("a");
    link.href = "/tags/" + encodeURIComponent(tag.tag);
    link.className = "inline-flex items-center gap-2 rounded-2xl border font-bold transition-all hover:scale-[1.02] shadow-sm no-underline";
    if (tag.count >= 10) {
      link.className += " text-lg px-5 py-3.5 bg-teal/10 text-teal border-teal/30 hover:border-teal/50";
    } else if (tag.count >= 5) {
      link.className += " text-base px-4.5 py-2.75 bg-teal/5 text-teal/90 border-teal/20 hover:border-teal/40";
    } else {
      link.className += " text-xs px-3 py-1.75 bg-panel/15 text-muted hover:text-text-base border-border/15 hover:border-border/30";
    }
    var name = document.createElement("span");
    name.textContent = "#" + tag.tag;
    var count = document.createElement("span");
    count.className = "px-1.5 py-0.5 rounded text-[10px] font-extrabold bg-panel border border-border/10 opacity-70";
    count.textContent = tag.count;
    link.append(name, count);
    cloud.appendChild(link);
  }

  function appendTagPageCard(list, page) {
    var link = document.createElement("a");
    link.href = "/p/" + page.path;
    link.className =
      "group block p-5 rounded-2xl border border-border/15 bg-panel/10 hover:bg-panel/20 hover:border-teal/30 hover:shadow-[0_8px_24px_-10px_var(--tealglow)] transition-all duration-200 no-underline";
    var title = document.createElement("span");
    title.className = "font-display font-bold text-base text-text-base group-hover:text-teal transition-colors";
    title.textContent = page.title;
    var titleRow = document.createElement("div");
    titleRow.className = "flex items-center gap-1.5 mb-1.5";
    titleRow.appendChild(title);
    var path = document.createElement("p");
    path.className = "font-mono text-[10px] text-muted/60 truncate bg-panel/30 border border-border/5 px-2 py-0.5 rounded-md inline-block";
    path.textContent = page.path + ".md";
    link.append(titleRow, path);
    list.appendChild(link);
  }

  function observeTagIndex() {
    var sentinel = document.querySelector("[data-tag-sentinel]");
    var cloud = document.querySelector("[data-tag-cloud]");
    if (!sentinel || !cloud || !window.IntersectionObserver) return;
    var loading = false;
    var observer = new IntersectionObserver(
      function (entries) {
        if (
          !entries.some(function (entry) {
            return entry.isIntersecting;
          }) ||
          loading
        )
          return;
        loading = true;
        sentinel.textContent = "Loading more tags…";
        fetch("/api/v1/tags?offset=" + sentinel.dataset.offset + "&limit=" + sentinel.dataset.limit, {
          headers: { Accept: "application/json" }
        })
          .then(function (response) {
            if (!response.ok) throw new Error("Tag page request failed");
            return response.json();
          })
          .then(function (payload) {
            (payload.tags || []).forEach(function (tag) {
              appendTagCard(cloud, tag);
            });
            sentinel.dataset.offset = String(payload.next_offset || 0);
            if (payload.has_more) {
              sentinel.textContent = "Scroll to load more…";
            } else {
              observer.disconnect();
              sentinel.remove();
            }
          })
          .catch(function () {
            sentinel.textContent = "Could not load more tags. Scroll to retry.";
          })
          .finally(function () {
            loading = false;
          });
      },
      { rootMargin: "420px 0px" }
    );
    observer.observe(sentinel);
  }

  function observeTagResults() {
    var sentinel = document.querySelector("[data-tag-page-sentinel]");
    if (!sentinel || !window.IntersectionObserver) return;
    var loading = false;
    var observer = new IntersectionObserver(
      function (entries) {
        if (
          !entries.some(function (entry) {
            return entry.isIntersecting;
          }) ||
          loading
        )
          return;
        var root = sentinel.closest(".mk-tag-results");
        var list = root && root.querySelector("[data-tag-page-list]");
        if (!root || !list) return;
        loading = true;
        sentinel.textContent = "Loading more pages…";
        fetch("/api/v1/tags/" + encodeURIComponent(root.dataset.tag) + "/pages?offset=" + sentinel.dataset.offset + "&limit=" + sentinel.dataset.limit, {
          headers: { Accept: "application/json" }
        })
          .then(function (response) {
            if (!response.ok) throw new Error("Tag result page request failed");
            return response.json();
          })
          .then(function (payload) {
            (payload.pages || []).forEach(function (page) {
              appendTagPageCard(list, page);
            });
            sentinel.dataset.offset = String(payload.next_offset || 0);
            if (payload.has_more) {
              sentinel.textContent = "Scroll to load more…";
            } else {
              observer.disconnect();
              sentinel.remove();
            }
          })
          .catch(function () {
            sentinel.textContent = "Could not load more pages. Scroll to retry.";
          })
          .finally(function () {
            loading = false;
          });
      },
      { rootMargin: "420px 0px" }
    );
    observer.observe(sentinel);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", observeTagResults);
    document.addEventListener("DOMContentLoaded", observeTagIndex);
  } else {
    observeTagResults();
    observeTagIndex();
  }

  /* ---- Reflect current state onto controls (data-set-theme / data-set-accent) */
  function syncActive() {
    var theme = root.getAttribute("data-theme");
    var accent = root.getAttribute("data-accent");
    document.querySelectorAll("[data-set-theme]").forEach(function (el) {
      el.classList.toggle("is-active", el.getAttribute("data-set-theme") === theme);
    });
    document.querySelectorAll("[data-set-accent]").forEach(function (el) {
      el.classList.toggle("is-active", el.getAttribute("data-set-accent") === accent);
    });
  }

  document.addEventListener("click", function (e) {
    var t = e.target.closest("[data-set-theme]");
    if (t) {
      setTheme(t.getAttribute("data-set-theme"));
      return;
    }
    var a = e.target.closest("[data-set-accent]");
    if (a) {
      setAccent(a.getAttribute("data-set-accent"));
      return;
    }
  });
  syncActive();
  syncBrandAssets();
  initContentSearch();

  if (window.htmx) {
    window.htmx.config.historyCacheSize = 0;
    window.htmx.config.refreshOnHistoryMiss = true;
  }

  /* ---- Persistent reader navigation ------------------------------------
     The shell stays mounted while Rust supplies a server-rendered reader
     fragment. Direct /p/{path} requests remain the canonical SSR route. */
  var readerNavigation = (function () {
    var navigating = false;

    function scrollToHash(hash) {
      if (!hash) return;
      var id = decodeURIComponent(hash.replace(/^#/, ""));
      var target = document.getElementById(id);
      var main = document.querySelector(".mk-main");
      if (!target || !main) return;
      var mainRect = main.getBoundingClientRect();
      var targetRect = target.getBoundingClientRect();
      main.scrollTo({
        top: Math.max(0, main.scrollTop + targetRect.top - mainRect.top - 24),
        behavior: "auto"
      });
    }

    function restoreHash(hash) {
      if (!hash) return;
      window.requestAnimationFrame(function () {
        scrollToHash(hash);
        window.requestAnimationFrame(function () { scrollToHash(hash); });
      });
      window.setTimeout(function () { scrollToHash(hash); }, 180);
    }

    function setActive(path) {
      document.querySelectorAll(".tree-link").forEach(function (link) {
        var active = link.getAttribute("href") === "/p/" + path;
        link.classList.toggle("is-active", active);
        link.classList.toggle("active", active);
      });
    }

    function load(url, push) {
      if (navigating) return Promise.resolve();
      navigating = true;
      document.documentElement.setAttribute("data-nav-loading", "true");
      var targetUrl = new URL(url, window.location.origin);
      var pagePath = targetUrl.pathname.slice("/p/".length);
      try {
        // URL.pathname is already percent-encoded. Decode once before
        // readerApiPath encodes each logical path segment for the API.
        pagePath = decodeURIComponent(pagePath);
      } catch (error) {
        // Let the API request fail normally for malformed URLs.
      }
      return fetch(readerApiPath(pagePath), {
        headers: { Accept: "application/json" }
      })
        .then(function (response) {
          if (!response.ok) throw new Error("Reader page request failed");
          return response.json();
        })
        .then(function (payload) {
          if (!payload || typeof payload.html !== "string" || !payload.html) {
            throw new Error("Reader page response has no HTML fragment");
          }
          var view = document.querySelector(".mk-view");
          if (!view) throw new Error("Reader view is missing");
          view.innerHTML = payload.html;
          if (push) history.pushState({}, "", targetUrl.pathname + targetUrl.search + targetUrl.hash);
          document.title = "Miku Note - " + (payload.path || "");
          setActive(payload.path || "");
          if (targetUrl.hash) {
            restoreHash(targetUrl.hash);
          } else {
            document.querySelector(".mk-main").scrollTo(0, 0);
          }
          if (window.Alpine && window.Alpine.initTree) window.Alpine.initTree(view);
          return enhanceReaderContent(view);
        })
        .catch(function () {
          window.location.href = targetUrl.pathname + targetUrl.search + targetUrl.hash;
        })
        .finally(function () {
          navigating = false;
          document.documentElement.removeAttribute("data-nav-loading");
        });
    }

    document.addEventListener("click", function (event) {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      var link = event.target.closest("a");
      if (!link || link.target === "_blank") return;
      var href = link.getAttribute("href");
      if (!href || href.indexOf("/p/") !== 0 || href.indexOf("/edit") !== -1) return;
      event.preventDefault();
      load(href, true);
    });

    document.addEventListener("click", function (event) {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      var link = event.target.closest(".mk-toc a[href^='#']");
      if (!link) return;
      var hash = link.getAttribute("href");
      if (!hash || hash === "#") return;
      var target = document.getElementById(decodeURIComponent(hash.slice(1)));
      if (!target) return;
      event.preventDefault();
      history.replaceState({}, "", window.location.pathname + window.location.search + hash);
      scrollToHash(hash);
    });

    if (window.location.hash) restoreHash(window.location.hash);

    window.addEventListener("popstate", function () {
      if (location.pathname.indexOf("/p/") === 0 && location.pathname.indexOf("/edit") === -1) {
        load(location.pathname + location.search + location.hash, false);
      }
    });
  })();

  /* ---- Keyboard: Cmd/Ctrl-N → new page --------------------------------
     Cmd/Ctrl-K (palette), Cmd-/ and Cmd-E are owned by the Alpine shell in
     base.html; only Cmd-N lives here to avoid a double-bound handler.        */
  document.addEventListener("keydown", function (e) {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.key.toLowerCase() === "n") {
      var n = document.querySelector("[data-go-new]");
      if (n) {
        e.preventDefault();
        n.href ? (location.href = n.href) : n.click();
      }
    }
  });

  /* ---- Create-page dialog: live-slug the path as the title is typed ----
     Markup contract: an input[data-create-title], a folder selector group of
     [data-folder] elements (data-folder = "" | "guides/" ...), and a
     [data-create-path] element to render "<folder><slug>.md" into.          */
  function slug(s) {
    return (
      (s || "")
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9\u00C0-\uFFFF]+/g, "-")
        .replace(/^-+|-+$/g, "") || "untitled-page"
    );
  }
  var titleEl = document.querySelector("[data-create-title]");
  if (titleEl) {
    var pathEl = document.querySelector("[data-create-path]");
    var folder = "";
    function repaint() {
      if (pathEl) pathEl.textContent = folder + slug(titleEl.value) + ".md";
    }
    titleEl.addEventListener("input", repaint);
    document.querySelectorAll("[data-folder]").forEach(function (el) {
      el.addEventListener("click", function () {
        folder = el.getAttribute("data-folder") || "";
        document.querySelectorAll("[data-folder]").forEach(function (x) {
          x.classList.toggle("is-active", x === el);
        });
        repaint();
      });
    });
    repaint();
  }

  /* ---- Mermaid diagram zoom-magnify lightbox --------------------------- */
  function initMermaidZoom() {
    document.addEventListener("click", function (e) {
      var pre = e.target.closest("pre.mermaid");
      if (!pre) return;

      var svg = pre.querySelector("svg");
      if (!svg) return;

      e.preventDefault();
      e.stopPropagation();

      var overlay = document.createElement("div");
      overlay.className = "mk-mermaid-lightbox";

      var closeBtn = document.createElement("button");
      closeBtn.className = "mk-mermaid-lightbox-close";
      closeBtn.type = "button";
      closeBtn.innerHTML = "&times;";
      closeBtn.setAttribute("aria-label", "Close zoom view");

      var container = document.createElement("div");
      container.className = "mk-mermaid-lightbox-container";

      var clonedSvg = svg.cloneNode(true);
      clonedSvg.removeAttribute("width");
      clonedSvg.removeAttribute("height");
      clonedSvg.style.width = "auto";
      clonedSvg.style.height = "auto";
      clonedSvg.style.maxWidth = "100%";
      clonedSvg.style.maxHeight = "100%";

      container.appendChild(clonedSvg);
      overlay.appendChild(closeBtn);
      overlay.appendChild(container);
      document.body.appendChild(overlay);

      requestAnimationFrame(function () {
        overlay.classList.add("is-active");
      });

      var scale = 1;
      var startX = 0,
        startY = 0;
      var translateX = 0,
        translateY = 0;
      var isDragging = false;

      function updateTransform() {
        container.style.transform = "translate(" + translateX + "px, " + translateY + "px) scale(" + scale + ")";
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
      overlay.addEventListener("wheel", wheelHandler, { passive: false });

      function dragStart(de) {
        de.stopPropagation();
        isDragging = true;
        startX = de.clientX - translateX;
        startY = de.clientY - translateY;
      }
      container.addEventListener("mousedown", dragStart);

      function dragMove(de) {
        if (!isDragging) return;
        translateX = de.clientX - startX;
        translateY = de.clientY - startY;
        updateTransform();
      }
      window.addEventListener("mousemove", dragMove);

      function dragEnd() {
        isDragging = false;
      }
      window.addEventListener("mouseup", dragEnd);

      function closeLightbox() {
        overlay.classList.remove("is-active");
        setTimeout(function () {
          if (overlay.parentNode) {
            overlay.parentNode.removeChild(overlay);
          }
        }, 200);

        window.removeEventListener("mousemove", dragMove);
        window.removeEventListener("mouseup", dragEnd);
        window.removeEventListener("keydown", keyHandler);
      }

      overlay.addEventListener("click", function (ce) {
        if (ce.target === overlay || ce.target === closeBtn || ce.target === container) {
          closeLightbox();
        }
      });

      function keyHandler(ke) {
        if (ke.key === "Escape") {
          closeLightbox();
        }
      }
      window.addEventListener("keydown", keyHandler);
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
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });
  }
  function relTime(secs) {
    var diff = Math.max(0, Math.floor(Date.now() / 1000) - secs);
    if (diff < 60) return "just now";
    if (diff < 3600) return Math.floor(diff / 60) + "m ago";
    if (diff < 86400) return Math.floor(diff / 3600) + "h ago";
    return Math.floor(diff / 86400) + "d ago";
  }

  document.addEventListener("alpine:init", function () {
    window.Alpine.store("tree", {
      dragging: null, // source path currently being dragged
      dropTarget: null, // folder path highlighted as the drop target ('' = root)
      menu: { open: false, x: 0, y: 0, path: "" },
      toast: { show: false, message: "", undo: null },
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
        try {
          raw = sessionStorage.getItem("miku:moveUndo");
        } catch (e) {
          return;
        }
        if (!raw) return;
        try {
          sessionStorage.removeItem("miku:moveUndo");
        } catch (e) {}
        var u;
        try {
          u = JSON.parse(raw);
        } catch (e) {
          return;
        }
        if (!u || !u.from || !u.to) return;
        this.showToast("Moved to “" + u.from + "”.", function () {
          // Reverse move runs directly (not via move()) so it doesn't re-stash.
          postJSON("/api/v1/move", { from: u.from, to: u.to }).then(function () {
            window.location.reload();
          });
        });
      },

      /* drag ------------------------------------------------------------- */
      startDrag: function (path, ev) {
        this.dragging = path;
        if (ev && ev.dataTransfer) {
          ev.dataTransfer.effectAllowed = "move";
          ev.dataTransfer.setData("text/plain", path);
        }
      },
      endDrag: function () {
        this.dragging = null;
        this.dropTarget = null;
      },
      // Drop `from` into `folder` ('' = root): keep the basename, swap the parent.
      dropInto: function (folder, ev) {
        this.dropTarget = null;
        var from = this.dragging || (ev && ev.dataTransfer && ev.dataTransfer.getData("text/plain"));
        this.dragging = null;
        if (!from) return;
        var base = from.split("/").pop();
        var to = folder ? folder + "/" + base : base;
        this.move(from, to);
      },

      /* move / rename ---------------------------------------------------- */
      move: function (from, to) {
        if (!to || from === to) return;
        var self = this;
        postJSON("/api/v1/move", { from: from, to: to })
          .then(function (r) {
            return r.json().then(function (data) {
              return { ok: r.ok, status: r.status, data: data };
            }).catch(function () {
              return { ok: r.ok, status: r.status, data: null };
            });
          })
          .then(function (result) {
            var data = result && result.data;
            if (!result || !result.ok || !data || !data.ok) {
              var message = result && result.status === 409
                ? "A page already exists at “" + to + "”."
                : result && result.status === 404
                  ? "That page no longer exists."
                  : (data && data.error) || "The page could not be moved.";
              self.showToast(message, null);
              return;
            }
            // Success reloads so the moved page appears in its new home. The
            // native-feel optimistic version is tracked separately (ux-2.0).
            // Stash the reverse move so init() can offer an Undo post-reload.
            if (data && data.ok) {
              try {
                sessionStorage.setItem("miku:moveUndo", JSON.stringify({ from: to, to: from }));
              } catch (e) {}
              window.location.reload();
            }
          })
          .catch(function () {
            self.showToast("The page could not be moved.", null);
          });
      },

      /* trash ------------------------------------------------------------ */
      trash: function (path) {
        this.closeMenu();
        var self = this;
        postJSON("/api/v1/trash", { path: path })
          .then(function (r) {
            return r.json().then(function (data) {
              return { ok: r.ok, data: data };
            }).catch(function () {
              return { ok: r.ok, data: null };
            });
          })
          .then(function (result) {
            var data = result && result.data;
            if (!result || !result.ok || !data || !data.ok) {
              self.showToast((data && data.error) || "The page could not be moved to Trash.", null);
              return;
            }
            // Remove the row in place and offer an Undo — no reload needed.
            document.querySelectorAll('[data-tree-path="' + (window.CSS && CSS.escape ? CSS.escape(path) : path) + '"]').forEach(function (el) {
              el.remove();
            });
            self.trashItems = [{
              id: data.id,
              original_path: data.original_path,
              title: data.title || path.split("/").pop(),
              trashed_at: data.trashed_at || Math.floor(Date.now() / 1000)
            }].concat(self.trashItems.filter(function (item) { return item.id !== data.id; }));
            self.trashLoaded = true;
            var id = data.id;
            self.showToast("Moved “" + path + "” to Trash.", function () {
              postJSON("/api/v1/trash/restore", { id: id }).then(function () {
                window.location.reload();
              });
            });
          })
          .catch(function () {
            self.showToast("The page could not be moved to Trash.", null);
          });
      },

      /* trash view ------------------------------------------------------- */
      loadTrash: function (force) {
        if ((!force && this.trashLoaded) || this.trashLoading) return;
        this.trashLoading = true;
        var self = this;
        fetch("/api/v1/trash")
          .then(function (r) {
            return r.ok ? r.json() : [];
          })
          .then(function (items) {
            self.trashItems = items || [];
            self.trashLoaded = true;
          })
          .catch(function () {
            self.trashItems = [];
          })
          .finally(function () {
            self.trashLoading = false;
          });
      },
      restore: function (id) {
        var self = this;
        postJSON("/api/v1/trash/restore", { id: id }).then(function (r) {
          if (r.status === 409) {
            self.showToast("A page already exists at that path.", null);
            return;
          }
          window.location.reload();
        });
      },
      purge: function (id) {
        if (!window.confirm("Delete this page forever? This cannot be undone.")) return;
        var self = this;
        postJSON("/api/v1/trash/purge", { id: id }).then(function () {
          self.trashItems = self.trashItems.filter(function (it) {
            return it.id !== id;
          });
        });
      },

      /* context menu ----------------------------------------------------- */
      openMenu: function (ev, path) {
        ev.preventDefault();
        ev.stopPropagation();
        this.menu = { open: true, x: ev.clientX, y: ev.clientY, path: path };
      },
      closeMenu: function () {
        this.menu.open = false;
      },

      /* toast ------------------------------------------------------------ */
      showToast: function (message, undo) {
        var self = this;
        if (this._toastTimer) clearTimeout(this._toastTimer);
        this.toast = { show: true, message: message, undo: undo };
        this._toastTimer = setTimeout(
          function () {
            self.toast.show = false;
          },
          undo ? 10000 : 6000
        );
      },
      runUndo: function () {
        var undo = this.toast.undo;
        this.toast.show = false;
        if (undo) undo();
      }
    });
  });

  syncActive();
  injectCopyButtons(document);
})();
