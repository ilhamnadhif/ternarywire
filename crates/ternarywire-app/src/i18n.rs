//! English-only UI string helpers.
//!
//! Call sites keep `tr` / `trf` as lightweight passthroughs so user-facing
//! strings stay consistent without a runtime translation table.

/// Returns the English UI string as-is.
pub fn tr(s: &str) -> &str {
    s
}

/// Fills `{}` placeholders in order:
/// `trf("Connected to {} (sid {})", &[&ns, &sid])`.
pub fn trf(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut parts = tr(template).split("{}").peekable();
    while let Some(part) = parts.next() {
        out.push_str(part);
        if parts.peek().is_some()
            && let Some(a) = args.next()
        {
            out.push_str(&a.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_fill_in_order() {
        assert_eq!(trf("{} of {}", &[&1, &3]), "1 of 3");
    }
}
