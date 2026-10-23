package layout

import (
	"strconv"

	"github.com/wellington-vell/lindaflor/internal/web/format"
)

func itoa(n int) string { return strconv.Itoa(n) }

func priceStr(c int64) string { return format.Price(c) }
