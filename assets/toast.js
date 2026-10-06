(function () {
  var script = document.currentScript;
  var root = (script && script.closest("[data-toaster]")) || document.querySelector("[data-toaster]");
  if (!root) return;
  var list = root.querySelector("[data-toast-list]");
  var gap = parseInt(root.getAttribute("data-gap") || "14", 10);
  var visibleToasts = parseInt(root.getAttribute("data-visible-toasts") || "3", 10);
  var defaultDuration = parseInt(root.getAttribute("data-duration") || "4000", 10);
  var closeButton = root.getAttribute("data-close-button") === "true";
  var y = root.getAttribute("data-y") || "bottom";
  var expandedPref = root.getAttribute("data-expand") === "true";
  var hovering = false;
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (!visibleToasts || visibleToasts < 1) visibleToasts = 1;

  function items() {
    return Array.prototype.filter.call(list.children, function (el) {
      return el.hasAttribute("data-toast-item") && el.getAttribute("data-removed") !== "true";
    });
  }

  function layout() {
    var expanded = expandedPref || hovering;
    var all = items();
    var front = all[0] && all[0].querySelector("[data-toast]");
    var frontH = front ? front.offsetHeight : 0;
    root.style.setProperty("--front-height", frontH + "px");
    var shown = Math.min(all.length, visibleToasts);
    for (var i = 0; i < all.length; i++) {
      var item = all[i];
      var hidden = i >= visibleToasts;
      item.setAttribute("data-hidden", hidden ? "true" : "false");
      item.setAttribute("data-front", i === 0 ? "true" : "false");
      item.setAttribute("data-expanded", expanded ? "true" : "false");
      item.style.setProperty("--z", String(all.length - i));
      if (hidden) {
        item.style.setProperty("--offset", "0px");
        item.style.setProperty("--scale", "1");
        continue;
      }
      if (!expanded) {
        var peek = (y === "top" ? 1 : -1) * i * gap;
        var scale = i === 0 ? 1 : Math.max(0.85, 1 - i * 0.05);
        item.style.setProperty("--offset", peek + "px");
        item.style.setProperty("--scale", String(scale));
      } else {
        var spread = 0;
        for (var j = 0; j < i; j++) {
          var prev = all[j].querySelector("[data-toast]");
          spread += (prev ? prev.offsetHeight : 0) + gap;
        }
        var dir = y === "top" ? 1 : -1;
        item.style.setProperty("--offset", (dir * spread) + "px");
        item.style.setProperty("--scale", "1");
      }
    }
    var height = 0;
    if (shown > 0) {
      if (!expanded) height = frontH;
      else {
        for (var k = 0; k < shown; k++) {
          var card = all[k].querySelector("[data-toast]");
          var h = card ? card.offsetHeight : 0;
          height += k === shown - 1 ? h : h + gap;
        }
      }
    }
    list.style.height = height + "px";
  }

  function durationOf(card) {
    var raw = card.getAttribute("data-duration");
    if (raw === null || raw === "") return defaultDuration;
    var n = parseInt(raw, 10);
    return isNaN(n) ? defaultDuration : n;
  }

  function startTimer(card, fresh) {
    if (card._timer) {
      window.clearTimeout(card._timer);
      card._timer = null;
    }
    if (fresh || typeof card._remaining !== "number") card._remaining = durationOf(card);
    if (!card._remaining) return;
    if (hovering || document.hidden) return;
    if (card.getAttribute("data-leaving") === "true") return;
    card._started = Date.now();
    card._timer = window.setTimeout(function () { leave(card); }, card._remaining);
  }

  function pause(card) {
    if (!card._timer) return;
    card._remaining -= Date.now() - card._started;
    if (card._remaining < 0) card._remaining = 0;
    window.clearTimeout(card._timer);
    card._timer = null;
  }

  function pauseAll() { items().forEach(function (item) { var c = item.querySelector("[data-toast]"); if (c) pause(c); }); }
  function resumeAll() { items().forEach(function (item) { var c = item.querySelector("[data-toast]"); if (c) startTimer(c, false); }); }

  function removeItem(item) {
    if (!item || item.getAttribute("data-removed") === "true") return;
    item.setAttribute("data-removed", "true");
    if (item.parentNode) item.parentNode.removeChild(item);
    layout();
  }

  function leave(card) {
    if (!card || card.getAttribute("data-leaving") === "true") return;
    card.setAttribute("data-leaving", "true");
    if (card._timer) { window.clearTimeout(card._timer); card._timer = null; }
    var item = card.closest("[data-toast-item]");
    if (reduced) { removeItem(item); return; }
    card.addEventListener("animationend", function (e) {
      if (e.target === card && e.animationName === "toast-out") removeItem(item);
    });
    window.setTimeout(function () { removeItem(item); }, 450);
  }

  function bindCard(card) {
    if (card.getAttribute("data-bound") === "true") return;
    card.setAttribute("data-bound", "true");
    if (reduced || card.getAttribute("data-mounted") === "true") {
      card.setAttribute("data-mounted", "true");
    } else card.addEventListener("animationend", function onIn(e) {
      if (e.target !== card || e.animationName !== "toast-in") return;
      card.setAttribute("data-mounted", "true");
      card.removeEventListener("animationend", onIn);
    });
    var btn = card.querySelector("[data-toast-close]");
    if (btn) btn.addEventListener("click", function (e) {
      e.preventDefault();
      leave(card);
    });
    var startY = 0;
    var active = false;
    var pointerId = null;
    card.addEventListener("pointerdown", function (e) {
      if (card.getAttribute("data-dismissible") === "false") return;
      if (card.getAttribute("data-leaving") === "true") return;
      if (e.button !== 0) return;
      if (e.target.closest && e.target.closest("[data-toast-close], a")) return;
      active = true;
      pointerId = e.pointerId;
      startY = e.clientY;
      try { card.setPointerCapture(e.pointerId); } catch {}
    });
    card.addEventListener("pointermove", function (e) {
      if (!active || e.pointerId !== pointerId) return;
      var dy = e.clientY - startY;
      if (y !== "top" && dy < 0) dy = 0;
      if (y === "top" && dy > 0) dy = 0;
      card.setAttribute("data-swipe", "true");
      card.style.setProperty("--swipe", dy + "px");
    });
    function endSwipe(e) {
      if (!active || (e && pointerId !== e.pointerId)) return;
      active = false;
      var dy = parseFloat(card.style.getPropertyValue("--swipe")) || 0;
      card.removeAttribute("data-swipe");
      card.style.removeProperty("--swipe");
      var passed = y === "top" ? dy < -48 : dy > 48;
      if (passed) leave(card);
    }
    card.addEventListener("pointerup", endSwipe);
    card.addEventListener("pointercancel", endSwipe);
  }

  function iconFor(type) {
    var bank = root.querySelector('[data-toast-icon-bank] [data-icon="' + type + '"]');
    return bank ? bank.cloneNode(true) : null;
  }

  function fillLink(node, link) {
    if (!node) return;
    if (link && link.label && link.href) {
      node.hidden = false;
      node.textContent = link.label;
      node.setAttribute("href", link.href);
    } else {
      node.hidden = true;
      node.textContent = "";
      node.removeAttribute("href");
    }
  }

  function apply(card, data) {
    var type = data.type || "default";
    card.setAttribute("data-type", type);
    card.setAttribute("data-dismissible", data.dismissible === false ? "false" : "true");
    if (typeof data.duration === "number") card.setAttribute("data-duration", String(data.duration));
    else card.setAttribute("data-duration", "");
    var title = card.querySelector("[data-toast-title]");
    if (title) title.textContent = data.title || "";
    var desc = card.querySelector("[data-toast-description]");
    if (desc) {
      if (data.description) {
        desc.hidden = false;
        desc.textContent = data.description;
      } else {
        desc.hidden = true;
        desc.textContent = "";
      }
    }
    var iconSlot = card.querySelector("[data-toast-icon]");
    if (iconSlot) {
      iconSlot.textContent = "";
      var icon = iconFor(type);
      if (icon) iconSlot.appendChild(icon);
    }
    fillLink(card.querySelector("[data-toast-action]"), data.action);
    fillLink(card.querySelector("[data-toast-cancel]"), data.cancel);
    var btn = card.querySelector("[data-toast-close]");
    if (btn) btn.hidden = !(closeButton && data.dismissible !== false);
    card.removeAttribute("data-leaving");
    startTimer(card, true);
    layout();
  }

  function mount(data) {
    var tpl = root.querySelector("[data-toast-template]");
    var item = tpl.content.firstElementChild.cloneNode(true);
    var card = item.querySelector("[data-toast]");
    list.insertBefore(item, list.firstChild);
    bindCard(card);
    apply(card, data);
    return card;
  }

  function parseToast(raw) {
    if (!raw) return null;
    try { return JSON.parse(raw); } catch {}
    try { return JSON.parse(decodeURIComponent(raw)); } catch {}
    return null;
  }

  function samePath(loc) {
    try {
      var url = new URL(loc, window.location.href);
      return url.origin === window.location.origin && url.pathname === window.location.pathname;
    } catch {
      return false;
    }
  }

  Array.prototype.forEach.call(list.querySelectorAll("[data-toast]"), function (card) {
    bindCard(card);
    startTimer(card, true);
  });
  layout();

  root.addEventListener("mouseover", function () {
    if (hovering) return;
    hovering = true;
    pauseAll();
    layout();
  });
  root.addEventListener("mouseout", function (e) {
    if (e.relatedTarget && root.contains(e.relatedTarget)) return;
    if (!hovering) return;
    hovering = false;
    resumeAll();
    layout();
  });
  document.addEventListener("visibilitychange", function () {
    if (document.hidden) pauseAll();
    else resumeAll();
  });
  window.addEventListener("resize", layout);

  document.addEventListener("submit", function (e) {
    if (e.defaultPrevented) return;
    var form = e.target;
    if (!form || !form.getAttribute || form.getAttribute("data-toast-promise") === null) return;
    if (form.querySelector('[aria-invalid="true"]')) return;
    e.preventDefault();
    if (form.getAttribute("data-toast-pending") === "true") return;
    form.setAttribute("data-toast-pending", "true");
    var loading = form.getAttribute("data-toast-loading") || "…";
    var card = mount({ type: "loading", title: loading, duration: 0, dismissible: false });
    var body = new URLSearchParams();
    new FormData(form).forEach(function (value, key) {
      if (typeof value === "string") body.append(key, value);
    });
    var submit = form.querySelector("[type=submit]");
    if (submit) submit.disabled = true;
    fetch(form.action, {
      method: (form.method || "post").toUpperCase(),
      body: body,
      redirect: "manual",
      credentials: "same-origin",
      headers: { "X-Toast-Promise": "1" }
    }).then(function (res) {
      var data = parseToast(res.headers.get("X-Toast"));
      var loc = res.headers.get("Location");
      if (data) {
        apply(card, data);
        if (loc && !samePath(loc)) {
          window.setTimeout(function () { window.location.assign(loc); }, 700);
        }
        return;
      }
      if (loc) {
        leave(card);
        window.location.assign(loc);
        return;
      }
      apply(card, { type: "error", title: "Não foi possível enviar.", duration: 4000, dismissible: true });
    }).catch(function () {
      apply(card, { type: "error", title: "Não foi possível enviar.", duration: 4000, dismissible: true });
    }).then(function () {
      form.removeAttribute("data-toast-pending");
      if (submit) submit.disabled = false;
    });
  });

  document.addEventListener("click", function (e) {
    var opt = e.target.closest && e.target.closest("[data-toast-option]");
    if (!opt) return;
    var key = opt.getAttribute("data-toast-option");
    var value = opt.getAttribute("data-toast-value") || "";
    if (key === "position") {
      var parts = value.split("-");
      y = parts[0] === "top" ? "top" : "bottom";
      root.setAttribute("data-y", y);
      root.setAttribute("data-x", parts[1] || "right");
    } else if (key === "expand") {
      expandedPref = value === "true";
      root.setAttribute("data-expand", expandedPref ? "true" : "false");
    } else if (key === "rich") {
      root.setAttribute("data-rich-colors", value === "true" ? "true" : "false");
    } else if (key === "close") {
      closeButton = value === "true";
      root.setAttribute("data-close-button", closeButton ? "true" : "false");
    }
    layout();
  });
})();
