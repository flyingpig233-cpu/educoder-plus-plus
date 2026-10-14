// Protocol adapted from alumkal/educoder-helper (MIT); see THIRD_PARTY_NOTICES.md.
use base64::{Engine, engine::general_purpose::STANDARD};

pub(crate) fn signature(method: &str, time_ms: i64) -> String {
    // Public frontend protocol constants, not user credentials.
    let raw = format!(
        "method={}&ak=e9dd5b4322f9f7d83d009de9bfa100c3&sk=2e3da06ae26ba9f76a5d8d355746f2fe&time={time_ms}",
        method.to_ascii_uppercase()
    );
    format!("{:x}", md5::compute(STANDARD.encode(raw)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_independently_computed_protocol_vector() {
        // Expected digest calculated independently with .NET MD5.
        assert_eq!(
            signature("get", 1_700_000_000_000),
            "e279c864e2752591aa53c3989b9fdbeb"
        );
    }
}
