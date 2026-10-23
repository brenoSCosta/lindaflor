// Package qrsvg renders QR codes as `data:image/svg+xml` URIs so pages can
// show them with a plain <img> (CSP img-src allows data:). Used by the PIX
// payment card (/pedido/{id}) and the one-time 2FA setup page
// (/settings/two-factor).
package qrsvg

import (
	"fmt"
	"net/url"
	"strings"

	"github.com/boombuler/barcode/qr"
)

// quietZone is the white border around the code, in modules.
const quietZone = 4

// SVG encodes payload as an SVG document (black modules on white, one
// path), or "" when the payload cannot be encoded.
func SVG(payload string, minSize int) string {
	code, err := qr.Encode(payload, qr.M, qr.Auto)
	if err != nil {
		return ""
	}
	b := code.Bounds()
	w, h := b.Dx(), b.Dy()
	size := w + 2*quietZone
	var path strings.Builder
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			r, _, _, _ := code.At(b.Min.X+x, b.Min.Y+y).RGBA()
			if r == 0 {
				fmt.Fprintf(&path, "M%d %dh1v1h-1z", x+quietZone, y+quietZone)
			}
		}
	}
	return fmt.Sprintf(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" width="%d" height="%d" shape-rendering="crispEdges"><rect width="100%%" height="100%%" fill="#fff"/><path fill="#000" d="%s"/></svg>`,
		size, size, minSize, minSize, path.String())
}

// DataURI is SVG as a `data:image/svg+xml,` URI ("" on failure).
func DataURI(payload string, minSize int) string {
	svg := SVG(payload, minSize)
	if svg == "" {
		return ""
	}
	return "data:image/svg+xml," + url.PathEscape(svg)
}
