//! Same-site redirect target validation.
//!
//! Every user-controlled redirect target (`next`, `redirect`,
//! `callback_url`, same-origin `Referer` paths, admin banner links) goes
//! through [`safe_local_redirect`] so the rules live in one place.

/// Return `raw` (trimmed) when it is a same-site absolute path, else `None`.
///
/// Browsers treat `\` as `/` in http(s) URLs and silently drop TAB/CR/LF,
/// so `/\evil.com` and `/\t/evil.com` both resolve to `//evil.com` (a
/// protocol-relative URL to another host). Rules:
/// - must start with a single `/` (the second byte is neither `/` nor `\`);
/// - no whitespace in the value as given;
/// - no `\` or control characters, checked on the value as given *and*
///   after one round of percent-decoding (so `%5C`, `%09` and friends are
///   rejected too, while `%20` in a query string is fine);
/// - no scheme or authority (implied by the leading `/` rule, which leaves
///   no room for `scheme:` and keeps the host empty).
pub fn safe_local_redirect(raw: &str) -> Option<String> {
  let path = raw.trim();
  if !is_local_path(path) || path.chars().any(char::is_whitespace) {
    return None;
  }
  // Encoded spaces (`%20`) are legitimate in query strings, so only the
  // dangerous set (`\`, controls, a leading `//`) is re-checked here.
  let decoded = percent_decode(path)?;
  if !is_local_path(&decoded) {
    return None;
  }
  Some(path.to_owned())
}

fn is_local_path(path: &str) -> bool {
  let bytes = path.as_bytes();
  bytes.first() == Some(&b'/')
    && !matches!(bytes.get(1), Some(b'/' | b'\\'))
    && !path.chars().any(|c| c == '\\' || c.is_control())
}

/// One round of `%XX` decoding. Malformed escapes and decoded bytes that
/// are not UTF-8 yield `None` (the caller rejects the target).
fn percent_decode(value: &str) -> Option<String> {
  let bytes = value.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' {
      let hex = bytes.get(i + 1..i + 3)?;
      let hex = std::str::from_utf8(hex).ok()?;
      out.push(u8::from_str_radix(hex, 16).ok()?);
      i += 3;
    } else {
      out.push(bytes[i]);
      i += 1;
    }
  }
  String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
  use super::safe_local_redirect;

  #[test]
  fn accepts_same_site_paths() {
    for (raw, expected) in [
      ("/", "/"),
      ("/conta", "/conta"),
      ("  /conta  ", "/conta"),
      (
        "/produtos?q=biquini&cor=azul",
        "/produtos?q=biquini&cor=azul",
      ),
      ("/produtos?q=biquini%20azul", "/produtos?q=biquini%20azul"),
      ("/favoritos#topo", "/favoritos#topo"),
      ("/pedido/abc?token=x", "/pedido/abc?token=x"),
    ] {
      assert_eq!(
        safe_local_redirect(raw).as_deref(),
        Some(expected),
        "expected {raw:?} to be accepted"
      );
    }
  }

  #[test]
  fn rejects_off_site_and_ambiguous_targets() {
    for raw in [
      "",
      "   ",
      "conta",
      "//evil.com",
      "///evil.com",
      "/\\evil.com",
      "/\\/evil.com",
      "\\\\evil.com",
      "/\tevil.com",
      "/\t/evil.com",
      "/\n/evil.com",
      "/\r/evil.com",
      "/ /evil.com",
      "/con\u{0}ta",
      "/conta\\x",
      "%2F%2Fevil.com",
      "/%2Fevil.com",
      "/%5Cevil.com",
      "/%09/evil.com",
      "/%0d%0aSet-Cookie:x",
      "/%zz",
      "/%ff",
      "https://evil.com",
      "http://evil.com/conta",
      "javascript:alert(1)",
      "data:text/html,x",
      "\u{a0}//evil.com",
    ] {
      assert_eq!(
        safe_local_redirect(raw),
        None,
        "expected {raw:?} to be rejected"
      );
    }
  }
}
