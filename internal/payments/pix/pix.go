// Package pix encodes static PIX BR Code payloads (EMV fields, CRC16/
// CCITT-FALSE, merchant-name sanitizing, PIX key normalization). Port of
// src/app/store/pix.rs.
package pix

import (
	"errors"
	"fmt"
	"strings"
	"unicode/utf8"
)

// UnavailableMessage is shown when the store has no PIX key configured.
const UnavailableMessage = "Pagamento via PIX indisponível no momento. Fale com a loja pelo WhatsApp."

// Key validation errors (pt-BR, shown to the admin).
var (
	ErrCPF     = errors.New("Chave PIX inválida: CPF deve ter 11 dígitos.")                           //nolint:staticcheck // user-facing pt-BR copy
	ErrCNPJ    = errors.New("Chave PIX inválida: CNPJ deve ter 14 dígitos.")                          //nolint:staticcheck // user-facing pt-BR copy
	ErrPhone   = errors.New("Chave PIX inválida: telefone deve ter DDD + número (10 ou 11 dígitos).") //nolint:staticcheck // user-facing pt-BR copy
	ErrEmail   = errors.New("Chave PIX inválida: informe um e-mail válido.")                          //nolint:staticcheck // user-facing pt-BR copy
	ErrRandom  = errors.New("Chave PIX inválida: chave aleatória deve ser um UUID válido.")           //nolint:staticcheck // user-facing pt-BR copy
	ErrKeyType = errors.New("Tipo de chave PIX inválido.")                                            //nolint:staticcheck // user-facing pt-BR copy
)

func emvField(id, value string) string {
	return fmt.Sprintf("%s%02d%s", id, utf8.RuneCountInString(value), value)
}

// CRC16 is CRC16/CCITT-FALSE as 4 uppercase hex digits.
func CRC16(payload string) string {
	crc := uint16(0xffff)
	for i := 0; i < len(payload); i++ {
		crc ^= uint16(payload[i]) << 8
		for range 8 {
			if crc&0x8000 != 0 {
				crc = crc<<1 ^ 0x1021
			} else {
				crc <<= 1
			}
		}
	}
	return fmt.Sprintf("%04X", crc)
}

// transliterate maps an accented Latin letter to its ASCII base.
func transliterate(r rune) rune {
	switch r {
	case 'á', 'à', 'â', 'ã', 'ä', 'å', 'Á', 'À', 'Â', 'Ã', 'Ä', 'Å':
		return 'A'
	case 'é', 'è', 'ê', 'ë', 'É', 'È', 'Ê', 'Ë':
		return 'E'
	case 'í', 'ì', 'î', 'ï', 'Í', 'Ì', 'Î', 'Ï':
		return 'I'
	case 'ó', 'ò', 'ô', 'õ', 'ö', 'Ó', 'Ò', 'Ô', 'Õ', 'Ö':
		return 'O'
	case 'ú', 'ù', 'û', 'ü', 'Ú', 'Ù', 'Û', 'Ü':
		return 'U'
	case 'ç', 'Ç':
		return 'C'
	case 'ñ', 'Ñ':
		return 'N'
	case 'ý', 'ÿ', 'Ý':
		return 'Y'
	}
	return r
}

// SanitizeMerchantField keeps what Bacen tags 59/60 accept (A-Z 0-9 and
// spaces): transliterates accents, uppercases, drops the rest, truncates
// to maxLen and trims.
func SanitizeMerchantField(value string, maxLen int) string {
	var b strings.Builder
	n := 0
	for _, r := range value {
		if n >= maxLen {
			break
		}
		r = transliterate(r)
		if 'a' <= r && r <= 'z' {
			r -= 'a' - 'A'
		}
		if ('A' <= r && r <= 'Z') || ('0' <= r && r <= '9') || r == ' ' {
			b.WriteRune(r)
			n++
		}
	}
	return strings.TrimSpace(b.String())
}

func digits(raw string) string {
	var b strings.Builder
	for _, r := range raw {
		if '0' <= r && r <= '9' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func isUUIDFormat(v string) bool {
	if len(v) != 36 {
		return false
	}
	for i := 0; i < len(v); i++ {
		c := v[i]
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !strings.ContainsRune("0123456789abcdefABCDEF", rune(c)) {
			return false
		}
	}
	return true
}

// NormalizeKey validates a raw PIX key for its type and returns the form
// used in the BR Code: digits for CPF/CNPJ, E.164 (+55…) for phones,
// lowercased/trimmed for email and random (EVP) keys.
func NormalizeKey(keyType, raw string) (string, error) {
	switch strings.TrimSpace(keyType) {
	case "cpf":
		d := digits(raw)
		if len(d) != 11 {
			return "", ErrCPF
		}
		return d, nil
	case "cnpj":
		d := digits(raw)
		if len(d) != 14 {
			return "", ErrCNPJ
		}
		return d, nil
	case "phone":
		d := digits(raw)
		switch {
		case len(d) == 10 || len(d) == 11:
			d = "55" + d
		case (len(d) == 12 || len(d) == 13) && strings.HasPrefix(d, "55"):
		default:
			return "", ErrPhone
		}
		return "+" + d, nil
	case "email":
		key := strings.ToLower(strings.TrimSpace(raw))
		parts := strings.Split(key, "@")
		if len(parts) != 2 || parts[0] == "" || parts[1] == "" || strings.ContainsAny(key, " \t") {
			return "", ErrEmail
		}
		return key, nil
	case "random":
		key := strings.ToLower(strings.TrimSpace(raw))
		if !isUUIDFormat(key) {
			return "", ErrRandom
		}
		return key, nil
	}
	return "", ErrKeyType
}

// Payload builds the BR Code copy-paste string for an order. Returns the
// validation error when the configured key is invalid.
func Payload(pixKey, keyType, merchantName, merchantCity string, amountCents int64, orderID string) (string, error) {
	key, err := NormalizeKey(keyType, pixKey)
	if err != nil {
		return "", err
	}
	txid := strings.ReplaceAll(orderID, "-", "")
	if r := []rune(txid); len(r) > 25 {
		txid = string(r[:25])
	}
	amount := fmt.Sprintf("%d.%02d", amountCents/100, amountCents%100) // amounts are positive
	merchantAccount := emvField("00", "br.gov.bcb.pix") + emvField("01", key)
	withoutCRC := emvField("00", "01") +
		emvField("26", merchantAccount) +
		emvField("52", "0000") +
		emvField("53", "986") +
		emvField("54", amount) +
		emvField("58", "BR") +
		emvField("59", SanitizeMerchantField(merchantName, 25)) +
		emvField("60", SanitizeMerchantField(merchantCity, 15)) +
		emvField("62", emvField("05", txid)) +
		"6304"
	return withoutCRC + CRC16(withoutCRC), nil
}
