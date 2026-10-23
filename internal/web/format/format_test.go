package format

import (
	"testing"
	"time"
)

func TestPrice(t *testing.T) {
	for cents, want := range map[int64]string{
		0: "R$ 0,00", 5: "R$ 0,05", 990: "R$ 9,90", 8990: "R$ 89,90", 100000: "R$ 1.000,00",
		123456: "R$ 1.234,56", 123456789: "R$ 1.234.567,89", 99999999900: "R$ 999.999.999,00",
		-150: "R$ -1,50", -50: "R$ 0,50",
	} {
		if got := Price(cents); got != want {
			t.Errorf("Price(%d) = %q, want %q", cents, got, want)
		}
	}
}

func TestDateTimeBR(t *testing.T) {
	at := time.Date(2026, 1, 2, 2, 5, 0, 0, time.UTC)
	if got := DateTimeBR(at); got != "01/01/2026 23:05" {
		t.Fatal(got)
	}
	if got := DateTimeBR(time.Date(2026, 10, 22, 15, 30, 0, 0, time.UTC)); got != "22/10/2026 12:30" {
		t.Fatal(got)
	}
}

func TestDates(t *testing.T) {
	d := time.Date(2026, 3, 9, 0, 0, 0, 0, time.UTC)
	if DateBR(d) != "09/03/2026" || DateISO(d) != "2026-03-09" {
		t.Fatal(DateBR(d), DateISO(d))
	}
	if got := TodayBR(time.Date(2026, 3, 10, 2, 0, 0, 0, time.UTC)); DateISO(got) != "2026-03-09" {
		t.Fatal(got)
	}
}
