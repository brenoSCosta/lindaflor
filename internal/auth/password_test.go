package auth

import (
	"errors"
	"strings"
	"testing"
	"unicode/utf8"
)

func mustErr(t *testing.T, err error, want string) {
	t.Helper()
	if err == nil {
		t.Fatalf("expected error containing %q", want)
	}
	if !strings.Contains(err.Error(), want) {
		t.Fatalf("unexpected error: %v (want %q)", err, want)
	}
}

// hash_and_verify_round_trip
func TestHashAndVerifyRoundTrip(t *testing.T) {
	hash, err := HashPassword("secret-password")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(hash, "$argon2id$v=19$m=19456,t=2,p=1$") {
		t.Fatalf("unexpected PHC: %s", hash)
	}
	if ok, err := VerifyPassword("secret-password", hash); err != nil || !ok {
		t.Fatalf("verify ok=%v err=%v", ok, err)
	}
	if ok, err := VerifyPassword("wrong", hash); err != nil || ok {
		t.Fatalf("wrong password verified ok=%v err=%v", ok, err)
	}
	if _, err := VerifyPassword("x", "not-a-phc"); err == nil {
		t.Fatal("malformed hash must be an error")
	}
}

// rejects_short_password_before_strength_check
func TestRejectsShortPasswordBeforeStrengthCheck(t *testing.T) {
	mustErr(t, ValidatePassword("Ab1!"), "at least")
}

// rejects_password_over_max_chars
func TestRejectsPasswordOverMaxChars(t *testing.T) {
	long := strings.Repeat("a", MaxPasswordLen+1)
	mustErr(t, ValidatePassword(long), "at most")
}

// max_counts_chars_not_bytes_for_multibyte
func TestMaxCountsCharsNotBytesForMultibyte(t *testing.T) {
	long := strings.Repeat("é", 129)
	if utf8.RuneCountInString(long) != 129 || len(long) <= MaxPasswordLen {
		t.Fatal("bad fixture")
	}
	mustErr(t, ValidatePassword(long), "at most")
	fits := strings.Repeat("é", 100)
	mustErr(t, ValidatePassword(fits), "too weak")
}

// rejects_weak_password
func TestRejectsWeakPassword(t *testing.T) {
	err := ValidatePassword("password123", "user@example.com")
	mustErr(t, err, "too weak")
	if !errors.Is(err, ErrPasswordTooWeak) {
		t.Fatalf("want ErrPasswordTooWeak, got %v", err)
	}
}

// min_is_checked_before_max_and_strength
func TestMinIsCheckedBeforeMaxAndStrength(t *testing.T) {
	mustErr(t, ValidatePassword("pw"), "at least")
}

// accepts_strong_password_with_user_inputs
func TestAcceptsStrongPasswordWithUserInputs(t *testing.T) {
	for _, p := range []string{"Harbor-Violet-Tiger-4821-Lantern!", "Quilt-Mango-Brave-Pixel-7395?"} {
		if err := ValidatePassword(p, "user@example.com", "Test User", "lindaflor"); err != nil {
			t.Fatalf("strong password rejected: %s: %v", p, err)
		}
	}
}

func TestDummyVerifyDoesNotPanic(t *testing.T) {
	DummyVerify("anything")
	DummyVerify("")
}

func TestPortugueseMessages(t *testing.T) {
	cases := map[error]string{
		PasswordTooShort(8):  "A senha deve ter pelo menos 8 caracteres.",
		PasswordTooLong(128): "A senha deve ter no máximo 128 caracteres.",
		ErrPasswordTooWeak:   "Escolha uma senha mais forte.",
		ErrInvalidToken:      "Link inválido ou expirado.",
		ErrUnauthorized:      "Credenciais inválidas.",
		ErrForbidden:         "Credenciais inválidas.",
		ErrTooManyRequests:   "Muitas tentativas. Aguarde um momento e tente novamente.",
		ErrNameRequired:      "Informe o nome.",
		ErrEmailInvalid:      "Informe um e-mail válido.",
		BadRequest("Google sign-in is not configured"): "Login com Google não está configurado.",
		errors.New("db exploded unauthorized"):         "Não foi possível concluir a operação. Tente novamente.",
	}
	for err, want := range cases {
		if got := PortugueseMessage(err); got != want {
			t.Errorf("%v → %q, want %q", err, got, want)
		}
	}
}
