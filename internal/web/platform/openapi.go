package platform

import (
	_ "embed"
	"net/http"
)

// openAPISpec is the OpenAPI 3.1 document of the HTTP API (/api/*).
//
//go:embed openapi.json
var openAPISpec []byte

// OpenAPISpec returns the embedded document (tests).
func OpenAPISpec() []byte { return openAPISpec }

// OpenAPIJSON serves GET /api/openapi.json (dev only, see Mount).
func OpenAPIJSON(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write(openAPISpec)
}

func html(w http.ResponseWriter, body string) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write([]byte(body))
}

// ScalarDocs serves GET /api/docs (Scalar from the CDN allowed by the dev CSP).
func ScalarDocs(w http.ResponseWriter, _ *http.Request) { html(w, scalarHTML) }

// SwaggerDocs serves GET /api/swagger.
func SwaggerDocs(w http.ResponseWriter, _ *http.Request) { html(w, swaggerHTML) }

const scalarHTML = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Linda Flor API — Scalar</title>
  </head>
  <body>
    <div id="app"></div>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
    <script>
      Scalar.createApiReference("#app", {
        url: "/api/openapi.json",
      });
    </script>
  </body>
</html>
`

const swaggerHTML = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Linda Flor API — Swagger</title>
    <link
      rel="stylesheet"
      href="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui.css"
    />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui-bundle.js"></script>
    <script>
      window.onload = () => {
        window.ui = SwaggerUIBundle({
          url: "/api/openapi.json",
          dom_id: "#swagger-ui",
          deepLinking: true,
          presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.presets.standalone],
          plugins: [SwaggerUIBundle.plugins.DownloadUrl],
        });
      };
    </script>
  </body>
</html>
`
