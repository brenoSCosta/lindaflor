package render_test

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/a-h/templ"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

func TestStatusCachingByEnv(t *testing.T) {
	for _, tc := range []struct {
		env  string
		want string
	}{
		{"development", "no-store"},
		{"dev", "no-store"},
		{"test", "no-store"},
		{"production", ""},
		{"staging", ""},
	} {
		t.Run(tc.env, func(t *testing.T) {
			d := &app.Deps{Cfg: &config.Config{AppEnv: tc.env}}
			rec := httptest.NewRecorder()
			req := httptest.NewRequest(http.MethodGet, "/", nil)
			render.Page(rec, req, d, templ.ComponentFunc(func(context.Context, io.Writer) error { return nil }))
			if got := rec.Header().Get("Cache-Control"); got != tc.want {
				t.Fatalf("Cache-Control = %q, want %q", got, tc.want)
			}
			wantReload := tc.want != ""
			if got := strings.Contains(rec.Body.String(), "js/dev-reload.js"); got != wantReload {
				t.Fatalf("dev-reload script included = %v, want %v", got, wantReload)
			}
		})
	}
}
