package store_test

import (
	"errors"
	"strconv"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of the src/app/store/addresses.rs tests.

func sampleAddress(label string) store.AddressInput {
	return store.AddressInput{
		Label: label, RecipientName: "Maria Silva", ZipCode: "49000-000", Street: "Rua A", Number: "10",
		Neighborhood: "Centro", City: "Aracaju", State: "SE", Phone: "(79) 99981-6511",
	}
}

func TestAddressValidationUsesCEPRules(t *testing.T) {
	pool := testutil.NewPool(t)
	user := insertUser(t, pool, "U")
	id, err := store.CreateAddress(ctx, pool, user, sampleAddress("Casa"))
	mustNoErr(t, err)
	a, err := store.GetAddress(ctx, pool, user, id)
	mustNoErr(t, err)
	if a.ZipCode != "49000000" || a.State != "SE" || a.Phone != "79999816511" || a.FormattedZip() != "49000-000" {
		t.Fatalf("%+v", a)
	}
	if got := a.OneLine(); got != "Rua A, 10 — Centro, Aracaju/SE — CEP 49000-000" {
		t.Fatal(got)
	}
	for _, mut := range []func(*store.AddressInput){
		func(i *store.AddressInput) { i.State = "SP" },
		func(i *store.AddressInput) { i.Phone = "9999" },
		func(i *store.AddressInput) { i.Street = "  " },
	} {
		in := sampleAddress("Casa")
		mut(&in)
		if _, err := store.CreateAddress(ctx, pool, user, in); !store.IsUserError(err) {
			t.Errorf("%+v: %v", in, err)
		}
	}
}

func TestFirstAddressIsDefaultAndDefaultMoves(t *testing.T) {
	pool := testutil.NewPool(t)
	user := insertUser(t, pool, "U")
	first, err := store.CreateAddress(ctx, pool, user, sampleAddress("Casa"))
	mustNoErr(t, err)
	work := sampleAddress("Trabalho")
	work.Street = "Rua B"
	second, err := store.CreateAddress(ctx, pool, user, work)
	mustNoErr(t, err)
	list, err := store.ListAddresses(ctx, pool, user)
	mustNoErr(t, err)
	if len(list) != 2 || list[0].ID != first || !list[0].IsDefault {
		t.Fatalf("%+v", list)
	}
	mustNoErr(t, store.SetDefaultAddress(ctx, pool, user, second))
	list, _ = store.ListAddresses(ctx, pool, user)
	defaults := 0
	for _, a := range list {
		if a.IsDefault {
			defaults++
		}
	}
	if list[0].ID != second || defaults != 1 {
		t.Fatalf("%+v", list)
	}
	mustNoErr(t, store.DeleteAddress(ctx, pool, user, second))
	list, _ = store.ListAddresses(ctx, pool, user)
	if len(list) != 1 || !list[0].IsDefault {
		t.Fatalf("%+v", list)
	}
}

func TestUsersCannotTouchEachOthersAddresses(t *testing.T) {
	pool := testutil.NewPool(t)
	owner := insertUser(t, pool, "U")
	intruder := insertUser(t, pool, "I")
	id, err := store.CreateAddress(ctx, pool, owner, sampleAddress("Casa"))
	mustNoErr(t, err)
	if a, _ := store.GetAddress(ctx, pool, intruder, id); a != nil {
		t.Fatal("foreign address visible")
	}
	edit := sampleAddress("Hack")
	edit.Street = "Rua Invasora"
	for _, err := range []error{
		store.UpdateAddress(ctx, pool, intruder, id, edit),
		store.SetDefaultAddress(ctx, pool, intruder, id),
		store.DeleteAddress(ctx, pool, intruder, id),
	} {
		if !errors.Is(err, store.ErrAddressNotFound) {
			t.Fatal(err)
		}
	}
	kept, _ := store.GetAddress(ctx, pool, owner, id)
	if kept.Street != "Rua A" || kept.Label != "Casa" {
		t.Fatalf("%+v", kept)
	}
	mustNoErr(t, store.UpdateAddress(ctx, pool, owner, id, edit))
	edited, _ := store.GetAddress(ctx, pool, owner, id)
	if edited.Street != "Rua Invasora" || !edited.IsDefault {
		t.Fatalf("%+v", edited)
	}
}

func TestLimitOfTenAddresses(t *testing.T) {
	pool := testutil.NewPool(t)
	user := insertUser(t, pool, "U")
	for n := range store.MaxAddressesPerUser {
		in := sampleAddress("Casa")
		in.Number = strconv.Itoa(n)
		_, err := store.CreateAddress(ctx, pool, user, in)
		mustNoErr(t, err)
	}
	if _, err := store.CreateAddress(ctx, pool, user, sampleAddress("Extra")); !errors.Is(err, store.ErrAddressLimit) {
		t.Fatal(err)
	}
	other := sampleAddress("Extra")
	other.Number = "999"
	if added, err := store.SaveAddressFromCheckout(ctx, pool, user, other); err != nil || added {
		t.Fatal(added, err)
	}
}

func TestCheckoutSaveSkipsDuplicates(t *testing.T) {
	pool := testutil.NewPool(t)
	user := insertUser(t, pool, "U")
	if added, err := store.SaveAddressFromCheckout(ctx, pool, user, sampleAddress("")); err != nil || !added {
		t.Fatal(added, err)
	}
	same := sampleAddress("")
	same.Street, same.ZipCode = "rua a", "49000000"
	if added, err := store.SaveAddressFromCheckout(ctx, pool, user, same); err != nil || added {
		t.Fatal(added, err)
	}
	list, _ := store.ListAddresses(ctx, pool, user)
	if len(list) != 1 || !list[0].IsDefault {
		t.Fatalf("%+v", list)
	}
}
