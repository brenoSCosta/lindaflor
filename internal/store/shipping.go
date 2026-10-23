package store

import (
	"fmt"
	"strconv"
	"strings"
)

// Shipping rates derived from the destination CEP. The UF is always taken
// from the CEP (Correios ranges), never from the posted state field: the
// rate depends on it, so trusting the client would let anyone pick the
// cheapest tier.

// BrazilianStates are the 27 UFs.
var BrazilianStates = []string{
	"AC", "AL", "AP", "AM", "BA", "CE", "DF", "ES", "GO", "MA", "MT", "MS", "MG",
	"PA", "PB", "PR", "PE", "PI", "RJ", "RN", "RS", "RO", "RR", "SC", "SP", "SE",
	"TO",
}

type cepRange struct {
	start, end int
	uf         string
}

// cepRanges are Correios CEP ranges by UF as 5-digit prefixes (inclusive).
var cepRanges = []cepRange{
	{1000, 19999, "SP"},
	{20000, 28999, "RJ"},
	{29000, 29999, "ES"},
	{30000, 39999, "MG"},
	{40000, 48999, "BA"},
	{49000, 49999, "SE"},
	{50000, 56999, "PE"},
	{57000, 57999, "AL"},
	{58000, 58999, "PB"},
	{59000, 59999, "RN"},
	{60000, 63999, "CE"},
	{64000, 64999, "PI"},
	{65000, 65999, "MA"},
	{66000, 68899, "PA"},
	{68900, 68999, "AP"},
	{69000, 69299, "AM"},
	{69300, 69399, "RR"},
	{69400, 69899, "AM"},
	{69900, 69999, "AC"},
	{70000, 72799, "DF"},
	{72800, 72999, "GO"},
	{73000, 73699, "DF"},
	{73700, 76799, "GO"},
	{76800, 76999, "RO"},
	{77000, 77999, "TO"},
	{78000, 78899, "MT"},
	// Legacy Rondônia range, kept so old addresses still resolve.
	{78900, 78999, "RO"},
	{79000, 79999, "MS"},
	{80000, 87999, "PR"},
	{88000, 89999, "SC"},
	{90000, 99999, "RS"},
}

// Shipping prices in cents.
const (
	ShippingLocalCents    int64 = 1990
	ShippingStandardCents int64 = 3990
	ShippingRemoteCents   int64 = 5990
)

// ShippingTier is the rate bucket of a UF.
type ShippingTier int

// Tiers.
const (
	TierLocal    ShippingTier = iota // Sergipe
	TierStandard                     // Nordeste (outside SE), Sudeste, Sul, Centro-Oeste
	TierRemote                       // Região Norte
)

// TierForUF maps a UF to its tier.
func TierForUF(uf string) ShippingTier {
	switch uf {
	case "SE":
		return TierLocal
	case "AC", "AM", "AP", "PA", "RO", "RR", "TO":
		return TierRemote
	}
	return TierStandard
}

// Cents is the tier price.
func (t ShippingTier) Cents() int64 {
	switch t {
	case TierLocal:
		return ShippingLocalCents
	case TierRemote:
		return ShippingRemoteCents
	}
	return ShippingStandardCents
}

// IsBrazilianState reports a valid UF.
func IsBrazilianState(uf string) bool { return contains(BrazilianStates, uf) }

// CEPDigits keeps only ASCII digits.
func CEPDigits(raw string) string {
	var b strings.Builder
	for _, r := range raw {
		if r >= '0' && r <= '9' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// UFFromCEP maps an 8-digit CEP (digits only) to its UF.
func UFFromCEP(digits string) (string, bool) {
	if len(digits) != 8 {
		return "", false
	}
	for i := 0; i < 8; i++ {
		if digits[i] < '0' || digits[i] > '9' {
			return "", false
		}
	}
	prefix, err := strconv.Atoi(digits[:5])
	if err != nil {
		return "", false
	}
	for _, r := range cepRanges {
		if prefix >= r.start && prefix <= r.end {
			return r.uf, true
		}
	}
	return "", false
}

// ResolveShippingUF validates the posted CEP (any formatting) and state
// and returns the UF derived from the CEP. A posted state that disagrees
// with the CEP is rejected so the customer can fix a typo.
func ResolveShippingUF(rawCEP, postedState string) (string, error) {
	digits := CEPDigits(rawCEP)
	if digits == "" {
		return "", userErr("Informe o CEP.")
	}
	if len(digits) != 8 {
		return "", userErr("Informe um CEP válido com 8 dígitos.")
	}
	uf, ok := UFFromCEP(digits)
	if !ok {
		return "", userErr("CEP não encontrado. Confira o número informado.")
	}
	posted := strings.TrimSpace(postedState)
	if !IsBrazilianState(posted) {
		return "", userErr("Selecione um estado válido.")
	}
	if posted != uf {
		return "", userErr(fmt.Sprintf("O CEP informado pertence a %s, mas o estado selecionado é %s. Confira o estado ou o CEP.", uf, posted))
	}
	return uf, nil
}

// CalculateShipping returns (cents, free) for subtotal and destination UF.
func CalculateShipping(subtotalCents int64, uf string, freeShippingThresholdCents int64) (int64, bool) {
	if subtotalCents >= freeShippingThresholdCents {
		return 0, true
	}
	return TierForUF(uf).Cents(), false
}
