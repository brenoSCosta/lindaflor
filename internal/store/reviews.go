package store

import (
	"context"
	"errors"
	"fmt"
	"math"
	"strconv"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
)

// Product reviews (src/app/store/reviews.rs): verified-buyer submission,
// aggregates and moderation. Eligible: the user owns an order with a
// variant of the product that is delivered, or shipped at least 21 days
// ago. Reviews start pending; only approved ones are public.

// Review limits.
const (
	ReviewTitleMax   = 80
	ReviewBodyMax    = 2000
	ShippedGraceDays = 21
	ReviewsPageSize  = 5
	// ScopeReviewSubmit is the per-user submit budget (ratelimit window
	// scope, env RATE_LIMIT_REVIEW_SUBMIT_PER_WINDOW): 5 per hour.
	ScopeReviewSubmit          = "review:submit"
	DefaultReviewsPerWindow    = 5
	ReviewSubmitWindow         = time.Hour
	ReviewRateLimitedMessage   = "Muitas avaliações em pouco tempo. Tente mais tarde."
	ReviewSubmittedTitle       = "Obrigada pela avaliação!"
	ReviewSubmittedDescription = "Ela aparece na página depois da moderação."
)

// receivedOrderSQL is the `orders o` predicate: owned by $1 and received.
const receivedOrderSQL = `o.user_id = $1
  AND (o.status = 'delivered'
       OR (o.status = 'shipped'
           AND o.shipped_at <= now() - make_interval(days => 21)))`

// Fit is the size feedback.
type Fit string

// Fits.
const (
	FitPequeno Fit = "pequeno"
	FitIdeal   Fit = "ideal"
	FitGrande  Fit = "grande"
)

// ParseFit parses a fit value (trimmed).
func ParseFit(raw string) (Fit, bool) {
	switch f := Fit(strings.TrimSpace(raw)); f {
	case FitPequeno, FitIdeal, FitGrande:
		return f, true
	}
	return "", false
}

// Label is the pt-BR label.
func (f Fit) Label() string {
	switch f {
	case FitPequeno:
		return "Veste pequeno"
	case FitIdeal:
		return "Tamanho ideal"
	case FitGrande:
		return "Veste grande"
	}
	return ""
}

// Review errors (Message is the pt-BR toast).
var (
	ErrReviewNotEligible = errors.New("Só quem recebeu este produto pode avaliá-lo.")
	ErrAlreadyReviewed   = errors.New("Você já avaliou este produto.")
)

// NewReview is a validated submission.
type NewReview struct {
	Rating int16
	Title  string
	Body   string
	Fit    *Fit
}

// ParseNewReview trims and validates the raw form fields (*UserError).
func ParseNewReview(rating, title, body string, fit *string) (NewReview, error) {
	r, err := strconv.ParseInt(strings.TrimSpace(rating), 10, 16)
	if err != nil || r < 1 || r > 5 {
		return NewReview{}, userErr("Escolha uma nota de 1 a 5.")
	}
	t := strings.TrimSpace(title)
	if t == "" || runeLen(t) > ReviewTitleMax {
		return NewReview{}, userErr("O título deve ter entre 1 e 80 caracteres.")
	}
	b := strings.TrimSpace(body)
	if b == "" || runeLen(b) > ReviewBodyMax {
		return NewReview{}, userErr("O comentário deve ter entre 1 e 2000 caracteres.")
	}
	out := NewReview{Rating: int16(r), Title: t, Body: b}
	if fit != nil && strings.TrimSpace(*fit) != "" {
		f, ok := ParseFit(*fit)
		if !ok {
			return NewReview{}, userErr("Caimento inválido.")
		}
		out.Fit = &f
	}
	return out, nil
}

var (
	sqlEligibleOrder = sqlText(`SELECT o.id FROM orders o
 WHERE ` + receivedOrderSQL + `
   AND EXISTS (SELECT 1 FROM order_items oi
               JOIN product_variants v ON v.id = oi.variant_id
               WHERE oi.order_id = o.id AND v.product_id = $2)
 ORDER BY o.created_at DESC LIMIT 1`)
	sqlOwnReviewStatus = sqlText(`SELECT status::text FROM product_reviews WHERE user_id = $1 AND product_id = $2`)
	sqlInsertReview    = sqlText(`INSERT INTO product_reviews
   (id, product_id, user_id, order_id, rating, title, body, fit)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8::text::review_fit)
 ON CONFLICT (user_id, product_id) DO NOTHING
 RETURNING id`)
)

// EligibleOrder is the user's most recent received order containing the
// product, nil when none.
func EligibleOrder(ctx context.Context, q DBTX, userID, productID uuid.UUID) (*uuid.UUID, error) {
	var id uuid.UUID
	err := q.QueryRow(ctx, sqlEligibleOrder, userID, productID).Scan(&id)
	if found, err := optional(err); err != nil || !found {
		return nil, err
	}
	return &id, nil
}

// OwnReviewStatus is the status of the user's review of the product ("" =
// none).
func OwnReviewStatus(ctx context.Context, q DBTX, userID, productID uuid.UUID) (string, error) {
	var s string
	err := q.QueryRow(ctx, sqlOwnReviewStatus, userID, productID).Scan(&s)
	if found, err := optional(err); err != nil || !found {
		return "", err
	}
	return s, nil
}

// SubmitReview stores a verified buyer's review as pending (one per user
// and product).
func SubmitReview(ctx context.Context, q DBTX, userID, productID uuid.UUID, r NewReview) (uuid.UUID, error) {
	status, err := OwnReviewStatus(ctx, q, userID, productID)
	if err != nil {
		return uuid.Nil, err
	}
	if status != "" {
		return uuid.Nil, ErrAlreadyReviewed
	}
	orderID, err := EligibleOrder(ctx, q, userID, productID)
	if err != nil {
		return uuid.Nil, err
	}
	if orderID == nil {
		return uuid.Nil, ErrReviewNotEligible
	}
	var fit *string
	if r.Fit != nil {
		fit = ptr(string(*r.Fit))
	}
	var id uuid.UUID
	err = q.QueryRow(ctx, sqlInsertReview, uuid.Must(uuid.NewV7()), productID, userID, *orderID, r.Rating, r.Title, r.Body, fit).Scan(&id)
	if found, err := optional(err); err != nil {
		return uuid.Nil, err
	} else if !found {
		return uuid.Nil, ErrAlreadyReviewed
	}
	return id, nil
}

// ReviewErrorMessage maps a submit error to its toast ("" = internal).
func ReviewErrorMessage(err error) string {
	if msg, ok := UserMessage(err); ok {
		return msg
	}
	if errors.Is(err, ErrReviewNotEligible) || errors.Is(err, ErrAlreadyReviewed) {
		return err.Error()
	}
	return ""
}

// RatingSummary is the approved-review aggregate of a product.
type RatingSummary struct {
	Average    float64
	Count      int64
	FitPequeno int64
	FitIdeal   int64
	FitGrande  int64
}

// FitMajority is the most reported fit and its rounded share; ok=false
// without fit answers. Ties go to the last of (ideal, pequeno, grande).
func (s RatingSummary) FitMajority() (string, int64, bool) {
	total := s.FitPequeno + s.FitIdeal + s.FitGrande
	if total == 0 {
		return "", 0, false
	}
	best, n := FitIdeal, s.FitIdeal
	for _, c := range []struct {
		f Fit
		n int64
	}{{FitPequeno, s.FitPequeno}, {FitGrande, s.FitGrande}} {
		if c.n >= n {
			best, n = c.f, c.n
		}
	}
	return best.Label(), (n*100 + total/2) / total, true
}

// AverageLabel is "4,5".
func (s RatingSummary) AverageLabel() string { return FormatAverage(s.Average) }

// CountLabel is "1 avaliação" / "N avaliações".
func (s RatingSummary) CountLabel() string { return ReviewsCountLabel(s.Count) }

// FormatAverage renders an average with one decimal and a comma.
func FormatAverage(avg float64) string {
	return strings.Replace(strconv.FormatFloat(avg, 'f', 1, 64), ".", ",", 1)
}

// ReviewsCountLabel pluralizes the review count.
func ReviewsCountLabel(n int64) string {
	if n == 1 {
		return "1 avaliação"
	}
	return fmt.Sprintf("%d avaliações", n)
}

// StarsFilled is the number of filled stars (rounded, 0..5).
func StarsFilled(rating float64) int { return int(math.Min(math.Max(math.Round(rating), 0), 5)) }

// StarsAriaLabel is "Nota 4,5 de 5".
func StarsAriaLabel(rating float64) string { return "Nota " + FormatAverage(rating) + " de 5" }

var sqlRatingSummary = sqlText(`SELECT AVG(rating)::float8 AS average,
        COUNT(*) AS count,
        COUNT(*) FILTER (WHERE fit = 'pequeno') AS fit_pequeno,
        COUNT(*) FILTER (WHERE fit = 'ideal') AS fit_ideal,
        COUNT(*) FILTER (WHERE fit = 'grande') AS fit_grande
 FROM product_reviews
 WHERE product_id = $1 AND status = 'approved'`)

// LoadRatingSummary loads a product's aggregates.
func LoadRatingSummary(ctx context.Context, q DBTX, productID uuid.UUID) (RatingSummary, error) {
	var s RatingSummary
	var avg *float64
	err := q.QueryRow(ctx, sqlRatingSummary, productID).Scan(&avg, &s.Count, &s.FitPequeno, &s.FitIdeal, &s.FitGrande)
	s.Average = deref(avg)
	return s, err
}

// AggregateRating is the schema.org input; nil without approved reviews.
type AggregateRating struct {
	RatingAverage float64 // one decimal
	ReviewCount   int64
}

// LoadAggregateRating loads the JSON-LD AggregateRating of a product.
func LoadAggregateRating(ctx context.Context, q DBTX, productID uuid.UUID) (*AggregateRating, error) {
	s, err := LoadRatingSummary(ctx, q, productID)
	if err != nil || s.Count == 0 {
		return nil, err
	}
	return &AggregateRating{RatingAverage: math.Round(s.Average*10) / 10, ReviewCount: s.Count}, nil
}

// CardRating is (average, count) for product cards.
type CardRating struct {
	Average float64
	Count   int64
}

var sqlAllRatings = sqlText(`SELECT product_id, AVG(rating)::float8, COUNT(*)
 FROM product_reviews WHERE status = 'approved'
 GROUP BY product_id`)

// AllRatings maps product → approved rating (one query per page).
func AllRatings(ctx context.Context, q DBTX) (map[uuid.UUID]CardRating, error) {
	rows, err := q.Query(ctx, sqlAllRatings)
	if err != nil {
		return nil, err
	}
	out := map[uuid.UUID]CardRating{}
	var id uuid.UUID
	var r CardRating
	_, err = pgx.ForEachRow(rows, []any{&id, &r.Average, &r.Count}, func() error {
		out[id] = r
		return nil
	})
	return out, err
}

// PublicReview is an approved review.
type PublicReview struct {
	ID         uuid.UUID `db:"id"`
	Rating     int16     `db:"rating"`
	Title      string    `db:"title"`
	Body       string    `db:"body"`
	Fit        *string   `db:"fit"`
	AuthorName string    `db:"author_name"`
	CreatedAt  time.Time `db:"created_at"`
}

// Author is the public author ("Maria S.").
func (r PublicReview) Author() string { return PublicAuthor(r.AuthorName) }

// FitLabel is the fit label ("" when none).
func (r PublicReview) FitLabel() string {
	if r.Fit == nil {
		return ""
	}
	f, _ := ParseFit(*r.Fit)
	return f.Label()
}

// PublicAuthor is first name + last initial ("Maria da Silva" → "Maria S.").
func PublicAuthor(name string) string {
	parts := strings.Fields(name)
	if len(parts) == 0 {
		return "Cliente"
	}
	if len(parts) == 1 {
		return parts[0]
	}
	initial := []rune(parts[len(parts)-1])[0]
	return parts[0] + " " + strings.ToUpper(string(initial)) + "."
}

var sqlListApproved = sqlText(`SELECT r.id, r.rating, r.title, r.body, r.fit::text AS fit,
        u.name AS author_name, r.created_at
 FROM product_reviews r
 JOIN users u ON u.id = r.user_id
 WHERE r.product_id = $1 AND r.status = 'approved'
 ORDER BY r.created_at DESC
 LIMIT $2 OFFSET $3`)

// ListApprovedReviews pages approved reviews, newest first.
func ListApprovedReviews(ctx context.Context, q DBTX, productID uuid.UUID, limit, offset int64) ([]PublicReview, error) {
	return collect[PublicReview](q.Query(ctx, sqlListApproved, productID, limit, offset))
}

// ReviewSection is the product page review block.
type ReviewSection struct {
	Summary RatingSummary
	// ShowForm: signed-in verified buyer without a review.
	ShowForm bool
	// ReviewedNote is shown when the user already reviewed ("" otherwise).
	ReviewedNote string
	Reviews      []PublicReview
	Pager        Pagination
}

// LoadReviewSection loads the section for an (optional) viewer and page.
func LoadReviewSection(ctx context.Context, q DBTX, productID uuid.UUID, viewer *uuid.UUID, page int64) (ReviewSection, error) {
	var out ReviewSection
	var err error
	if out.Summary, err = LoadRatingSummary(ctx, q, productID); err != nil {
		return out, err
	}
	if viewer != nil {
		status, err := OwnReviewStatus(ctx, q, *viewer, productID)
		if err != nil {
			return out, err
		}
		switch status {
		case "":
			order, err := EligibleOrder(ctx, q, *viewer, productID)
			if err != nil {
				return out, err
			}
			out.ShowForm = order != nil
		case "approved":
			out.ReviewedNote = "Sua avaliação está publicada. Obrigada!"
		case "rejected":
			out.ReviewedNote = "Sua avaliação não foi aprovada pela moderação."
		default:
			out.ReviewedNote = "Recebemos sua avaliação. Ela aparece aqui depois da moderação."
		}
	}
	totalPages := max((max(out.Summary.Count, 0)+ReviewsPageSize-1)/ReviewsPageSize, 1)
	out.Pager = Pagination{Page: min(max(page, 1), totalPages), PageSize: ReviewsPageSize, Total: out.Summary.Count, TotalPages: totalPages}
	if out.Summary.Count > 0 {
		out.Reviews, err = ListApprovedReviews(ctx, q, productID, ReviewsPageSize, out.Pager.Offset())
	}
	return out, err
}

// ReviewableProduct is a received product not yet reviewed.
type ReviewableProduct struct {
	ProductID uuid.UUID `db:"product_id"`
	Name      string    `db:"name"`
	Slug      string    `db:"slug"`
}

var (
	sqlReviewableForOrder = sqlText(`SELECT DISTINCT p.id AS product_id, p.name, p.slug
 FROM orders o
 JOIN order_items oi ON oi.order_id = o.id
 JOIN product_variants v ON v.id = oi.variant_id
 JOIN products p ON p.id = v.product_id
 WHERE o.id = $2 AND ` + receivedOrderSQL + ` AND p.active
   AND NOT EXISTS (SELECT 1 FROM product_reviews r
                   WHERE r.user_id = $1 AND r.product_id = p.id)
 ORDER BY p.name`)
	sqlReviewableOrders = sqlText(`SELECT DISTINCT o.id
 FROM orders o
 JOIN order_items oi ON oi.order_id = o.id
 JOIN product_variants v ON v.id = oi.variant_id
 JOIN products p ON p.id = v.product_id
 WHERE ` + receivedOrderSQL + ` AND p.active
   AND NOT EXISTS (SELECT 1 FROM product_reviews r
                   WHERE r.user_id = $1 AND r.product_id = p.id)`)
)

// ReviewableProductsForOrder lists the order's products the user may still
// review ("Avalie suas compras" card).
func ReviewableProductsForOrder(ctx context.Context, q DBTX, userID, orderID uuid.UUID) ([]ReviewableProduct, error) {
	return collect[ReviewableProduct](q.Query(ctx, sqlReviewableForOrder, userID, orderID))
}

// ReviewableOrderIDs are the user's received orders with something to review.
func ReviewableOrderIDs(ctx context.Context, q DBTX, userID uuid.UUID) (map[uuid.UUID]bool, error) {
	ids, err := scalars[uuid.UUID](q.Query(ctx, sqlReviewableOrders, userID))
	if err != nil {
		return nil, err
	}
	out := make(map[uuid.UUID]bool, len(ids))
	for _, id := range ids {
		out[id] = true
	}
	return out, nil
}

// --- Moderation ---

// AdminReview is a moderation queue row.
type AdminReview struct {
	ID          uuid.UUID `db:"id"`
	ProductName string    `db:"product_name"`
	ProductSlug string    `db:"product_slug"`
	AuthorName  string    `db:"author_name"`
	AuthorEmail string    `db:"author_email"`
	Rating      int16     `db:"rating"`
	Title       string    `db:"title"`
	Body        string    `db:"body"`
	Fit         *string   `db:"fit"`
	Status      string    `db:"status"`
	CreatedAt   time.Time `db:"created_at"`
}

// StatusBadge is (tone, label) of the moderation status.
func (r AdminReview) StatusBadge() (BadgeTone, string) {
	switch r.Status {
	case "approved":
		return TonePrimary, "Aprovada"
	case "rejected":
		return ToneDestructive, "Rejeitada"
	}
	return ToneSecondary, "Pendente"
}

// AdminReviewsLimit is how many rows the moderation page shows.
const AdminReviewsLimit = 200

var (
	sqlAdminReviews = sqlText(`SELECT r.id, p.name AS product_name, p.slug AS product_slug,
        u.name AS author_name, u.email AS author_email,
        r.rating, r.title, r.body, r.fit::text AS fit,
        r.status::text AS status, r.created_at
 FROM product_reviews r
 JOIN products p ON p.id = r.product_id
 JOIN users u ON u.id = r.user_id
 ORDER BY (r.status = 'pending') DESC,
          CASE WHEN r.status = 'pending' THEN r.created_at END ASC,
          r.moderated_at DESC NULLS LAST, r.created_at DESC
 LIMIT $1`)
	sqlPendingReviews = sqlText(`SELECT COUNT(*) FROM product_reviews WHERE status = 'pending'`)
	sqlReviewLock     = sqlText(`SELECT status::text, product_id FROM product_reviews WHERE id = $1 FOR UPDATE`)
	sqlModerateReview = sqlText(`UPDATE product_reviews SET status = $2::text::review_status, moderated_at = now(), moderated_by = $3 WHERE id = $1`)
)

// ListReviewsForAdmin is the moderation queue: pending first (FIFO), then
// the most recently moderated.
func ListReviewsForAdmin(ctx context.Context, q DBTX, limit int64) ([]AdminReview, error) {
	return collect[AdminReview](q.Query(ctx, sqlAdminReviews, limit))
}

// CountPendingReviews is the moderation badge.
func CountPendingReviews(ctx context.Context, q DBTX) (int64, error) {
	var n int64
	err := q.QueryRow(ctx, sqlPendingReviews).Scan(&n)
	return n, err
}

// ModerateReview approves or rejects a review with an audit row; false when
// the review does not exist.
func ModerateReview(ctx context.Context, pool *pgxpool.Pool, reviewID uuid.UUID, approve bool, actorID uuid.UUID, actorName string) (bool, error) {
	next := "rejected"
	action := "review.reject"
	if approve {
		next, action = "approved", "review.approve"
	}
	found := false
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var from string
		var productID uuid.UUID
		err := tx.QueryRow(ctx, sqlReviewLock, reviewID).Scan(&from, &productID)
		if ok, err := optional(err); err != nil || !ok {
			return err
		}
		found = true
		if _, err := tx.Exec(ctx, sqlModerateReview, reviewID, next, actorID); err != nil {
			return err
		}
		return audit.Record(ctx, tx, audit.Event{
			AggregateType: "product_review",
			AggregateID:   reviewID.String(),
			EntityType:    "product_review",
			EntityID:      reviewID,
			ActorID:       &actorID,
			ActorName:     &actorName,
			Metadata:      map[string]any{"action": action, "from": from, "to": next, "product_id": productID},
		})
	})
	return found, err
}

// ModerationMessage is the admin toast of a moderation form post
// (action "approve" | "reject"; ok=false for error toasts).
func ModerationMessage(found, approve bool) (string, bool) {
	switch {
	case !found:
		return "Avaliação não encontrada.", false
	case approve:
		return "Avaliação aprovada.", true
	}
	return "Avaliação rejeitada.", true
}
