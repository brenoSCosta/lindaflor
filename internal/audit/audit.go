// Package audit appends privileged mutations to the audit_events trail
// (port of src/audit.rs).
package audit

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Event is one audit_events row.
type Event struct {
	AggregateType string
	AggregateID   string
	EntityType    string
	EntityID      uuid.UUID
	ActorID       *uuid.UUID
	ActorName     *string
	// Metadata is marshaled to JSON (nil → {}).
	Metadata any
}

// Record inserts the event using db (pool or the caller's transaction, so
// the trail commits atomically with the mutation it describes).
func Record(ctx context.Context, db sqlc.DBTX, e Event) error {
	meta := json.RawMessage("{}")
	if e.Metadata != nil {
		b, err := json.Marshal(e.Metadata)
		if err != nil {
			return fmt.Errorf("audit metadata: %w", err)
		}
		meta = b
	}
	id, err := uuid.NewV7()
	if err != nil {
		return err
	}
	return sqlc.New(db).InsertAuditEvent(ctx, sqlc.InsertAuditEventParams{
		ID:            id,
		AggregateType: e.AggregateType,
		AggregateID:   e.AggregateID,
		EntityType:    e.EntityType,
		EntityID:      e.EntityID,
		ActorUserID:   e.ActorID,
		ActorName:     e.ActorName,
		Metadata:      meta,
	})
}
