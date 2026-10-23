package store

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Saved delivery addresses (src/app/store/addresses.rs), managed under
// /conta/enderecos and offered as checkout prefill. Every query is scoped
// by user id: another account's address id behaves like an unknown id.

// MaxAddressesPerUser caps saved addresses.
const MaxAddressesPerUser = 10

const (
	addressLabelMax = 40
	addressFieldMax = 120
)

// Address errors.
var (
	ErrAddressNotFound = errors.New("Endereço não encontrado.")
	ErrAddressLimit    = fmt.Errorf("Você pode salvar até %d endereços. Remova um para adicionar outro.", MaxAddressesPerUser)
)

// AddressInput is the raw form (also built from the checkout form).
type AddressInput struct {
	Label         string
	RecipientName string
	ZipCode       string
	Street        string
	Number        string
	Complement    string
	Neighborhood  string
	City          string
	State         string
	Phone         string
	// IsDefault is the "use as default" checkbox.
	IsDefault bool
}

// SavedAddress is a user_addresses row.
type SavedAddress struct {
	ID            uuid.UUID `db:"id"`
	Label         string    `db:"label"`
	RecipientName string    `db:"recipient_name"`
	ZipCode       string    `db:"zip_code"`
	Street        string    `db:"street"`
	Number        string    `db:"number"`
	Complement    string    `db:"complement"`
	Neighborhood  string    `db:"neighborhood"`
	City          string    `db:"city"`
	State         string    `db:"state"`
	Phone         string    `db:"phone"`
	IsDefault     bool      `db:"is_default"`
}

// FormattedZip is "12345-678".
func (a SavedAddress) FormattedZip() string {
	if len(a.ZipCode) == 8 {
		return a.ZipCode[:5] + "-" + a.ZipCode[5:]
	}
	return a.ZipCode
}

// OneLine is "Rua, 10 (apto) — Bairro, Cidade/UF — CEP 12345-678".
func (a SavedAddress) OneLine() string {
	complement := ""
	if a.Complement != "" {
		complement = " (" + a.Complement + ")"
	}
	return fmt.Sprintf("%s, %s%s — %s, %s/%s — CEP %s", a.Street, a.Number, complement, a.Neighborhood, a.City, a.State, a.FormattedZip())
}

// Title is the label, or "street, number" without one (checkout picker).
func (a SavedAddress) Title() string {
	if a.Label == "" {
		return a.Street + ", " + a.Number
	}
	return a.Label
}

// AsCheckoutInput pre-fills the checkout form.
func (a SavedAddress) AsCheckoutInput() CheckoutInput {
	return CheckoutInput{
		Name: a.RecipientName, Phone: a.Phone, Street: a.Street, Number: a.Number,
		Complement: a.Complement, Neighborhood: a.Neighborhood, City: a.City,
		State: a.State, ZipCode: a.FormattedZip(),
	}
}

type validAddress struct {
	label, recipient, zip, street, number, complement, neighborhood, city, state, phone string
}

func required(value, message string, maxChars int) (string, error) {
	v := strings.TrimSpace(value)
	if v == "" {
		return "", userErr(message)
	}
	if runeLen(v) > maxChars {
		return "", userErr(fmt.Sprintf("%s (máximo de %d caracteres)", strings.TrimRight(message, "."), maxChars))
	}
	return v, nil
}

// validateAddress applies the checkout rules: WhatsApp with DDD (10–11
// digits) and the UF derived from the CEP.
func validateAddress(in AddressInput) (validAddress, error) {
	var v validAddress
	var err error
	if v.recipient, err = required(in.RecipientName, "Informe o nome de quem recebe.", addressFieldMax); err != nil {
		return v, err
	}
	v.phone = digitsOnly(in.Phone)
	if len(v.phone) < 10 || len(v.phone) > 11 {
		return v, userErr("Informe um WhatsApp válido (DDD + número).")
	}
	if v.street, err = required(in.Street, "Informe a rua.", addressFieldMax); err != nil {
		return v, err
	}
	if v.number, err = required(in.Number, "Informe o número.", 20); err != nil {
		return v, err
	}
	if v.neighborhood, err = required(in.Neighborhood, "Informe o bairro.", addressFieldMax); err != nil {
		return v, err
	}
	if v.city, err = required(in.City, "Informe a cidade.", addressFieldMax); err != nil {
		return v, err
	}
	v.complement = strings.TrimSpace(in.Complement)
	if runeLen(v.complement) > addressFieldMax {
		return v, userErr("Complemento muito longo.")
	}
	v.label = strings.TrimSpace(in.Label)
	if runeLen(v.label) > addressLabelMax {
		return v, userErr(fmt.Sprintf("Use um apelido com até %d caracteres.", addressLabelMax))
	}
	if v.state, err = ResolveShippingUF(in.ZipCode, in.State); err != nil {
		return v, err
	}
	v.zip = CEPDigits(in.ZipCode)
	return v, nil
}

const addressColumns = `id, label, recipient_name, zip_code, street, number, complement,
  neighborhood, city, state, phone, is_default`

var (
	sqlListAddresses = sqlText(`SELECT ` + addressColumns + ` FROM user_addresses WHERE user_id = $1
 ORDER BY is_default DESC, created_at DESC`)
	sqlGetAddress     = sqlText(`SELECT ` + addressColumns + ` FROM user_addresses WHERE id = $1 AND user_id = $2`)
	sqlLockUser       = sqlText(`SELECT id FROM users WHERE id = $1 FOR UPDATE`)
	sqlClearDefault   = sqlText(`UPDATE user_addresses SET is_default = false, updated_at = now() WHERE user_id = $1 AND is_default`)
	sqlCountAddresses = sqlText(`SELECT COUNT(*) FROM user_addresses WHERE user_id = $1`)
	sqlInsertAddress  = sqlText(`INSERT INTO user_addresses (id, user_id, label, recipient_name, zip_code, street,
   number, complement, neighborhood, city, state, phone, is_default)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)`)
	sqlAddressIsDefault = sqlText(`SELECT is_default FROM user_addresses WHERE id = $1 AND user_id = $2`)
	sqlUpdateAddress    = sqlText(`UPDATE user_addresses SET label = $3, recipient_name = $4, zip_code = $5,
   street = $6, number = $7, complement = $8, neighborhood = $9, city = $10,
   state = $11, phone = $12, is_default = $13, updated_at = now()
 WHERE id = $1 AND user_id = $2`)
	sqlDeleteAddress = sqlText(`DELETE FROM user_addresses WHERE id = $1 AND user_id = $2 RETURNING is_default`)
	sqlPromoteLatest = sqlText(`UPDATE user_addresses SET is_default = true, updated_at = now()
 WHERE id = (SELECT id FROM user_addresses WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1)`)
	sqlAddressOwned = sqlText(`SELECT id FROM user_addresses WHERE id = $1 AND user_id = $2`)
	sqlSetDefault   = sqlText(`UPDATE user_addresses SET is_default = true, updated_at = now() WHERE id = $1 AND user_id = $2`)
	sqlCheckoutDupe = sqlText(`SELECT COUNT(*),
        COALESCE(bool_or(zip_code = $2
                         AND lower(street) = lower($3)
                         AND lower(number) = lower($4)
                         AND lower(complement) = lower($5)), false)
 FROM user_addresses WHERE user_id = $1`)
)

// ListAddresses lists the user's addresses, default first.
func ListAddresses(ctx context.Context, q DBTX, userID uuid.UUID) ([]SavedAddress, error) {
	return collect[SavedAddress](q.Query(ctx, sqlListAddresses, userID))
}

// GetAddress loads one of the user's addresses (nil when not theirs).
func GetAddress(ctx context.Context, q DBTX, userID, addressID uuid.UUID) (*SavedAddress, error) {
	return collectOne[SavedAddress](q.Query(ctx, sqlGetAddress, addressID, userID))
}

func lockUser(ctx context.Context, tx DBTX, userID uuid.UUID) error {
	var id uuid.UUID
	err := tx.QueryRow(ctx, sqlLockUser, userID).Scan(&id)
	if found, err := optional(err); err != nil {
		return err
	} else if !found {
		return ErrAddressNotFound
	}
	return nil
}

func insertAddress(ctx context.Context, tx DBTX, id, userID uuid.UUID, v validAddress, isDefault bool) error {
	_, err := tx.Exec(ctx, sqlInsertAddress, id, userID, v.label, v.recipient, v.zip, v.street, v.number,
		v.complement, v.neighborhood, v.city, v.state, v.phone, isDefault)
	return err
}

// CreateAddress saves a new address (the first one is always default).
func CreateAddress(ctx context.Context, pool *pgxpool.Pool, userID uuid.UUID, in AddressInput) (uuid.UUID, error) {
	v, err := validateAddress(in)
	if err != nil {
		return uuid.Nil, err
	}
	id := uuid.Must(uuid.NewV7())
	err = withTx(ctx, pool, func(tx pgx.Tx) error {
		if err := lockUser(ctx, tx, userID); err != nil {
			return err
		}
		var count int64
		if err := tx.QueryRow(ctx, sqlCountAddresses, userID).Scan(&count); err != nil {
			return err
		}
		if count >= MaxAddressesPerUser {
			return ErrAddressLimit
		}
		makeDefault := in.IsDefault || count == 0
		if makeDefault {
			if _, err := tx.Exec(ctx, sqlClearDefault, userID); err != nil {
				return err
			}
		}
		return insertAddress(ctx, tx, id, userID, v, makeDefault)
	})
	if err != nil {
		return uuid.Nil, err
	}
	return id, nil
}

// UpdateAddress edits an address. Unticking "default" on the current
// default is ignored (some address must stay default).
func UpdateAddress(ctx context.Context, pool *pgxpool.Pool, userID, addressID uuid.UUID, in AddressInput) error {
	v, err := validateAddress(in)
	if err != nil {
		return err
	}
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		if err := lockUser(ctx, tx, userID); err != nil {
			return err
		}
		var was bool
		err := tx.QueryRow(ctx, sqlAddressIsDefault, addressID, userID).Scan(&was)
		if found, err := optional(err); err != nil {
			return err
		} else if !found {
			return ErrAddressNotFound
		}
		makeDefault := was || in.IsDefault
		if makeDefault && !was {
			if _, err := tx.Exec(ctx, sqlClearDefault, userID); err != nil {
				return err
			}
		}
		_, err = tx.Exec(ctx, sqlUpdateAddress, addressID, userID, v.label, v.recipient, v.zip, v.street,
			v.number, v.complement, v.neighborhood, v.city, v.state, v.phone, makeDefault)
		return err
	})
}

// DeleteAddress removes an address; deleting the default promotes the
// most recent remaining one.
func DeleteAddress(ctx context.Context, pool *pgxpool.Pool, userID, addressID uuid.UUID) error {
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		if err := lockUser(ctx, tx, userID); err != nil {
			return err
		}
		var was bool
		err := tx.QueryRow(ctx, sqlDeleteAddress, addressID, userID).Scan(&was)
		if found, err := optional(err); err != nil {
			return err
		} else if !found {
			return ErrAddressNotFound
		}
		if was {
			_, err = tx.Exec(ctx, sqlPromoteLatest, userID)
		}
		return err
	})
}

// SetDefaultAddress makes an address the default.
func SetDefaultAddress(ctx context.Context, pool *pgxpool.Pool, userID, addressID uuid.UUID) error {
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		if err := lockUser(ctx, tx, userID); err != nil {
			return err
		}
		var id uuid.UUID
		err := tx.QueryRow(ctx, sqlAddressOwned, addressID, userID).Scan(&id)
		if found, err := optional(err); err != nil {
			return err
		} else if !found {
			return ErrAddressNotFound
		}
		if _, err := tx.Exec(ctx, sqlClearDefault, userID); err != nil {
			return err
		}
		_, err = tx.Exec(ctx, sqlSetDefault, addressID, userID)
		return err
	})
}

// SaveAddressFromCheckout is "Salvar este endereço": skips silently on a
// duplicate street/number/CEP or at the limit. Returns whether a row was
// added.
func SaveAddressFromCheckout(ctx context.Context, pool *pgxpool.Pool, userID uuid.UUID, in AddressInput) (bool, error) {
	v, err := validateAddress(in)
	if err != nil {
		return false, err
	}
	added := false
	err = withTx(ctx, pool, func(tx pgx.Tx) error {
		if err := lockUser(ctx, tx, userID); err != nil {
			return err
		}
		var count int64
		var dup bool
		if err := tx.QueryRow(ctx, sqlCheckoutDupe, userID, v.zip, v.street, v.number, v.complement).Scan(&count, &dup); err != nil {
			return err
		}
		if dup || count >= MaxAddressesPerUser {
			return nil
		}
		added = true
		return insertAddress(ctx, tx, uuid.Must(uuid.NewV7()), userID, v, count == 0)
	})
	return added, err
}

// AddressErrorMessage maps an address error to its toast ("" = internal).
func AddressErrorMessage(err error) string {
	if msg, ok := UserMessage(err); ok {
		return msg
	}
	if errors.Is(err, ErrAddressNotFound) || errors.Is(err, ErrAddressLimit) {
		return err.Error()
	}
	return ""
}
