// Package format renders values the pt-BR way (prices, dates in Brasília
// time). Ports format_price / format_datetime_br from
// src/app/store/queries.rs and the report date helpers.
package format

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

// BRT is Brasília time (UTC-3; Brazil has had no DST since 2019). Matches
// SQL `AT TIME ZONE 'America/Sao_Paulo'`.
var BRT = time.FixedZone("BRT", -3*60*60)

// Price renders cents as "R$ 1.234,56" (exact port of the Rust helper,
// including its handling of negative amounts).
func Price(cents int64) string {
	reais := cents / 100
	centavos := cents % 100
	if centavos < 0 {
		centavos = -centavos
	}
	digits := strconv.FormatInt(reais, 10)
	var b strings.Builder
	for i := 0; i < len(digits); i++ {
		if i > 0 && (len(digits)-i)%3 == 0 {
			b.WriteByte('.')
		}
		b.WriteByte(digits[i])
	}
	return fmt.Sprintf("R$ %s,%02d", b.String(), centavos)
}

// DateTimeBR renders "DD/MM/YYYY HH:MM" in Brasília time.
func DateTimeBR(t time.Time) string { return t.In(BRT).Format("02/01/2006 15:04") }

// DateBR renders "DD/MM/YYYY" (the date as given, no zone shift).
func DateBR(t time.Time) string { return t.Format("02/01/2006") }

// DateISO renders "YYYY-MM-DD" (the date as given, no zone shift).
func DateISO(t time.Time) string { return t.Format("2006-01-02") }

// TodayBR is the current calendar day in Brasília (midnight, BRT zone).
func TodayBR(now time.Time) time.Time {
	l := now.In(BRT)
	return time.Date(l.Year(), l.Month(), l.Day(), 0, 0, 0, 0, BRT)
}
