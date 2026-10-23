// Package static embeds web/static (compiled CSS, JS, vendored libraries)
// and serves it under /static/ with content-hash cache busting.
//
//	<link rel="stylesheet" href={ static.URL("css/app.css") }>
//
// renders /static/css/app.css?v=<hash>; requests carrying the current hash
// are cached for a year (immutable), anything else must revalidate.
package static

import (
	"crypto/sha256"
	"embed"
	"encoding/hex"
	"io/fs"
	"net/http"
	"strings"
	"sync"
)

//go:embed css js vendor
var files embed.FS

// Prefix is the URL path the files are served under.
const Prefix = "/static/"

// FS exposes the embedded tree (paths like "css/app.css").
func FS() fs.FS { return files }

var (
	hashMu sync.RWMutex
	hashes = map[string]string{}
)

// Hash is the first 12 hex chars of the file's SHA-256 ("" if missing).
func Hash(name string) string {
	name = strings.TrimPrefix(name, "/")
	hashMu.RLock()
	h, ok := hashes[name]
	hashMu.RUnlock()
	if ok {
		return h
	}
	b, err := files.ReadFile(name)
	if err != nil {
		return ""
	}
	sum := sha256.Sum256(b)
	h = hex.EncodeToString(sum[:])[:12]
	hashMu.Lock()
	hashes[name] = h
	hashMu.Unlock()
	return h
}

// URL is the cache-busted URL of an embedded file, e.g.
// URL("css/app.css") → "/static/css/app.css?v=3f2a…".
func URL(name string) string {
	name = strings.TrimPrefix(name, "/")
	if h := Hash(name); h != "" {
		return Prefix + name + "?v=" + h
	}
	return Prefix + name
}

// Handler serves the embedded files (mount at Prefix). Directory listings
// are disabled.
func Handler() http.Handler {
	fileServer := http.StripPrefix(strings.TrimSuffix(Prefix, "/"), http.FileServerFS(files))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		name := strings.TrimPrefix(r.URL.Path, Prefix)
		if name == "" || strings.HasSuffix(name, "/") {
			http.NotFound(w, r)
			return
		}
		if v := r.URL.Query().Get("v"); v != "" && v == Hash(name) {
			w.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
		} else {
			w.Header().Set("Cache-Control", "no-cache")
		}
		fileServer.ServeHTTP(w, r)
	})
}
