# Lindaflor (Go). Every target expects the nix dev shell (`nix develop` or
# `devenv shell`), which provides go, templ, sqlc, goose, golangci-lint,
# tailwindcss and postgres.
SHELL := bash
.DEFAULT_GOAL := build

BIN        ?= bin
GOOSE_DIR  := db/migrations
CSS_IN     := web/styles/input.css
CSS_OUT    := web/static/css/app.css
IMAGE      ?= lindaflor
IMAGE_TAG  ?= dev

.PHONY: generate templ sqlc css build test test-race lint lint-infra vuln fmt fmt-check vet dev run migrate migrate-status backup seed create-admin db-reset tidy drift docker docker-runtime docker-migrate docker-smoke e2e clean

## generate: templ + sqlc + tailwind (outputs are committed)
generate: templ sqlc css

templ:
	templ generate

sqlc:
	sqlc generate

css:
	tailwindcss -i $(CSS_IN) -o $(CSS_OUT) --minify

## build: compile the binaries into ./bin
build:
	CGO_ENABLED=0 go build -trimpath -o $(BIN)/ ./cmd/...

## test: unit + integration tests (embedded/nix Postgres + miniredis)
test:
	go test ./...

test-race:
	go test -race ./...

## lint: golangci-lint (v2) + sqlc vet + templ fmt check
lint:
	golangci-lint run ./...
	sqlc compile
	sqlc vet
	templ fmt -fail .

## lint-infra: Dockerfile (hadolint), shell scripts (shellcheck), workflows (actionlint)
lint-infra:
	hadolint --failure-threshold warning --ignore DL3008 Dockerfile
	shellcheck scripts/*.sh
	actionlint

## vuln: known vulnerabilities in reachable code (govulncheck)
vuln:
	govulncheck ./...

## fmt: gofumpt + goimports (golangci-lint fmt), templ fmt
fmt:
	golangci-lint fmt ./...
	templ fmt .

vet:
	go vet ./...

## drift: fail when generated code is stale (CI)
drift: generate
	sqlc diff
	git diff --exit-code -- '*_templ.go' internal/db/sqlc $(CSS_OUT)

## dev: live reload (air) — rebuilds templ/css/go on change
dev:
	air

run:
	go run ./cmd/server

## migrate: apply goose migrations to $$DATABASE_URL
migrate:
	goose -dir $(GOOSE_DIR) postgres "$$DATABASE_URL" up

migrate-status:
	goose -dir $(GOOSE_DIR) postgres "$$DATABASE_URL" status

## backup: pg_dump $$DATABASE_URL into ./backups (scripts/backup.sh)
backup:
	scripts/backup.sh manual

seed:
	go run ./cmd/seed

create-admin:
	go run ./cmd/create-admin $(ARGS)

tidy:
	go mod tidy

## docker: build the runtime and migrate images ($(IMAGE):$(IMAGE_TAG), $(IMAGE)-migrate:$(IMAGE_TAG))
docker: docker-runtime docker-migrate

docker-runtime:
	docker build --target runtime -t $(IMAGE):$(IMAGE_TAG) .

docker-migrate:
	docker build --target migrate -t $(IMAGE)-migrate:$(IMAGE_TAG) .

## docker-smoke: boot both images against throwaway Postgres + Valkey
docker-smoke: docker
	scripts/docker-smoke.sh $(IMAGE):$(IMAGE_TAG) $(IMAGE)-migrate:$(IMAGE_TAG)

## e2e: Playwright suite against a throwaway stack (postgres, valkey, versitygw S3,
## mailpit, fake Mercado Pago, caddy TLS) — not part of go test. FLOWS="admin jobs" runs a subset.
E2E_NIX_PKGS := nixpkgs\#mailpit nixpkgs\#versitygw nixpkgs\#caddy nixpkgs\#nodejs nixpkgs\#playwright-test
e2e:
	nix shell $(E2E_NIX_PKGS) --command e2e/stack.sh $(FLOWS)

clean:
	rm -rf $(BIN) tmp
