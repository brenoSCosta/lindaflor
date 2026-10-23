// Package db embeds the goose migrations (db/migrations/*.sql) so the
// server and tests apply exactly the files sqlc generated code against.
package db

import "embed"

// Migrations holds db/migrations/*.sql (goose format).
//
//go:embed migrations/*.sql
var Migrations embed.FS

// MigrationsDir is the directory inside Migrations.
const MigrationsDir = "migrations"
