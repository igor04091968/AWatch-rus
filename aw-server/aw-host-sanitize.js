(function () {
  "use strict";

  var BAD_HOST = ["HOST", "EXAMPLE"].join("-");
  var DEFAULT_HOST = "HOST-EXAMPLE";

  function decode(value) {
    try {
      return decodeURIComponent(value);
    } catch (error) {
      return value;
    }
  }

  function hostFromHash() {
    var hash = window.location.hash || "";
    var match = hash.match(/#?\/activity\/([^/]+)/i) || hash.match(/#?\/trends\/([^/?#]+)/i);
    var host = match && match[1] ? decode(match[1]) : "";
    if (host && host !== BAD_HOST && host !== "unknown" && host !== "undefined") return host;
    return DEFAULT_HOST;
  }

  function rewriteText(value) {
    if (typeof value !== "string" || value.indexOf(BAD_HOST) === -1) return value;
    return value.split(BAD_HOST).join(hostFromHash());
  }

  function sanitizeStorage(storage) {
    if (!storage) return;
    try {
      for (var i = 0; i < storage.length; i += 1) {
        var key = storage.key(i);
        if (!key) continue;
        var value = storage.getItem(key);
        var next = rewriteText(value);
        if (next !== value) storage.setItem(key, next);
      }
      if (storage.landingpage && storage.landingpage.indexOf(BAD_HOST) !== -1) {
        storage.landingpage = "/activity/" + hostFromHash() + "/view/";
      }
    } catch (error) {
    }
  }

  function sanitizeRoute() {
    var hash = window.location.hash || "";
    var next = rewriteText(hash);
    if (next !== hash) window.location.replace(next);
  }

  sanitizeStorage(window.localStorage);
  sanitizeStorage(window.sessionStorage);
  sanitizeRoute();

  var originalFetch = window.fetch;
  if (typeof originalFetch === "function" && !originalFetch.__awHostSanitizePatched) {
    var patchedFetch = function (input, init) {
      var nextInput = input;
      var nextInit = init;
      try {
        if (typeof nextInput === "string") {
          nextInput = rewriteText(nextInput);
        } else if (nextInput && typeof nextInput.url === "string") {
          var nextUrl = rewriteText(nextInput.url);
          if (nextUrl !== nextInput.url && typeof Request === "function") {
            nextInput = new Request(nextUrl, nextInput);
          }
        }
        if (nextInit && typeof nextInit.body === "string") {
          nextInit = Object.assign({}, nextInit, { body: rewriteText(nextInit.body) });
        }
      } catch (error) {
      }
      return originalFetch.call(this, nextInput, nextInit);
    };
    patchedFetch.__awHostSanitizePatched = true;
    window.fetch = patchedFetch;
  }

  if (window.XMLHttpRequest && window.XMLHttpRequest.prototype && !window.XMLHttpRequest.prototype.__awHostSanitizePatched) {
    var proto = window.XMLHttpRequest.prototype;
    var originalOpen = proto.open;
    var originalSend = proto.send;
    proto.open = function (method, url) {
      if (typeof url === "string") arguments[1] = rewriteText(url);
      return originalOpen.apply(this, arguments);
    };
    proto.send = function (body) {
      if (typeof body === "string") body = rewriteText(body);
      return originalSend.call(this, body);
    };
    proto.__awHostSanitizePatched = true;
  }
})();
