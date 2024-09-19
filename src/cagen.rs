use rcgen::Certificate;

use std::{fs, path::Path};

use crate::proxy::CertificateAuthority;

pub fn gen_ca<T: AsRef<Path>>(ca: T, key: T) -> Certificate {
    let cert = CertificateAuthority::gen_ca().expect("generate cert");
    let cert_crt = cert.serialize_pem().unwrap();

    fs::create_dir("ca").unwrap();

    println!("{}", cert_crt);
    if let Err(err) = fs::write(ca, cert_crt) {
        tracing::error!("cert file write failed: {}", err);
    }

    let private_key = cert.serialize_private_key_pem();
    println!("{}", private_key);
    if let Err(err) = fs::write(key, private_key) {
        tracing::error!("private key file write failed: {}", err);
    }

    cert
}
