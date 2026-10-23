package store

import (
	"fmt"
	"strings"
	"unicode/utf8"
)

// BR Code (PIX) payload encoding: EMV fields, CRC16/CCITT-FALSE,
// merchant-name sanitizing and PIX key normalization (src/app/store/pix.rs).

// PixUnavailableMessage is returned by checkout when no PIX key is set.
const PixUnavailableMessage = "Pagamento via PIX indisponível no momento. Fale com a loja pelo WhatsApp."

func emvField(id, value string) string {
	return fmt.Sprintf("%s%02d%s", id, utf8.RuneCountInString(value), value)
}

func crc16CCITTFalse(payload string) string {
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

// sanitizeMerchantField keeps A-Z 0-9 and spaces (Bacen tags 59/60):
// transliterate, uppercase, drop the rest, truncate, trim.
func sanitizeMerchantField(value string, maxLength int) string {
	var b strings.Builder
	n := 0
	for _, r := range value {
		r = transliterate(r)
		if r >= 'a' && r <= 'z' {
			r -= 'a' - 'A'
		}
		if !(r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == ' ') {
			continue
		}
		if n == maxLength {
			break
		}
		b.WriteRune(r)
		n++
	}
	return strings.TrimSpace(b.String())
}

func isUUIDFormat(v string) bool {
	if len(v) != 36 {
		return false
	}
	for i := 0; i < 36; i++ {
		c := v[i]
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !isHex(c) {
			return false
		}
	}
	return true
}

func isHex(c byte) bool {
	return c >= '0' && c <= '9' || c >= 'a' && c <= 'f' || c >= 'A' && c <= 'F'
}

func digitsOnly(raw string) string {
	var b strings.Builder
	for _, r := range raw {
		if r >= '0' && r <= '9' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// NormalizePixKey validates a raw PIX key for its type and returns the
// BR Code form: digits for CPF/CNPJ, E.164 (+55…) for phones, lowercased
// for email and random (EVP) keys.
func NormalizePixKey(keyType, raw string) (string, error) {
	switch strings.TrimSpace(keyType) {
	case "cpf":
		d := digitsOnly(raw)
		if len(d) != 11 {
			return "", userErr("Chave PIX inválida: CPF deve ter 11 dígitos.")
		}
		return d, nil
	case "cnpj":
		d := digitsOnly(raw)
		if len(d) != 14 {
			return "", userErr("Chave PIX inválida: CNPJ deve ter 14 dígitos.")
		}
		return d, nil
	case "phone":
		d := digitsOnly(raw)
		var national string
		switch {
		case len(d) == 10 || len(d) == 11:
			national = "55" + d
		case (len(d) == 12 || len(d) == 13) && strings.HasPrefix(d, "55"):
			national = d
		default:
			return "", userErr("Chave PIX inválida: telefone deve ter DDD + número (10 ou 11 dígitos).")
		}
		return "+" + national, nil
	case "email":
		key := strings.ToLower(strings.TrimSpace(raw))
		parts := strings.Split(key, "@")
		valid := len(parts) == 2 && parts[0] != "" && parts[1] != "" &&
			!strings.Contains(key, " ") && !strings.Contains(key, "\t")
		if !valid {
			return "", userErr("Chave PIX inválida: informe um e-mail válido.")
		}
		return key, nil
	case "random":
		key := strings.ToLower(strings.TrimSpace(raw))
		if !isUUIDFormat(key) {
			return "", userErr("Chave PIX inválida: chave aleatória deve ser um UUID válido.")
		}
		return key, nil
	}
	return "", userErr("Tipo de chave PIX inválido.")
}

// GeneratePixPayload builds the BR Code copy-paste string for an order.
func GeneratePixPayload(pixKey, pixKeyType, merchantName, merchantCity string, amountCents int64, orderID string) (string, error) {
	key, err := NormalizePixKey(pixKeyType, pixKey)
	if err != nil {
		return "", err
	}
	name := sanitizeMerchantField(merchantName, 25)
	city := sanitizeMerchantField(merchantCity, 15)
	txid := strings.ReplaceAll(orderID, "-", "")
	if utf8.RuneCountInString(txid) > 25 {
		txid = string([]rune(txid)[:25])
	}
	amount := fmt.Sprintf("%.2f", float64(amountCents)/100)
	account := emvField("00", "br.gov.bcb.pix") + emvField("01", key)
	payload := emvField("00", "01") +
		emvField("26", account) +
		emvField("52", "0000") +
		emvField("53", "986") +
		emvField("54", amount) +
		emvField("58", "BR") +
		emvField("59", name) +
		emvField("60", city) +
		emvField("62", emvField("05", txid)) +
		"6304"
	return payload + crc16CCITTFalse(payload), nil
}
