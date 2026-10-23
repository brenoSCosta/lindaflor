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
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
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

// EligibleOrder is the user's most recent received order containing the
// product, nil when none.
func EligibleOrder(ctx context.Context, q DBTX, userID, productID uuid.UUID) (*uuid.UUID, error) {
	id, found, err := noRows(queries(q).StoreEligibleOrder(ctx, sqlc.StoreEligibleOrderParams{UserID: userID, ProductID: productID}))
	if err != nil || !found {
		return nil, err
	}
	return &id, nil
}

// OwnReviewStatus is the status of the user's review of the product ("" =
// none).
func OwnReviewStatus(ctx context.Context, q DBTX, userID, productID uuid.UUID) (string, error) {
	s, _, err := noRows(queries(q).StoreOwnReviewStatus(ctx, sqlc.StoreOwnReviewStatusParams{UserID: userID, ProductID: productID}))
	return s, err
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
	id, found, err := noRows(queries(q).StoreInsertReview(ctx, sqlc.StoreInsertReviewParams{
		ID: uuid.Must(uuid.NewV7()), ProductID: productID, UserID: userID, OrderID: orderID,
		Rating: r.Rating, Title: r.Title, Body: r.Body, Fit: fit,
	}))
	if err != nil {
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

// LoadRatingSummary loads a product's aggregates.
func LoadRatingSummary(ctx context.Context, q DBTX, productID uuid.UUID) (RatingSummary, error) {
	s, err := queries(q).StoreRatingSummary(ctx, productID)
	return RatingSummary(s), err
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

// AllRatings maps product → approved rating (one query per page).
func AllRatings(ctx context.Context, q DBTX) (map[uuid.UUID]CardRating, error) {
	rows, err := queries(q).StoreAllRatings(ctx)
	if err != nil {
		return nil, err
	}
	out := make(map[uuid.UUID]CardRating, len(rows))
	for _, r := range rows {
		out[r.ProductID] = CardRating{Average: r.Average, Count: r.Count}
	}
	return out, nil
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

// ListApprovedReviews pages approved reviews, newest first.
func ListApprovedReviews(ctx context.Context, q DBTX, productID uuid.UUID, limit, offset int64) ([]PublicReview, error) {
	rows, err := queries(q).StoreListApprovedReviews(ctx, sqlc.StoreListApprovedReviewsParams{ProductID: productID, PageSize: limit, PageOffset: offset})
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreListApprovedReviewsRow) PublicReview { return PublicReview(r) }), nil
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

// ReviewableProductsForOrder lists the order's products the user may still
// review ("Avalie suas compras" card).
func ReviewableProductsForOrder(ctx context.Context, q DBTX, userID, orderID uuid.UUID) ([]ReviewableProduct, error) {
	rows, err := queries(q).StoreReviewableForOrder(ctx, sqlc.StoreReviewableForOrderParams{UserID: userID, OrderID: orderID})
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreReviewableForOrderRow) ReviewableProduct { return ReviewableProduct(r) }), nil
}

// ReviewableOrderIDs are the user's received orders with something to review.
func ReviewableOrderIDs(ctx context.Context, q DBTX, userID uuid.UUID) (map[uuid.UUID]bool, error) {
	ids, err := queries(q).StoreReviewableOrders(ctx, userID)
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

// ListReviewsForAdmin is the moderation queue: pending first (FIFO), then
// the most recently moderated.
func ListReviewsForAdmin(ctx context.Context, q DBTX, limit int64) ([]AdminReview, error) {
	rows, err := queries(q).StoreAdminReviews(ctx, limit)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreAdminReviewsRow) AdminReview { return AdminReview(r) }), nil
}

// CountPendingReviews is the moderation badge.
func CountPendingReviews(ctx context.Context, q DBTX) (int64, error) {
	return queries(q).StorePendingReviewCount(ctx)
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
		q := queries(tx)
		locked, ok, err := noRows(q.StoreReviewLock(ctx, reviewID))
		if err != nil || !ok {
			return err
		}
		found = true
		from, productID := locked.Status, locked.ProductID
		if err := q.StoreModerateReview(ctx, sqlc.StoreModerateReviewParams{
			ID: reviewID, Status: sqlc.ReviewStatus(next), ModeratedBy: actorID,
		}); err != nil {
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
