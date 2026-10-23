// Command create-admin creates (or promotes) an admin account.
//
// TODO(wave1-A): port src/bin/create-admin.rs (args, ADMIN_PASSWORD_FILE,
// random password printed once, argon2id hash).
package main

import (
	"fmt"
	"os"
)

func main() {
	fmt.Fprintln(os.Stderr, "create-admin: not implemented yet")
	os.Exit(2)
}
