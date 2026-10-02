(function () {
  "use strict";

  var MAX_BYTES = 2 * 1024 * 1024;
  var MSG_TYPE = "Apenas imagens JPG, PNG ou WebP são permitidas";
  var MSG_SIZE = "A imagem deve ter no máximo 2MB";
  var MSG_CROP = "Não foi possível recortar a imagem";
  var ALLOWED = { "image/jpeg": true, "image/png": true, "image/webp": true };

  function init() {
    var dialog = document.getElementById("avatar-cropper-dialog");
    var openBtn = document.getElementById("avatar-cropper-open");
    var fileInput = document.getElementById("avatar-cropper-file");
    var drop = document.getElementById("avatar-cropper-drop");
    var stage = document.getElementById("avatar-cropper-stage");
    var img = document.getElementById("avatar-cropper-image");
    var controls = document.getElementById("avatar-cropper-controls");
    var zoomInput = document.getElementById("avatar-cropper-zoom");
    var zoomOut = document.getElementById("avatar-cropper-zoom-out");
    var zoomIn = document.getElementById("avatar-cropper-zoom-in");
    var resetBtn = document.getElementById("avatar-cropper-reset");
    var chooseBtn = document.getElementById("avatar-cropper-choose");
    var applyBtn = document.getElementById("avatar-cropper-apply");
    var errorEl = document.getElementById("avatar-cropper-error");

    if (
      !dialog ||
      !openBtn ||
      !fileInput ||
      !drop ||
      !stage ||
      !img ||
      !controls ||
      !zoomInput ||
      !zoomOut ||
      !zoomIn ||
      !resetBtn ||
      !chooseBtn ||
      !applyBtn ||
      !errorEl ||
      typeof dialog.showModal !== "function"
    ) {
      return;
    }

    var objectUrl = null;
    var loaded = false;
    var zoom = 1;
    var x = 0;
    var y = 0;
    var generation = 0;
    var dragging = false;
    var pointerId = null;
    var lastX = 0;
    var lastY = 0;

    function showError(message) {
      errorEl.textContent = message;
      errorEl.classList.remove("hidden");
    }

    function clearError() {
      errorEl.textContent = "";
      errorEl.classList.add("hidden");
    }

    function setVisible(el, visible, displayClass) {
      el.classList.toggle("hidden", !visible);
      if (displayClass) el.classList.toggle(displayClass, visible);
    }

    function viewSize() {
      return stage.clientWidth || 256;
    }

    function coverScale(view) {
      return Math.max(view / img.naturalWidth, view / img.naturalHeight);
    }

    function clampPosition() {
      var view = viewSize();
      var scale = coverScale(view) * zoom;
      var minX = view - img.naturalWidth * scale;
      var minY = view - img.naturalHeight * scale;
      if (x > 0) x = 0;
      if (y > 0) y = 0;
      if (x < minX) x = minX;
      if (y < minY) y = minY;
    }

    function render() {
      var view = viewSize();
      var scale = coverScale(view) * zoom;
      img.style.maxWidth = "none";
      img.style.maxHeight = "none";
      img.style.width = img.naturalWidth * scale + "px";
      img.style.height = img.naturalHeight * scale + "px";
      img.style.transform = "translate(" + x + "px," + y + "px)";
    }

    function setZoom(next, originX, originY) {
      var view = viewSize();
      var clamped = Math.min(5, Math.max(1, Math.round(next * 10) / 10));
      var ox = originX == null ? view / 2 : originX;
      var oy = originY == null ? view / 2 : originY;
      var prev = coverScale(view) * zoom;
      var ix = (ox - x) / prev;
      var iy = (oy - y) / prev;
      zoom = clamped;
      var nextScale = coverScale(view) * zoom;
      x = ox - ix * nextScale;
      y = oy - iy * nextScale;
      clampPosition();
      zoomInput.value = String(zoom);
      render();
    }

    function resetCrop() {
      zoom = 1;
      var view = viewSize();
      var scale = coverScale(view);
      x = (view - img.naturalWidth * scale) / 2;
      y = (view - img.naturalHeight * scale) / 2;
      zoomInput.value = "1";
      render();
    }

    function clearSelection() {
      generation += 1;
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl);
        objectUrl = null;
      }
      loaded = false;
      zoom = 1;
      x = 0;
      y = 0;
      dragging = false;
      fileInput.value = "";
      img.removeAttribute("src");
      zoomInput.value = "1";
      applyBtn.disabled = true;
      stage.classList.remove("cursor-grabbing");
      setVisible(drop, true, "flex");
      setVisible(stage, false);
      setVisible(controls, false, "flex");
      clearError();
    }

    function acceptFile(file) {
      clearError();
      if (!ALLOWED[file.type]) {
        showError(MSG_TYPE);
        fileInput.value = "";
        return;
      }
      if (file.size > MAX_BYTES) {
        showError(MSG_SIZE);
        fileInput.value = "";
        return;
      }

      var token = ++generation;
      var nextUrl = URL.createObjectURL(file);
      var probe = new Image();
      applyBtn.disabled = true;
      probe.onload = function () {
        if (token !== generation) {
          URL.revokeObjectURL(nextUrl);
          return;
        }
        if (!probe.naturalWidth || !probe.naturalHeight) {
          URL.revokeObjectURL(nextUrl);
          showError(MSG_CROP);
          applyBtn.disabled = !loaded;
          return;
        }
        if (objectUrl) URL.revokeObjectURL(objectUrl);
        objectUrl = nextUrl;
        img.onload = function () {
          if (token !== generation) return;
          loaded = true;
          setVisible(drop, false, "flex");
          setVisible(stage, true);
          setVisible(controls, true, "flex");
          resetCrop();
          applyBtn.disabled = false;
        };
        img.onerror = function () {
          if (token !== generation) return;
          showError(MSG_CROP);
          applyBtn.disabled = !loaded;
        };
        img.src = nextUrl;
      };
      probe.onerror = function () {
        if (token !== generation) {
          URL.revokeObjectURL(nextUrl);
          return;
        }
        URL.revokeObjectURL(nextUrl);
        showError(MSG_CROP);
        applyBtn.disabled = !loaded;
      };
      probe.src = nextUrl;
    }

    function pickFile() {
      fileInput.value = "";
      fileInput.click();
    }

    function toJpegBlob(canvas) {
      return new Promise(function (resolve, reject) {
        canvas.toBlob(
          function (blob) {
            if (!blob) reject(new Error("blob"));
            else resolve(blob);
          },
          "image/jpeg",
          0.92,
        );
      });
    }

    function drawCanvas(width, height, paint) {
      var canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      var ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("ctx");
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = "high";
      paint(ctx, canvas);
      return canvas;
    }

    function drawVisible(size) {
      var view = viewSize();
      var scale = coverScale(view) * zoom;
      return drawCanvas(size, size, function (ctx) {
        var ratio = size / view;
        ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
        ctx.drawImage(img, x, y, img.naturalWidth * scale, img.naturalHeight * scale);
      });
    }

    function scaleCanvas(source, size) {
      return drawCanvas(size, size, function (ctx) {
        ctx.drawImage(source, 0, 0, size, size);
      });
    }

    function exportJpeg() {
      var view = viewSize();
      var sourceSide = view / (coverScale(view) * zoom);
      var size = Math.max(1, Math.min(2048, Math.round(sourceSide)));
      var canvas = drawVisible(size);

      function shrink(blob) {
        if (blob.size <= MAX_BYTES) return blob;
        if (canvas.width <= 32) throw new Error("size");
        canvas = scaleCanvas(canvas, Math.max(32, Math.floor(canvas.width * 0.85)));
        return toJpegBlob(canvas).then(shrink);
      }

      return toJpegBlob(canvas).then(shrink);
    }

    function isJpeg(blob) {
      return blob.slice(0, 3).arrayBuffer().then(function (buffer) {
        var bytes = new Uint8Array(buffer);
        return bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
      });
    }

    function submitAvatar(blob) {
      return isJpeg(blob).then(function (ok) {
        if (!ok) throw new Error("magic");
        var file = new File([blob], "avatar.jpg", { type: "image/jpeg" });
        var form = document.createElement("form");
        form.method = "POST";
        form.action = "/settings/avatar";
        form.enctype = "multipart/form-data";
        var input = document.createElement("input");
        input.type = "file";
        input.name = "file";
        var transfer = new DataTransfer();
        transfer.items.add(file);
        input.files = transfer.files;
        form.appendChild(input);
        form.hidden = true;
        document.body.appendChild(form);
        form.submit();
      });
    }

    openBtn.addEventListener("click", function () {
      if (!dialog.open) dialog.showModal();
    });

    dialog.addEventListener("click", function (event) {
      if (event.target === dialog) dialog.close();
    });

    dialog.addEventListener("close", function () {
      clearSelection();
    });

    dialog.addEventListener("dragover", function (event) {
      event.preventDefault();
    });

    dialog.addEventListener("drop", function (event) {
      event.preventDefault();
      var file = event.dataTransfer && event.dataTransfer.files && event.dataTransfer.files[0];
      if (file) acceptFile(file);
    });

    drop.addEventListener("click", pickFile);
    chooseBtn.addEventListener("click", pickFile);

    fileInput.addEventListener("change", function () {
      var file = fileInput.files && fileInput.files[0];
      if (file) acceptFile(file);
    });

    zoomOut.addEventListener("click", function () {
      if (!loaded) return;
      var view = viewSize();
      setZoom(zoom - 0.1, view / 2, view / 2);
    });

    zoomIn.addEventListener("click", function () {
      if (!loaded) return;
      var view = viewSize();
      setZoom(zoom + 0.1, view / 2, view / 2);
    });

    resetBtn.addEventListener("click", function () {
      if (!loaded) return;
      resetCrop();
    });

    zoomInput.addEventListener("input", function () {
      if (!loaded) return;
      var view = viewSize();
      setZoom(parseFloat(zoomInput.value), view / 2, view / 2);
    });

    stage.addEventListener(
      "wheel",
      function (event) {
        if (!loaded || event.deltaY === 0) return;
        event.preventDefault();
        var rect = stage.getBoundingClientRect();
        var direction = event.deltaY > 0 ? -0.1 : 0.1;
        setZoom(zoom + direction, event.clientX - rect.left, event.clientY - rect.top);
      },
      { passive: false },
    );

    stage.addEventListener("pointerdown", function (event) {
      if (!loaded || event.button !== 0) return;
      dragging = true;
      pointerId = event.pointerId;
      lastX = event.clientX;
      lastY = event.clientY;
      stage.setPointerCapture(event.pointerId);
      stage.classList.add("cursor-grabbing");
    });

    stage.addEventListener("pointermove", function (event) {
      if (!dragging || event.pointerId !== pointerId) return;
      x += event.clientX - lastX;
      y += event.clientY - lastY;
      lastX = event.clientX;
      lastY = event.clientY;
      clampPosition();
      render();
    });

    function endDrag(event) {
      if (!dragging || event.pointerId !== pointerId) return;
      dragging = false;
      pointerId = null;
      stage.classList.remove("cursor-grabbing");
    }

    stage.addEventListener("pointerup", endDrag);
    stage.addEventListener("pointercancel", endDrag);

    window.addEventListener("resize", function () {
      if (!loaded || !dialog.open) return;
      clampPosition();
      render();
    });

    applyBtn.addEventListener("click", function () {
      if (!loaded) return;
      applyBtn.disabled = true;
      clearError();
      var pending;
      try {
        pending = exportJpeg();
      } catch {
        showError(MSG_CROP);
        applyBtn.disabled = false;
        return;
      }
      pending
        .then(function (blob) {
          if (!dialog.open) return;
          return submitAvatar(blob);
        })
        .catch(function () {
          if (!dialog.open) return;
          showError(MSG_CROP);
          applyBtn.disabled = false;
        });
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
