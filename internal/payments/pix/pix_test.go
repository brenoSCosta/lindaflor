package pix

import "testing"

func TestCRC16MatchesWellKnownVector(t *testing.T) {
	if got := CRC16("123456789"); got != "29B1" {
		t.Fatal(got)
	}
}

func payloadFor(t *testing.T, keyType, key string) string {
	t.Helper()
	p, err := Payload(key, keyType, "Linda Flor", "Aracaju", 10_000, "ordem123")
	if err != nil {
		t.Fatal(err)
	}
	return p
}

func TestGoldenVectors(t *testing.T) {
	for _, c := range []struct{ keyType, key, want string }{
		{"phone", "79999816511", "00020126360014br.gov.bcb.pix0114+55799998165115204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363048CDC"},
		{"cpf", "12345678901", "00020126330014br.gov.bcb.pix0111123456789015204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem1236304F9F4"},
		{"cnpj", "12345678000195", "00020126360014br.gov.bcb.pix0114123456780001955204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363045855"},
		{"email", "Loja@Exemplo.com", "00020126380014br.gov.bcb.pix0116loja@exemplo.com5204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363047DA5"},
		{"random", "A1B2C3D4-E5F6-7890-ABCD-EF1234567890", "00020126580014br.gov.bcb.pix0136a1b2c3d4-e5f6-7890-abcd-ef12345678905204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363048700"},
	} {
		if got := payloadFor(t, c.keyType, c.key); got != c.want {
			t.Errorf("%s:\n got %s\nwant %s", c.keyType, got, c.want)
		}
	}
}

func TestPhoneNormalizationCases(t *testing.T) {
	for raw, want := range map[string]string{
		"(79) 99981-6511": "+5579999816511",
		"79999816511":     "+5579999816511",
		"+5579999816511":  "+5579999816511",
		"5579999816511":   "+5579999816511",
		"7930211234":      "+557930211234",
	} {
		if got, err := NormalizeKey("phone", raw); err != nil || got != want {
			t.Errorf("%q → %q %v", raw, got, err)
		}
	}
	for _, bad := range []string{"123", "123456789012345", ""} {
		if _, err := NormalizeKey("phone", bad); err == nil {
			t.Errorf("%q accepted", bad)
		}
	}
}

func TestKeyValidationRejectsBadInput(t *testing.T) {
	bad := [][2]string{{"cpf", "123"}, {"cnpj", "123"}, {"email", "sem-arroba"}, {"random", "nao-um-uuid"}, {"other", "abc"}}
	for _, c := range bad {
		if _, err := NormalizeKey(c[0], c[1]); err == nil {
			t.Errorf("%v accepted", c)
		}
	}
	if _, err := NormalizeKey("cpf", "123.456.789-01"); err != nil {
		t.Fatal(err)
	}
	if got, err := NormalizeKey("email", "  LOJA@EXEMPLO.COM "); err != nil || got != "loja@exemplo.com" {
		t.Fatal(got, err)
	}
}

func TestMerchantFieldsTransliterateAccents(t *testing.T) {
	for _, c := range []struct {
		in   string
		max  int
		want string
	}{
		{"São João do Café", 25, "SAO JOAO DO CAFE"},
		{"Aracajú", 15, "ARACAJU"},
		{"Lindä Flôr Ü", 25, "LINDA FLOR U"},
		{"Linda Flor Moda Praia e Acessórios", 25, "LINDA FLOR MODA PRAIA E A"},
		{"Santa Luzia do Itanhy!", 15, "SANTA LUZIA DO"},
		{"Loja-123 & Cia.", 25, "LOJA123  CIA"},
	} {
		if got := SanitizeMerchantField(c.in, c.max); got != c.want {
			t.Errorf("%q → %q, want %q", c.in, got, c.want)
		}
	}
}

func TestPayloadRejectsInvalidKey(t *testing.T) {
	if _, err := Payload("", "phone", "Linda Flor", "Aracaju", 10_000, "ordem123"); err == nil {
		t.Fatal("empty key accepted")
	}
}
