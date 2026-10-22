//! Shipping rates derived from the destination CEP.
//!
//! The UF is always taken from the CEP (Correios ranges), never from the
//! posted `state` field: the rate depends on it, so trusting the client would
//! let anyone pick the cheapest tier.

pub const BRAZILIAN_STATES: [&str; 27] = [
  "AC", "AL", "AP", "AM", "BA", "CE", "DF", "ES", "GO", "MA", "MT", "MS", "MG",
  "PA", "PB", "PR", "PE", "PI", "RJ", "RN", "RS", "RO", "RR", "SC", "SP", "SE",
  "TO",
];

/// Correios CEP ranges by UF, as 5-digit prefixes (inclusive). Some UFs
/// own more than one range (AM, DF, GO, RO).
const CEP_RANGES: &[(u32, u32, &str)] = &[
  (1_000, 19_999, "SP"),
  (20_000, 28_999, "RJ"),
  (29_000, 29_999, "ES"),
  (30_000, 39_999, "MG"),
  (40_000, 48_999, "BA"),
  (49_000, 49_999, "SE"),
  (50_000, 56_999, "PE"),
  (57_000, 57_999, "AL"),
  (58_000, 58_999, "PB"),
  (59_000, 59_999, "RN"),
  (60_000, 63_999, "CE"),
  (64_000, 64_999, "PI"),
  (65_000, 65_999, "MA"),
  (66_000, 68_899, "PA"),
  (68_900, 68_999, "AP"),
  (69_000, 69_299, "AM"),
  (69_300, 69_399, "RR"),
  (69_400, 69_899, "AM"),
  (69_900, 69_999, "AC"),
  (70_000, 72_799, "DF"),
  (72_800, 72_999, "GO"),
  (73_000, 73_699, "DF"),
  (73_700, 76_799, "GO"),
  (76_800, 76_999, "RO"),
  (77_000, 77_999, "TO"),
  (78_000, 78_899, "MT"),
  // Legacy Rondônia range, kept so old addresses still resolve.
  (78_900, 78_999, "RO"),
  (79_000, 79_999, "MS"),
  (80_000, 87_999, "PR"),
  (88_000, 89_999, "SC"),
  (90_000, 99_999, "RS"),
];

const SHIPPING_LOCAL_CENTS: i64 = 1_990;
const SHIPPING_STANDARD_CENTS: i64 = 3_990;
const SHIPPING_REMOTE_CENTS: i64 = 5_990;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShippingTier {
  /// Same state as the store (Sergipe).
  Local,
  /// Nordeste (outside SE), Sudeste, Sul and Centro-Oeste.
  Standard,
  /// Região Norte.
  Remote,
}

impl ShippingTier {
  pub fn for_uf(uf: &str) -> Self {
    match uf {
      "SE" => Self::Local,
      "AC" | "AM" | "AP" | "PA" | "RO" | "RR" | "TO" => Self::Remote,
      _ => Self::Standard,
    }
  }

  pub fn cents(self) -> i64 {
    match self {
      Self::Local => SHIPPING_LOCAL_CENTS,
      Self::Standard => SHIPPING_STANDARD_CENTS,
      Self::Remote => SHIPPING_REMOTE_CENTS,
    }
  }
}

pub fn is_brazilian_state(uf: &str) -> bool {
  BRAZILIAN_STATES.contains(&uf)
}

pub fn cep_digits(raw: &str) -> String {
  raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// UF for an 8-digit CEP (digits only), or `None` if malformed/unassigned.
pub fn uf_from_cep(digits: &str) -> Option<&'static str> {
  if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  let prefix: u32 = digits[..5].parse().ok()?;
  CEP_RANGES
    .iter()
    .find(|(start, end, _)| (*start..=*end).contains(&prefix))
    .map(|(_, _, uf)| *uf)
}

/// Validates the posted CEP (any formatting) and state and returns the UF
/// derived from the CEP. A posted state that disagrees with the CEP is
/// rejected rather than silently overwritten so the customer can fix a typo.
pub fn resolve_shipping_uf(
  raw_cep: &str,
  posted_state: &str,
) -> Result<&'static str, String> {
  let digits = cep_digits(raw_cep);
  if digits.is_empty() {
    return Err("Informe o CEP.".to_string());
  }
  if digits.len() != 8 {
    return Err("Informe um CEP válido com 8 dígitos.".to_string());
  }
  let Some(uf) = uf_from_cep(&digits) else {
    return Err("CEP não encontrado. Confira o número informado.".to_string());
  };
  let posted = posted_state.trim();
  if !is_brazilian_state(posted) {
    return Err("Selecione um estado válido.".to_string());
  }
  if posted != uf {
    return Err(format!(
      "O CEP informado pertence a {uf}, mas o estado selecionado é {posted}. Confira o estado ou o CEP."
    ));
  }
  Ok(uf)
}

/// Returns `(cents, is_free)` for the given subtotal and destination UF.
pub fn calculate_shipping(
  subtotal_cents: i64,
  uf: &str,
  free_shipping_threshold_cents: i64,
) -> (i64, bool) {
  if subtotal_cents >= free_shipping_threshold_cents {
    return (0, true);
  }
  (ShippingTier::for_uf(uf).cents(), false)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cep_boundaries_map_to_uf() {
    let cases = [
      ("01000000", "SP"),
      ("19999999", "SP"),
      ("20000000", "RJ"),
      ("28999999", "RJ"),
      ("29000000", "ES"),
      ("48999999", "BA"),
      ("49000000", "SE"),
      ("49099999", "SE"),
      ("49999999", "SE"),
      ("50000000", "PE"),
      ("57000000", "AL"),
      ("65999999", "MA"),
      ("66000000", "PA"),
      ("68899999", "PA"),
      ("68900000", "AP"),
      ("69000000", "AM"),
      ("69300000", "RR"),
      ("69399999", "RR"),
      ("69400000", "AM"),
      ("69900000", "AC"),
      ("70000000", "DF"),
      ("72800000", "GO"),
      ("73000000", "DF"),
      ("73700000", "GO"),
      ("76799999", "GO"),
      ("76800000", "RO"),
      ("77000000", "TO"),
      ("78000000", "MT"),
      ("78899999", "MT"),
      ("79000000", "MS"),
      ("79999999", "MS"),
      ("80000000", "PR"),
      ("88000000", "SC"),
      ("90000000", "RS"),
      ("99999999", "RS"),
    ];
    for (cep, uf) in cases {
      assert_eq!(uf_from_cep(cep), Some(uf), "CEP {cep}");
    }
  }

  #[test]
  fn every_state_has_a_cep_range() {
    for uf in BRAZILIAN_STATES {
      assert!(
        CEP_RANGES.iter().any(|(_, _, range_uf)| *range_uf == uf),
        "{uf} missing"
      );
    }
    for (_, _, uf) in CEP_RANGES {
      assert!(is_brazilian_state(uf), "{uf} not a UF");
    }
  }

  #[test]
  fn invalid_ceps_are_rejected() {
    for cep in [
      "",
      "4900000",
      "490000000",
      "4900000a",
      "00000000",
      "00999999",
      "49000-00",
    ] {
      assert_eq!(uf_from_cep(cep), None, "CEP {cep:?}");
    }
  }

  #[test]
  fn resolve_accepts_formatted_matching_cep() {
    assert_eq!(resolve_shipping_uf("49000-000", "SE"), Ok("SE"));
    assert_eq!(resolve_shipping_uf(" 01310-100 ", " SP "), Ok("SP"));
  }

  #[test]
  fn resolve_rejects_bad_input() {
    assert_eq!(
      resolve_shipping_uf("", "SE"),
      Err("Informe o CEP.".to_string())
    );
    assert!(resolve_shipping_uf("4900-000", "SE").is_err());
    assert!(resolve_shipping_uf("00000-000", "SE").is_err());
    assert!(resolve_shipping_uf("49000-000", "XX").is_err());
    assert!(resolve_shipping_uf("49000-000", "").is_err());
  }

  #[test]
  fn mismatched_state_cannot_get_cheaper_rate() {
    // Belém (PA) CEP posted with state=SE to get the local rate.
    let error = resolve_shipping_uf("66010-000", "SE").unwrap_err();
    assert!(error.contains("PA"), "{error}");
    // São Paulo CEP posted with state=SE.
    assert!(resolve_shipping_uf("01310-100", "SE").is_err());
    // The rate charged follows the CEP's UF.
    let uf = uf_from_cep("66010000").unwrap();
    assert_eq!(calculate_shipping(10_000, uf, 29_900), (5_990, false));
  }

  #[test]
  fn tiers_follow_region() {
    assert_eq!(ShippingTier::for_uf("SE"), ShippingTier::Local);
    for uf in ["AC", "AM", "AP", "PA", "RO", "RR", "TO"] {
      assert_eq!(ShippingTier::for_uf(uf), ShippingTier::Remote, "{uf}");
    }
    // MT/MS are Centro-Oeste, not Norte.
    for uf in ["MT", "MS", "GO", "DF", "SP", "RJ", "RS", "BA", "AL", "PE"] {
      assert_eq!(ShippingTier::for_uf(uf), ShippingTier::Standard, "{uf}");
    }
    assert_eq!(calculate_shipping(10_000, "SE", 29_900), (1_990, false));
    assert_eq!(calculate_shipping(10_000, "MT", 29_900), (3_990, false));
    assert_eq!(calculate_shipping(10_000, "AM", 29_900), (5_990, false));
  }

  #[test]
  fn calculate_shipping_honors_threshold() {
    assert_eq!(calculate_shipping(29_900, "SE", 29_900), (0, true));
    assert_eq!(calculate_shipping(1, "AM", 0), (0, true));
    assert_eq!(calculate_shipping(5_000, "SP", 4_999), (0, true));
  }
}
