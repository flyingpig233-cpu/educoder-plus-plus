use std::{
    fmt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::{Url, header::HeaderValue};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct Cookie {
    domain: String,
    include_subdomains: bool,
    path: String,
    secure: bool,
    expires: u64,
    name: String,
    value: String,
}

/// A scoped browser session. Debug output is always redacted.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    cookies: Vec<Cookie>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Session([REDACTED])")
    }
}

impl Session {
    /// Opaque session namespace, suitable for isolating cached data and drafts.
    pub fn cache_namespace(&self) -> Result<String> {
        let (_, auth) =
            self.headers_for(&Url::parse("https://data.educoder.net/api/").expect("constant URL"))?;
        Ok(crate::cache::digest(auth.as_bytes()))
    }
    /// Import Netscape cookies.txt, including #HttpOnly_ entries.
    /// Unrelated domains and expired cookies are ignored.
    pub fn from_netscape(text: &str) -> Result<Self> {
        let now = unix_seconds();
        let mut cookies = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<_> = line.split('\t').collect();
            if parts.len() != 7 {
                return Err(Error::InvalidCookie { line: index + 1 });
            }
            let domain = parts[0].trim_start_matches('.').to_ascii_lowercase();
            if domain != "educoder.net" && !domain.ends_with(".educoder.net") {
                continue;
            }
            let invalid = || Error::InvalidCookie { line: index + 1 };
            let expires = parts[4].parse::<u64>().map_err(|_| invalid())?;
            if expires != 0 && expires <= now {
                continue;
            }
            if !matches!(parts[1], "TRUE" | "FALSE")
                || !matches!(parts[3], "TRUE" | "FALSE")
                || !parts[2].starts_with('/')
            {
                return Err(invalid());
            }
            validate_pair(parts[5], parts[6]).map_err(|_| invalid())?;
            cookies.push(Cookie {
                domain,
                include_subdomains: parts[1] == "TRUE",
                path: parts[2].into(),
                secure: parts[3] == "TRUE",
                expires,
                name: parts[5].into(),
                value: parts[6].into(),
            });
        }
        let session = Self { cookies };
        session
            .headers_for(&Url::parse("https://data.educoder.net/api/").expect("constant URL"))?;
        Ok(session)
    }

    /// Import a Cookie request header from the user's own logged-in browser.
    pub fn from_cookie_header(header: &str) -> Result<Self> {
        let mut cookies = Vec::new();
        for item in header.split(';') {
            let (name, value) = item
                .trim()
                .split_once('=')
                .ok_or(Error::InvalidCookie { line: 1 })?;
            validate_pair(name, value)?;
            cookies.push(Cookie {
                domain: "educoder.net".into(),
                include_subdomains: true,
                path: "/".into(),
                secure: true,
                expires: 0,
                name: name.into(),
                value: value.into(),
            });
        }
        let session = Self { cookies };
        session
            .headers_for(&Url::parse("https://data.educoder.net/api/").expect("constant URL"))?;
        Ok(session)
    }

    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let text =
            zeroize::Zeroizing::new(tokio::fs::read_to_string(path).await.map_err(Error::Io)?);
        Self::from_netscape(&text)
    }

    pub(crate) fn headers_for(&self, url: &Url) -> Result<(HeaderValue, HeaderValue)> {
        let host = url.host_str().ok_or(Error::SessionExpired)?;
        let now = unix_seconds();
        let mut matching: Vec<_> = self
            .cookies
            .iter()
            .filter(|cookie| {
                (host == cookie.domain
                    || (cookie.include_subdomains
                        && host.ends_with(&format!(".{}", cookie.domain))))
                    && (cookie.expires == 0 || cookie.expires > now)
                    && (!cookie.secure || url.scheme() == "https")
                    && (url.path() == cookie.path
                        || (url.path().starts_with(&cookie.path)
                            && (cookie.path.ends_with('/')
                                || url.path().as_bytes().get(cookie.path.len()) == Some(&b'/'))))
            })
            .collect();
        matching.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        let auth = matching
            .iter()
            .find(|c| c.name == "_educoder_session" && !c.value.is_empty())
            .ok_or(Error::SessionExpired)?;
        let mut authorization =
            HeaderValue::from_str(&auth.value).map_err(|_| Error::InvalidCookie { line: 0 })?;
        let text = zeroize::Zeroizing::new(
            matching
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; "),
        );
        let mut cookie =
            HeaderValue::from_str(&text).map_err(|_| Error::InvalidCookie { line: 0 })?;
        cookie.set_sensitive(true);
        authorization.set_sensitive(true);
        Ok((cookie, authorization))
    }

    pub(crate) fn encode(&self) -> Result<zeroize::Zeroizing<String>> {
        serde_json::to_string(self)
            .map(zeroize::Zeroizing::new)
            .map_err(|_| Error::Vault)
    }

    pub(crate) fn decode(text: &str) -> Result<Self> {
        let session: Self = serde_json::from_str(text).map_err(|_| Error::Vault)?;
        for cookie in &session.cookies {
            validate_pair(&cookie.name, &cookie.value)?;
        }
        session
            .headers_for(&Url::parse("https://data.educoder.net/api/").expect("constant URL"))?;
        Ok(session)
    }
}

fn validate_pair(name: &str, value: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
        || !value
            .bytes()
            .all(|c| (0x21..=0x7e).contains(&c) && !b";\"\\,".contains(&c))
    {
        return Err(Error::InvalidCookie { line: 0 });
    }
    Ok(())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_scopes_expiry_and_http_only() {
        let text = "# Netscape HTTP Cookie File\n#HttpOnly_.educoder.net\tTRUE\t/\tTRUE\t0\t_educoder_session\tfake\n.evil.net\tTRUE\t/\tTRUE\t0\tleak\tbad\n.educoder.net\tTRUE\t/\tTRUE\t1\told\tbad\n.educoder.net\tTRUE\t/api/private\tTRUE\t0\tprivate\tsecret";
        let session = Session::from_netscape(text).unwrap();
        let (cookie, auth) = session
            .headers_for(&Url::parse("https://www.educoder.net/api/tasks/a.json").unwrap())
            .unwrap();
        assert_eq!(cookie, "_educoder_session=fake");
        assert_eq!(auth, "fake");
        assert!(
            session
                .headers_for(&Url::parse("https://evil.net/api/").unwrap())
                .is_err()
        );
        assert!(!format!("{session:?}").contains("fake"));
    }

    #[test]
    fn rejects_injection_missing_and_expired_session() {
        for header in [
            "x=y",
            "_educoder_session=",
            "_educoder_session=fake\r\nX-Evil: 1",
        ] {
            assert!(Session::from_cookie_header(header).is_err());
        }
        assert!(
            Session::from_netscape(".educoder.net\tTRUE\t/\tTRUE\t1\t_educoder_session\tfake")
                .is_err()
        );
    }

    #[test]
    fn vault_round_trip() {
        let session = Session::from_cookie_header("_educoder_session=fake; other=a=b").unwrap();
        let restored = Session::decode(&session.encode().unwrap()).unwrap();
        let url = Url::parse("https://www.educoder.net/api/").unwrap();
        assert_eq!(
            session.headers_for(&url).unwrap(),
            restored.headers_for(&url).unwrap()
        );
    }

    #[test]
    fn api_host_cookies_are_accepted_without_forwarding_website_cookies() {
        let session = Session::from_netscape("data.educoder.net\tFALSE\t/\tTRUE\t0\t_educoder_session\tapi-session\nwww.educoder.net\tFALSE\t/\tTRUE\t0\twebsite-only\tprivate").unwrap();
        let (cookie, auth) = session
            .headers_for(&Url::parse("https://data.educoder.net/api/tasks/g1.json").unwrap())
            .unwrap();
        assert_eq!(cookie, "_educoder_session=api-session");
        assert_eq!(auth, "api-session");
        assert!(
            Session::from_netscape(
                "www.educoder.net\tFALSE\t/\tTRUE\t0\t_educoder_session\twebsite-session"
            )
            .is_err()
        );
    }
}
