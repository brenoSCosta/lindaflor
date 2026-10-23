# Lindaflor (Go). Every target expects the nix dev shell (`nix develop` or
# `devenv shell`), which provides go, templ, sqlc, goose, golangci-lint,
# tailwindcss and postgres.
SHELL := bash
.DEFAULT_GOAL := build

BIN        ?= bin
GOOSE_DIR  := db/migrations
CSS_IN     := web/styles/input.css
CSS_OUT    := web/static/css/app.css

.PHONY: generate templ sqlc css build test test-race lint fmt fmt-check vet dev run migrate migrate-status seed create-admin db-reset tidy drift clean

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
	sqlc vet
	templ fmt -fail .

## fmt: gofumpt + goimports (golangci-lint fmt), templ fmt
fmt:
	golangci-lint fmt ./...
	templ fmt .

vet:
	go vet ./...

## drift: fail when generated code is stale (CI)
drift: generate
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

seed:
	go run ./cmd/seed

create-admin:
	go run ./cmd/create-admin $(ARGS)

tidy:
	go mod tidy

clean:
	rm -rf $(BIN) tmp
