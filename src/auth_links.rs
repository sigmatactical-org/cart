//! Public sign-in and registration links to the identity BFF, used to gate the
//! "pay deposit to reserve" action on the public cart.

/// Resolved auth URLs for the cart navbar and reserve form.
pub struct AuthLinks {
    pub sign_in_url: String,
    pub logout_url: String,
    pub identity_base_url: String,
    pub contact_us_url: String,
    pub edit_profile_url: String,
    pub store_url: String,
}

/// Build login and registration URLs that return the shopper to `return_path` on the cart.
#[must_use]
pub fn auth_links_for_return_path(return_path: &str) -> AuthLinks {
    let identity_base = crate::config::identity_public_base_url();
    let cart_base = crate::config::public_base_url();
    let app_uri = join_url(&cart_base, return_path);
    let identity_root = identity_base.trim_end_matches('/');
    let callback_uri = format!("{identity_root}/auth/callback");
    let logout_callback_uri = format!("{identity_root}/auth/logoutcallback");

    let sign_in_url = format!(
        "{identity_root}/auth/login?app_uri={app_uri}&redirect_uri={callback_uri}&scope=openid",
        app_uri = percent_encode(&app_uri),
        callback_uri = percent_encode(&callback_uri),
    );
    let logout_url = format!(
        "{identity_root}/auth/logout?app_uri={app_uri}&redirect_uri={logout_callback_uri}",
        app_uri = percent_encode(&app_uri),
        logout_callback_uri = percent_encode(&logout_callback_uri),
    );
    let contact_base = crate::config::contact_public_base_url();
    let contact_us_url = format!(
        "{contact_base}contact?return_url={}",
        percent_encode(&app_uri)
    );
    let edit_profile_url = format!(
        "{identity_root}/profile?return_url={}",
        percent_encode(&app_uri)
    );

    AuthLinks {
        sign_in_url,
        logout_url,
        identity_base_url: format!("{identity_root}/"),
        contact_us_url,
        edit_profile_url,
        store_url: crate::config::store_public_base_url(),
    }
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if path == "/" {
        return format!("{base}/");
    }
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b'/' => out.push_str("%2F"),
            b':' => out.push_str("%3A"),
            b'?' => out.push_str("%3F"),
            b'#' => out.push_str("%23"),
            b'[' => out.push_str("%5B"),
            b']' => out.push_str("%5D"),
            b'@' => out.push_str("%40"),
            b'!' => out.push_str("%21"),
            b'$' => out.push_str("%24"),
            b'&' => out.push_str("%26"),
            b'\'' => out.push_str("%27"),
            b'(' => out.push_str("%28"),
            b')' => out.push_str("%29"),
            b'*' => out.push_str("%2A"),
            b'+' => out.push_str("%2B"),
            b',' => out.push_str("%2C"),
            b';' => out.push_str("%3B"),
            b'=' => out.push_str("%3D"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_login_with_identity_callback() {
        let links = auth_links_for_return_path("/");
        assert!(links.sign_in_url.contains("/auth/login?app_uri="));
        assert!(links.sign_in_url.contains("redirect_uri="));
        assert!(links.contact_us_url.contains("/contact?return_url="));
        assert!(links.edit_profile_url.contains("/profile?return_url="));
    }
}
