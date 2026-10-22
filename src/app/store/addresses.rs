//! Saved delivery addresses (`user_addresses`), managed under
//! `/conta/enderecos` and offered as prefill at checkout.
//!
//! Every query is scoped by `user_id`: an address id from another account
//! behaves exactly like an unknown id.

use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::shipping::{cep_digits, resolve_shipping_uf};

pub const MAX_ADDRESSES_PER_USER: i64 = 10;
const LABEL_MAX_CHARS: usize = 40;
const FIELD_MAX_CHARS: usize = 120;

/// Raw form fields (also built from the checkout form).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AddressInput {
  #[serde(default)]
  pub label: String,
  #[serde(default)]
  pub recipient_name: String,
  #[serde(default)]
  pub zip_code: String,
  #[serde(default)]
  pub street: String,
  #[serde(default)]
  pub number: String,
  #[serde(default)]
  pub complement: String,
  #[serde(default)]
  pub neighborhood: String,
  #[serde(default)]
  pub city: String,
  #[serde(default)]
  pub state: String,
  #[serde(default)]
  pub phone: String,
  /// Checkbox: present = "use as default".
  #[serde(default)]
  pub is_default: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SavedAddress {
  pub id: Uuid,
  pub label: String,
  pub recipient_name: String,
  pub zip_code: String,
  pub street: String,
  pub number: String,
  pub complement: String,
  pub neighborhood: String,
  pub city: String,
  pub state: String,
  pub phone: String,
  pub is_default: bool,
}

impl SavedAddress {
  /// `12345-678`.
  pub fn formatted_zip(&self) -> String {
    if self.zip_code.len() == 8 {
      format!("{}-{}", &self.zip_code[..5], &self.zip_code[5..])
    } else {
      self.zip_code.clone()
    }
  }

  pub fn one_line(&self) -> String {
    let complement = if self.complement.is_empty() {
      String::new()
    } else {
      format!(" ({})", self.complement)
    };
    format!(
      "{}, {}{} — {}, {}/{} — CEP {}",
      self.street,
      self.number,
      complement,
      self.neighborhood,
      self.city,
      self.state,
      self.formatted_zip()
    )
  }
}

/// Normalized, validated address ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidAddress {
  label: String,
  recipient_name: String,
  zip_code: String,
  street: String,
  number: String,
  complement: String,
  neighborhood: String,
  city: String,
  state: &'static str,
  phone: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AddressError {
  #[error("{0}")]
  Invalid(String),
  #[error("Endereço não encontrado.")]
  NotFound,
  #[error(
    "Você pode salvar até {MAX_ADDRESSES_PER_USER} endereços. Remova um para adicionar outro."
  )]
  LimitReached,
  #[error(transparent)]
  Db(#[from] sqlx::Error),
}

fn required(
  value: &str,
  message: &str,
  max: usize,
) -> Result<String, AddressError> {
  let value = value.trim();
  if value.is_empty() {
    return Err(AddressError::Invalid(message.to_string()));
  }
  if value.chars().count() > max {
    return Err(AddressError::Invalid(format!(
      "{} (máximo de {max} caracteres)",
      message.trim_end_matches('.')
    )));
  }
  Ok(value.to_string())
}

/// Same rules as checkout: digits-only WhatsApp with DDD (10–11 digits) and
/// the UF derived from the CEP via [`resolve_shipping_uf`].
fn validate(input: &AddressInput) -> Result<ValidAddress, AddressError> {
  let recipient_name = required(
    &input.recipient_name,
    "Informe o nome de quem recebe.",
    FIELD_MAX_CHARS,
  )?;
  let phone: String =
    input.phone.chars().filter(|c| c.is_ascii_digit()).collect();
  if !(10..=11).contains(&phone.len()) {
    return Err(AddressError::Invalid(
      "Informe um WhatsApp válido (DDD + número).".to_string(),
    ));
  }
  let street = required(&input.street, "Informe a rua.", FIELD_MAX_CHARS)?;
  let number = required(&input.number, "Informe o número.", 20)?;
  let neighborhood =
    required(&input.neighborhood, "Informe o bairro.", FIELD_MAX_CHARS)?;
  let city = required(&input.city, "Informe a cidade.", FIELD_MAX_CHARS)?;
  let complement = input.complement.trim().to_string();
  if complement.chars().count() > FIELD_MAX_CHARS {
    return Err(AddressError::Invalid(
      "Complemento muito longo.".to_string(),
    ));
  }
  let label = input.label.trim().to_string();
  if label.chars().count() > LABEL_MAX_CHARS {
    return Err(AddressError::Invalid(format!(
      "Use um apelido com até {LABEL_MAX_CHARS} caracteres."
    )));
  }
  let state = resolve_shipping_uf(&input.zip_code, &input.state)
    .map_err(AddressError::Invalid)?;
  Ok(ValidAddress {
    label,
    recipient_name,
    zip_code: cep_digits(&input.zip_code),
    street,
    number,
    complement,
    neighborhood,
    city,
    state,
    phone,
  })
}

const SELECT_COLUMNS: &str =
  "id, label, recipient_name, zip_code, street, number, complement,
  neighborhood, city, state, phone, is_default";

pub async fn list_for_user(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<SavedAddress>, sqlx::Error> {
  sqlx::query_as::<_, SavedAddress>(&format!(
    "SELECT {SELECT_COLUMNS} FROM user_addresses WHERE user_id = $1
     ORDER BY is_default DESC, created_at DESC"
  ))
  .bind(user_id)
  .fetch_all(pool)
  .await
}

pub async fn get_for_user(
  pool: &PgPool,
  user_id: Uuid,
  address_id: Uuid,
) -> Result<Option<SavedAddress>, sqlx::Error> {
  sqlx::query_as::<_, SavedAddress>(&format!(
    "SELECT {SELECT_COLUMNS} FROM user_addresses WHERE id = $1 AND user_id = $2"
  ))
  .bind(address_id)
  .bind(user_id)
  .fetch_optional(pool)
  .await
}

/// Serializes address writes per user (limit check + default flag) on the
/// owning `users` row.
async fn lock_user(
  tx: &mut Transaction<'_, Postgres>,
  user_id: Uuid,
) -> Result<(), AddressError> {
  let found: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM users WHERE id = $1 FOR UPDATE")
      .bind(user_id)
      .fetch_optional(&mut **tx)
      .await?;
  found.map(|_| ()).ok_or(AddressError::NotFound)
}

async fn clear_default(
  tx: &mut Transaction<'_, Postgres>,
  user_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE user_addresses SET is_default = false, updated_at = now()
     WHERE user_id = $1 AND is_default",
  )
  .bind(user_id)
  .execute(&mut **tx)
  .await?;
  Ok(())
}

pub async fn create(
  pool: &PgPool,
  user_id: Uuid,
  input: &AddressInput,
) -> Result<Uuid, AddressError> {
  let valid = validate(input)?;
  let mut tx = pool.begin().await?;
  lock_user(&mut tx, user_id).await?;
  let count: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM user_addresses WHERE user_id = $1",
  )
  .bind(user_id)
  .fetch_one(&mut *tx)
  .await?;
  if count >= MAX_ADDRESSES_PER_USER {
    return Err(AddressError::LimitReached);
  }
  // The first address is always the default.
  let make_default = input.is_default.is_some() || count == 0;
  if make_default {
    clear_default(&mut tx, user_id).await?;
  }
  let id = Uuid::now_v7();
  insert(&mut tx, id, user_id, &valid, make_default).await?;
  tx.commit().await?;
  Ok(id)
}

async fn insert(
  tx: &mut Transaction<'_, Postgres>,
  id: Uuid,
  user_id: Uuid,
  valid: &ValidAddress,
  is_default: bool,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "INSERT INTO user_addresses (id, user_id, label, recipient_name, zip_code, street,
       number, complement, neighborhood, city, state, phone, is_default)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
  )
  .bind(id)
  .bind(user_id)
  .bind(&valid.label)
  .bind(&valid.recipient_name)
  .bind(&valid.zip_code)
  .bind(&valid.street)
  .bind(&valid.number)
  .bind(&valid.complement)
  .bind(&valid.neighborhood)
  .bind(&valid.city)
  .bind(valid.state)
  .bind(&valid.phone)
  .bind(is_default)
  .execute(&mut **tx)
  .await?;
  Ok(())
}

pub async fn update(
  pool: &PgPool,
  user_id: Uuid,
  address_id: Uuid,
  input: &AddressInput,
) -> Result<(), AddressError> {
  let valid = validate(input)?;
  let mut tx = pool.begin().await?;
  lock_user(&mut tx, user_id).await?;
  let owned: Option<bool> = sqlx::query_scalar(
    "SELECT is_default FROM user_addresses WHERE id = $1 AND user_id = $2",
  )
  .bind(address_id)
  .bind(user_id)
  .fetch_optional(&mut *tx)
  .await?;
  let Some(was_default) = owned else {
    return Err(AddressError::NotFound);
  };
  // Unticking "default" on the current default is ignored: some address
  // must stay default, and the customer picks another one explicitly.
  let make_default = was_default || input.is_default.is_some();
  if make_default && !was_default {
    clear_default(&mut tx, user_id).await?;
  }
  sqlx::query(
    "UPDATE user_addresses SET label = $3, recipient_name = $4, zip_code = $5,
       street = $6, number = $7, complement = $8, neighborhood = $9, city = $10,
       state = $11, phone = $12, is_default = $13, updated_at = now()
     WHERE id = $1 AND user_id = $2",
  )
  .bind(address_id)
  .bind(user_id)
  .bind(&valid.label)
  .bind(&valid.recipient_name)
  .bind(&valid.zip_code)
  .bind(&valid.street)
  .bind(&valid.number)
  .bind(&valid.complement)
  .bind(&valid.neighborhood)
  .bind(&valid.city)
  .bind(valid.state)
  .bind(&valid.phone)
  .bind(make_default)
  .execute(&mut *tx)
  .await?;
  tx.commit().await?;
  Ok(())
}

/// Deletes the address; if it was the default, the most recent remaining
/// address takes over.
pub async fn delete(
  pool: &PgPool,
  user_id: Uuid,
  address_id: Uuid,
) -> Result<(), AddressError> {
  let mut tx = pool.begin().await?;
  lock_user(&mut tx, user_id).await?;
  let removed: Option<bool> = sqlx::query_scalar(
    "DELETE FROM user_addresses WHERE id = $1 AND user_id = $2 RETURNING is_default",
  )
  .bind(address_id)
  .bind(user_id)
  .fetch_optional(&mut *tx)
  .await?;
  let Some(was_default) = removed else {
    return Err(AddressError::NotFound);
  };
  if was_default {
    sqlx::query(
      "UPDATE user_addresses SET is_default = true, updated_at = now()
       WHERE id = (SELECT id FROM user_addresses WHERE user_id = $1
                   ORDER BY created_at DESC LIMIT 1)",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
  }
  tx.commit().await?;
  Ok(())
}

pub async fn set_default(
  pool: &PgPool,
  user_id: Uuid,
  address_id: Uuid,
) -> Result<(), AddressError> {
  let mut tx = pool.begin().await?;
  lock_user(&mut tx, user_id).await?;
  let owned: Option<Uuid> = sqlx::query_scalar(
    "SELECT id FROM user_addresses WHERE id = $1 AND user_id = $2",
  )
  .bind(address_id)
  .bind(user_id)
  .fetch_optional(&mut *tx)
  .await?;
  if owned.is_none() {
    return Err(AddressError::NotFound);
  }
  clear_default(&mut tx, user_id).await?;
  sqlx::query(
    "UPDATE user_addresses SET is_default = true, updated_at = now()
     WHERE id = $1 AND user_id = $2",
  )
  .bind(address_id)
  .bind(user_id)
  .execute(&mut *tx)
  .await?;
  tx.commit().await?;
  Ok(())
}

/// "Salvar este endereço" at checkout. Skips silently when the same
/// street/number/CEP is already saved or the limit is reached; the order
/// itself never depends on this. Returns whether a row was added.
pub async fn save_from_checkout(
  pool: &PgPool,
  user_id: Uuid,
  input: &AddressInput,
) -> Result<bool, AddressError> {
  let valid = validate(input)?;
  let mut tx = pool.begin().await?;
  lock_user(&mut tx, user_id).await?;
  let (count, duplicate): (i64, bool) = sqlx::query_as(
    "SELECT COUNT(*),
            COALESCE(bool_or(zip_code = $2
                             AND lower(street) = lower($3)
                             AND lower(number) = lower($4)
                             AND lower(complement) = lower($5)), false)
     FROM user_addresses WHERE user_id = $1",
  )
  .bind(user_id)
  .bind(&valid.zip_code)
  .bind(&valid.street)
  .bind(&valid.number)
  .bind(&valid.complement)
  .fetch_one(&mut *tx)
  .await?;
  if duplicate || count >= MAX_ADDRESSES_PER_USER {
    return Ok(false);
  }
  insert(&mut tx, Uuid::now_v7(), user_id, &valid, count == 0).await?;
  tx.commit().await?;
  Ok(true)
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn seed_user(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, 'U', $2)")
      .bind(id)
      .bind(format!("u-{}@example.com", id.simple()))
      .execute(pool)
      .await
      .unwrap();
    id
  }

  fn sample(label: &str) -> AddressInput {
    AddressInput {
      label: label.to_string(),
      recipient_name: "Maria Silva".to_string(),
      zip_code: "49000-000".to_string(),
      street: "Rua A".to_string(),
      number: "10".to_string(),
      complement: String::new(),
      neighborhood: "Centro".to_string(),
      city: "Aracaju".to_string(),
      state: "SE".to_string(),
      phone: "(79) 99981-6511".to_string(),
      is_default: None,
    }
  }

  #[test]
  fn validation_uses_cep_rules() {
    let ok = validate(&sample("Casa")).unwrap();
    assert_eq!(ok.zip_code, "49000000");
    assert_eq!(ok.state, "SE");
    assert_eq!(ok.phone, "79999816511");

    let mut wrong_uf = sample("Casa");
    wrong_uf.state = "SP".to_string();
    assert!(matches!(validate(&wrong_uf), Err(AddressError::Invalid(_))));

    let mut short_phone = sample("Casa");
    short_phone.phone = "9999".to_string();
    assert!(matches!(
      validate(&short_phone),
      Err(AddressError::Invalid(_))
    ));

    let mut no_street = sample("Casa");
    no_street.street = "  ".to_string();
    assert!(matches!(
      validate(&no_street),
      Err(AddressError::Invalid(_))
    ));
  }

  #[tokio::test]
  async fn first_address_is_default_and_default_moves() {
    let pool = crate::test_support::fresh_pool().await;
    let user = seed_user(&pool).await;
    let first = create(&pool, user, &sample("Casa")).await.unwrap();
    let mut work = sample("Trabalho");
    work.street = "Rua B".to_string();
    let second = create(&pool, user, &work).await.unwrap();
    let list = list_for_user(&pool, user).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, first);
    assert!(list[0].is_default);

    set_default(&pool, user, second).await.unwrap();
    let list = list_for_user(&pool, user).await.unwrap();
    assert_eq!(list[0].id, second);
    assert_eq!(list.iter().filter(|a| a.is_default).count(), 1);

    // Deleting the default promotes the remaining address.
    delete(&pool, user, second).await.unwrap();
    let list = list_for_user(&pool, user).await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].is_default);
  }

  #[tokio::test]
  async fn users_cannot_touch_each_others_addresses() {
    let pool = crate::test_support::fresh_pool().await;
    let owner = seed_user(&pool).await;
    let intruder = seed_user(&pool).await;
    let address = create(&pool, owner, &sample("Casa")).await.unwrap();

    assert!(
      get_for_user(&pool, intruder, address)
        .await
        .unwrap()
        .is_none()
    );
    let mut edit = sample("Hack");
    edit.street = "Rua Invasora".to_string();
    assert!(matches!(
      update(&pool, intruder, address, &edit).await,
      Err(AddressError::NotFound)
    ));
    assert!(matches!(
      set_default(&pool, intruder, address).await,
      Err(AddressError::NotFound)
    ));
    assert!(matches!(
      delete(&pool, intruder, address).await,
      Err(AddressError::NotFound)
    ));

    let kept = get_for_user(&pool, owner, address).await.unwrap().unwrap();
    assert_eq!(kept.street, "Rua A");
    assert_eq!(kept.label, "Casa");

    // The owner can edit it.
    update(&pool, owner, address, &edit).await.unwrap();
    let edited = get_for_user(&pool, owner, address).await.unwrap().unwrap();
    assert_eq!(edited.street, "Rua Invasora");
    assert!(edited.is_default);
  }

  #[tokio::test]
  async fn limit_of_ten_addresses() {
    let pool = crate::test_support::fresh_pool().await;
    let user = seed_user(&pool).await;
    for n in 0..MAX_ADDRESSES_PER_USER {
      let mut input = sample("Casa");
      input.number = n.to_string();
      create(&pool, user, &input).await.unwrap();
    }
    assert!(matches!(
      create(&pool, user, &sample("Extra")).await,
      Err(AddressError::LimitReached)
    ));
    let mut other = sample("Extra");
    other.number = "999".to_string();
    assert!(!save_from_checkout(&pool, user, &other).await.unwrap());
  }

  #[tokio::test]
  async fn checkout_save_skips_duplicates() {
    let pool = crate::test_support::fresh_pool().await;
    let user = seed_user(&pool).await;
    assert!(save_from_checkout(&pool, user, &sample("")).await.unwrap());
    let mut same = sample("");
    same.street = "rua a".to_string();
    same.zip_code = "49000000".to_string();
    assert!(!save_from_checkout(&pool, user, &same).await.unwrap());
    let list = list_for_user(&pool, user).await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].is_default);
  }
}
