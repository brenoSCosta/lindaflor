// Command create-admin creates or promotes a Lindaflor admin (production
// bootstrap). Port of src/bin/create-admin.rs:
//
//	create-admin --email EMAIL [--name NAME] [--no-password]
//
// The password comes from the file named by ADMIN_PASSWORD_FILE (Docker
// secret style) or, failing that, the first line of stdin — never from
// argv, so it stays out of shell history and `ps`. It must satisfy the
// sign-up password policy. --no-password promotes an existing user without
// touching their credentials. Needs DATABASE_URL (or DATABASE_URL_FILE);
// refuses to run against a database with pending migrations.
package main

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db"
)

const usage = "usage: create-admin --email EMAIL [--name NAME] [--no-password]\n" +
	"  password: ADMIN_PASSWORD_FILE=<path> or one line on stdin"

type args struct {
	email      string
	name       string
	noPassword bool
}

func parseArgs(argv []string) (args, error) {
	a := args{name: "Admin"}
	email := ""
	for i := 0; i < len(argv); i++ {
		arg := argv[i]
		switch {
		case arg == "--email":
			if i+1 < len(argv) {
				i++
				email = argv[i]
			}
		case arg == "--name":
			if i+1 < len(argv) {
				i++
				a.name = argv[i]
			}
		case arg == "--no-password":
			a.noPassword = true
		case arg == "-h" || arg == "--help":
			return a, errors.New(usage)
		case strings.HasPrefix(arg, "--email="):
			email = strings.TrimPrefix(arg, "--email=")
		case strings.HasPrefix(arg, "--name="):
			a.name = strings.TrimPrefix(arg, "--name=")
		default:
			return a, fmt.Errorf("unknown argument `%s`\n%s", arg, usage)
		}
	}
	if email == "" {
		return a, fmt.Errorf("--email is required\n%s", usage)
	}
	a.email = email
	return a, nil
}

func readPassword(stdin io.Reader, stderr io.Writer) (string, error) {
	if path := os.Getenv("ADMIN_PASSWORD_FILE"); path != "" {
		b, err := os.ReadFile(path) //nolint:gosec // operator-supplied secret path
		if err != nil {
			return "", fmt.Errorf("cannot read ADMIN_PASSWORD_FILE: %w", err)
		}
		return strings.TrimRight(string(b), "\r\n"), nil
	}
	if f, ok := stdin.(*os.File); ok {
		if st, err := f.Stat(); err == nil && st.Mode()&os.ModeCharDevice != 0 {
			_, _ = fmt.Fprint(stderr, "Password (input is visible; prefer ADMIN_PASSWORD_FILE): ")
		}
	}
	line, err := bufio.NewReader(stdin).ReadString('\n')
	if err != nil && !errors.Is(err, io.EOF) {
		return "", fmt.Errorf("cannot read password from stdin: %w", err)
	}
	return strings.TrimRight(line, "\r\n"), nil
}

func run(ctx context.Context, argv []string, stdin io.Reader, stdout, stderr io.Writer) error {
	a, err := parseArgs(argv)
	if err != nil {
		return err
	}
	var password *string
	if !a.noPassword {
		p, err := readPassword(stdin, stderr)
		if err != nil {
			return err
		}
		if p == "" {
			return errors.New("empty password (use --no-password to promote without one)")
		}
		password = &p
	}
	dbURL, ok := config.ReadEnvOrFile("DATABASE_URL")
	if !ok {
		return &config.MissingVarError{Key: "DATABASE_URL"}
	}
	pool, err := db.Open(ctx, dbURL, db.PoolOptions{MaxConns: 2})
	if err != nil {
		return err
	}
	defer pool.Close()
	out, err := auth.CreateOrPromoteAdmin(ctx, pool, a.email, a.name, password)
	if err != nil {
		return err
	}
	if out.Created {
		_, _ = fmt.Fprintf(stdout, "created admin %s (%s)\n", out.UserID, strings.TrimSpace(a.email))
	} else {
		_, _ = fmt.Fprintf(stdout, "promoted %s (%s) to admin\n", out.UserID, strings.TrimSpace(a.email))
	}
	return nil
}

func main() {
	if err := run(context.Background(), os.Args[1:], os.Stdin, os.Stdout, os.Stderr); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "create-admin: %s\n", err)
		os.Exit(1)
	}
}
