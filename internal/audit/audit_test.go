package audit_test

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func TestRecordAppendsEvent(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	entity := uuid.New()
	name := "Admin"
	if err := audit.Record(ctx, pool, audit.Event{
		AggregateType: "order", AggregateID: entity.String(), EntityType: "order", EntityID: entity,
		ActorName: &name, Metadata: map[string]string{"from": "paid", "to": "shipped"},
	}); err != nil {
		t.Fatal(err)
	}
	if err := audit.Record(ctx, pool, audit.Event{AggregateType: "order", AggregateID: "x", EntityType: "order", EntityID: entity}); err != nil {
		t.Fatal(err)
	}
	rows, err := sqlc.New(pool).ListAuditEventsForEntity(ctx, sqlc.ListAuditEventsForEntityParams{EntityType: "order", EntityID: entity})
	if err != nil || len(rows) != 2 {
		t.Fatalf("rows=%d err=%v", len(rows), err)
	}
	var meta map[string]string
	if err := json.Unmarshal(rows[0].Metadata, &meta); err != nil || meta["to"] != "shipped" || *rows[0].ActorName != "Admin" {
		t.Fatalf("meta=%v", meta)
	}
	if string(rows[1].Metadata) != "{}" {
		t.Fatalf("nil metadata must be {}: %s", rows[1].Metadata)
	}
}
