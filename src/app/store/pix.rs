//! BR Code (PIX) payload encoding: EMV fields, CRC16/CCITT-FALSE,
//! merchant-name sanitizing and PIX key normalization.

/// Returned by [`crate::app::checkout`] order creation and shown on the
/// checkout page when the store has no PIX key configured.
pub const PIX_UNAVAILABLE_MESSAGE: &str =
  "Pagamento via PIX indisponível no momento. Fale com a loja pelo WhatsApp.";

fn emv_field(id: &str, value: &str) -> String {
  format!("{}{:02}{}", id, value.chars().count(), value)
}

fn crc16_ccitt_false(payload: &str) -> String {
  let mut crc: u16 = 0xffff;
  for b in payload.bytes() {
    crc ^= (b as u16) << 8;
    for _ in 0..8 {
      if (crc & 0x8000) != 0 {
        crc = (crc << 1) ^ 0x1021;
      } else {
        crc <<= 1;
      }
    }
  }
  format!("{:04X}", crc)
}

/// Maps an accented Latin letter to its plain ASCII base. Anything else is
/// returned unchanged so the caller can uppercase and filter it.
fn transliterate_to_ascii(value: char) -> char {
  match value {
    'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => {
      'A'
    }
    'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'E',
    'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
    'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
    'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
    'ç' | 'Ç' => 'C',
    'ñ' | 'Ñ' => 'N',
    'ý' | 'ÿ' | 'Ý' => 'Y',
    other => other,
  }
}

/// Bacen tags 59/60 accept only `A-Z 0-9` and spaces. Transliterates
/// accents to ASCII, uppercases, drops anything else, then truncates.
fn sanitize_merchant_field(value: &str, max_length: usize) -> String {
  value
    .chars()
    .map(transliterate_to_ascii)
    .map(|c| c.to_ascii_uppercase())
    .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
    .take(max_length)
    .collect::<String>()
    .trim()
    .to_string()
}

fn is_uuid_format(value: &str) -> bool {
  if value.len() != 36 {
    return false;
  }
  for (i, c) in value.chars().enumerate() {
    if i == 8 || i == 13 || i == 18 || i == 23 {
      if c != '-' {
        return false;
      }
    } else if !c.is_ascii_hexdigit() {
      return false;
    }
  }
  true
}

/// Validates a raw PIX key for its type and returns the normalized form
/// used in the BR Code: digits-only for CPF/CNPJ, E.164 (`+55…`) for
/// phones, lowercased/trimmed for email and random (EVP) keys.
pub fn normalize_pix_key(
  key_type: &str,
  raw: &str,
) -> Result<String, &'static str> {
  match key_type.trim() {
    "cpf" => {
      let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
      if digits.len() != 11 {
        return Err("Chave PIX inválida: CPF deve ter 11 dígitos.");
      }
      Ok(digits)
    }
    "cnpj" => {
      let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
      if digits.len() != 14 {
        return Err("Chave PIX inválida: CNPJ deve ter 14 dígitos.");
      }
      Ok(digits)
    }
    "phone" => {
      let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
      let national = if digits.len() == 10 || digits.len() == 11 {
        format!("55{digits}")
      } else if (digits.len() == 12 || digits.len() == 13)
        && digits.starts_with("55")
      {
        digits
      } else {
        return Err(
          "Chave PIX inválida: telefone deve ter DDD + número (10 ou 11 dígitos).",
        );
      };
      Ok(format!("+{national}"))
    }
    "email" => {
      let key = raw.trim().to_lowercase();
      let mut parts = key.split('@');
      let valid = match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
          !local.is_empty()
            && !domain.is_empty()
            && !key.contains(' ')
            && !key.contains('\t')
        }
        _ => false,
      };
      if !valid {
        return Err("Chave PIX inválida: informe um e-mail válido.");
      }
      Ok(key)
    }
    "random" => {
      let key = raw.trim().to_lowercase();
      if !is_uuid_format(&key) {
        return Err(
          "Chave PIX inválida: chave aleatória deve ser um UUID válido.",
        );
      }
      Ok(key)
    }
    _ => Err("Tipo de chave PIX inválido."),
  }
}

/// Builds the BR Code copy-paste string for the order. Returns the
/// validation error when the configured key is invalid.
pub fn generate_pix_payload(
  pix_key: &str,
  pix_key_type: &str,
  merchant_name: &str,
  merchant_city: &str,
  amount_cents: i64,
  order_id: &str,
) -> Result<String, &'static str> {
  let key = normalize_pix_key(pix_key_type, pix_key)?;
  let merchant_name = sanitize_merchant_field(merchant_name, 25);
  let merchant_city = sanitize_merchant_field(merchant_city, 15);
  let txid = order_id
    .replace('-', "")
    .chars()
    .take(25)
    .collect::<String>();
  let amount = format!("{:.2}", amount_cents as f64 / 100.0);

  let merchant_account =
    emv_field("00", "br.gov.bcb.pix") + &emv_field("01", key.as_str());
  let additional_data = emv_field("05", &txid);

  let payload_without_crc = emv_field("00", "01")
    + &emv_field("26", &merchant_account)
    + &emv_field("52", "0000")
    + &emv_field("53", "986")
    + &emv_field("54", &amount)
    + &emv_field("58", "BR")
    + &emv_field("59", &merchant_name)
    + &emv_field("60", &merchant_city)
    + &emv_field("62", &additional_data)
    + "6304";

  Ok(format!(
    "{}{}",
    payload_without_crc,
    crc16_ccitt_false(&payload_without_crc)
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn crc16_matches_well_known_vector() {
    assert_eq!(crc16_ccitt_false("123456789"), "29B1");
  }

  fn payload_for(key_type: &str, key: &str) -> String {
    generate_pix_payload(
      key,
      key_type,
      "Linda Flor",
      "Aracaju",
      10_000,
      "ordem123",
    )
    .expect("valid test key")
  }

  #[test]
  fn golden_vector_phone_key() {
    assert_eq!(
      payload_for("phone", "79999816511"),
      "00020126360014br.gov.bcb.pix0114+55799998165115204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363048CDC"
    );
  }

  #[test]
  fn golden_vector_cpf_key() {
    assert_eq!(
      payload_for("cpf", "12345678901"),
      "00020126330014br.gov.bcb.pix0111123456789015204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem1236304F9F4"
    );
  }

  #[test]
  fn golden_vector_cnpj_key() {
    assert_eq!(
      payload_for("cnpj", "12345678000195"),
      "00020126360014br.gov.bcb.pix0114123456780001955204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363045855"
    );
  }

  #[test]
  fn golden_vector_email_key() {
    assert_eq!(
      payload_for("email", "Loja@Exemplo.com"),
      "00020126380014br.gov.bcb.pix0116loja@exemplo.com5204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363047DA5"
    );
  }

  #[test]
  fn golden_vector_random_key() {
    assert_eq!(
      payload_for("random", "A1B2C3D4-E5F6-7890-ABCD-EF1234567890"),
      "00020126580014br.gov.bcb.pix0136a1b2c3d4-e5f6-7890-abcd-ef12345678905204000053039865406100.005802BR5910LINDA FLOR6007ARACAJU62120508ordem12363048700"
    );
  }

  #[test]
  fn phone_normalization_cases() {
    assert_eq!(
      normalize_pix_key("phone", "(79) 99981-6511"),
      Ok("+5579999816511".to_string())
    );
    assert_eq!(
      normalize_pix_key("phone", "79999816511"),
      Ok("+5579999816511".to_string())
    );
    assert_eq!(
      normalize_pix_key("phone", "+5579999816511"),
      Ok("+5579999816511".to_string())
    );
    assert_eq!(
      normalize_pix_key("phone", "5579999816511"),
      Ok("+5579999816511".to_string())
    );
    assert_eq!(
      normalize_pix_key("phone", "7930211234"),
      Ok("+557930211234".to_string())
    );
    assert!(normalize_pix_key("phone", "123").is_err());
    assert!(normalize_pix_key("phone", "123456789012345").is_err());
    assert!(normalize_pix_key("phone", "").is_err());
  }

  #[test]
  fn key_validation_rejects_bad_input() {
    assert!(normalize_pix_key("cpf", "123").is_err());
    assert!(normalize_pix_key("cpf", "123.456.789-01").is_ok());
    assert!(normalize_pix_key("cnpj", "123").is_err());
    assert!(normalize_pix_key("email", "sem-arroba").is_err());
    assert!(normalize_pix_key("email", "  LOJA@EXEMPLO.COM ").is_ok());
    assert_eq!(
      normalize_pix_key("email", "  LOJA@EXEMPLO.COM "),
      Ok("loja@exemplo.com".to_string())
    );
    assert!(normalize_pix_key("random", "nao-um-uuid").is_err());
    assert!(normalize_pix_key("other", "abc").is_err());
  }

  #[test]
  fn merchant_fields_transliterate_accents() {
    assert_eq!(
      sanitize_merchant_field("São João do Café", 25),
      "SAO JOAO DO CAFE"
    );
    assert_eq!(sanitize_merchant_field("Aracajú", 15), "ARACAJU");
    assert_eq!(sanitize_merchant_field("Lindä Flôr Ü", 25), "LINDA FLOR U");
  }

  #[test]
  fn merchant_fields_truncate_and_strip_symbols() {
    assert_eq!(
      sanitize_merchant_field("Linda Flor Moda Praia e Acessórios", 25),
      "LINDA FLOR MODA PRAIA E A"
    );
    assert_eq!(
      sanitize_merchant_field("Santa Luzia do Itanhy!", 15),
      "SANTA LUZIA DO"
    );
    assert_eq!(
      sanitize_merchant_field("Loja-123 & Cia.", 25),
      "LOJA123  CIA"
    );
  }

  #[test]
  fn payload_rejects_invalid_key() {
    assert!(
      generate_pix_payload(
        "",
        "phone",
        "Linda Flor",
        "Aracaju",
        10_000,
        "ordem123",
      )
      .is_err()
    );
  }
}
