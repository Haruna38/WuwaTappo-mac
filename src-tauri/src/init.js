// Injected into every window before page scripts run.
//
// 1. document.cookie shim: WKWebView does not persist cookies for the custom
//    `tauri://localhost` origin, but the app keeps its session token
//    (kuro_infoToken / kuro_lastLoginTime / kuro_lang) in cookies. Back the
//    cookie jar with localStorage, which is shared by all windows of the origin.
// (The listener_ready hook lives in dist via scripts/patch-dist.py because
// Tauri defines __TAURI_INTERNALS__.invoke as non-writable.)
(function () {
  if (window.__WUWA_MAC_INIT__) return;
  window.__WUWA_MAC_INIT__ = true;

  // ---------- cookie shim ----------
  var STORE_KEY = "__wuwa_cookie_jar__";
  function loadJar() {
    try {
      var raw = localStorage.getItem(STORE_KEY);
      return raw ? JSON.parse(raw) : {};
    } catch (e) {
      return {};
    }
  }
  function saveJar(jar) {
    try {
      localStorage.setItem(STORE_KEY, JSON.stringify(jar));
    } catch (e) {}
  }
  function purge(jar) {
    var now = Date.now();
    var changed = false;
    for (var k in jar) {
      if (jar[k].expires != null && jar[k].expires <= now) {
        delete jar[k];
        changed = true;
      }
    }
    if (changed) saveJar(jar);
    return jar;
  }
  function getCookie() {
    var jar = purge(loadJar());
    var out = [];
    for (var k in jar) out.push(k + "=" + jar[k].value);
    return out.join("; ");
  }
  function setCookie(str) {
    if (typeof str !== "string") return;
    var parts = str.split(";");
    var first = parts.shift().trim();
    var eq = first.indexOf("=");
    if (eq <= 0) return;
    var name = first.slice(0, eq).trim();
    var value = first.slice(eq + 1).trim();
    var expires = null;
    for (var i = 0; i < parts.length; i++) {
      var p = parts[i].trim();
      var pe = p.indexOf("=");
      var an = (pe >= 0 ? p.slice(0, pe) : p).trim().toLowerCase();
      var av = pe >= 0 ? p.slice(pe + 1).trim() : "";
      if (an === "expires") {
        var t = Date.parse(av);
        if (!isNaN(t)) expires = t;
      } else if (an === "max-age") {
        var n = Number(av);
        if (!isNaN(n)) expires = Date.now() + n * 1000;
      }
    }
    var jar = loadJar();
    if (expires != null && expires <= Date.now()) {
      delete jar[name];
    } else {
      jar[name] = { value: value, expires: expires };
    }
    saveJar(jar);
  }
  try {
    Object.defineProperty(Document.prototype, "cookie", {
      configurable: true,
      enumerable: true,
      get: getCookie,
      set: setCookie,
    });
  } catch (e) {
    try {
      Object.defineProperty(document, "cookie", {
        configurable: true,
        enumerable: true,
        get: getCookie,
        set: setCookie,
      });
    } catch (e2) {}
  }

  // ---------- console/error forwarding to the Rust log ----------
  function fwd(level, args) {
    try {
      var msg = Array.prototype.map
        .call(args, function (a) {
          if (a instanceof Error) return a.stack || String(a);
          if (typeof a === "object") {
            try { return JSON.stringify(a); } catch (e) { return String(a); }
          }
          return String(a);
        })
        .join(" ");
      var t = window.__TAURI_INTERNALS__;
      if (t && t.invoke) t.invoke("write_app_log", { level: level, message: "[console] " + msg.slice(0, 2000) }).catch(function () {});
    } catch (e) {}
  }
  var origError = console.error.bind(console);
  var origWarn = console.warn.bind(console);
  console.error = function () { fwd("error", arguments); origError.apply(null, arguments); };
  console.warn = function () { fwd("warn", arguments); origWarn.apply(null, arguments); };
  window.__wuwa_errs = [];
  window.addEventListener("error", function (e) { window.__wuwa_errs.push(e.message + " @" + e.filename + ":" + e.lineno); fwd("error", [e.message, e.filename + ":" + e.lineno]); });
  window.addEventListener("unhandledrejection", function (e) { window.__wuwa_errs.push("rejection " + String(e.reason)); fwd("error", ["unhandledrejection", e.reason]); });
})();
