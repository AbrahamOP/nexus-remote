use std::ffi::CStr;

#[test]
fn links_open_ssl_3_5_9_lts() {
    let version =
        unsafe { CStr::from_ptr(openssl_sys::OpenSSL_version(openssl_sys::OPENSSL_VERSION)) }
            .to_str()
            .expect("OpenSSL version should be UTF-8");
    assert!(
        version.starts_with("OpenSSL 3.5.9 "),
        "the linked OpenSSL must be the verified 3.5.9 LTS build, got {version}"
    );
}
