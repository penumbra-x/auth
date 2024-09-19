use std::sync::Arc;

use crate::proxy::{CertificateAuthority, Proxy};
use crate::{cagen, BootArgs};
use anyhow::{Context, Result};
use tokio::fs;

pub struct Serve(BootArgs);

impl Serve {
    pub fn new(args: BootArgs) -> Self {
        Self(args)
    }

    #[tokio::main]
    pub async fn run(self) -> Result<()> {
        // Generate a certificate authority
        if !self.0.cert.exists() || !self.0.key.exists() {
            cagen::gen_ca(&self.0.cert, &self.0.key);
        }

        // Read the private key and certificate
        tracing::info!("CA Private key use: {}", self.0.key.display());
        let private_key_bytes = fs::read(self.0.key)
            .await
            .context("ca private key file path not valid!")?;
        let private_key = rustls_pemfile::pkcs8_private_keys(&mut private_key_bytes.as_slice())
            .context("Failed to parse private key")?;
        let key = rustls::PrivateKey(private_key[0].clone());

        // Read the certificate
        tracing::info!("CA Certificate use: {}", self.0.cert.display());
        let ca_cert_bytes = fs::read(self.0.cert)
            .await
            .context("ca cert file path not valid!")?;
        let ca_cert = rustls_pemfile::certs(&mut ca_cert_bytes.as_slice())
            .context("Failed to parse CA certificate")?;
        let cert = rustls::Certificate(ca_cert[0].clone());

        // Gnerate a certificate authority
        let ca = CertificateAuthority::new(
            key,
            cert,
            String::from_utf8(ca_cert_bytes).context("Failed to parse CA certificate")?,
            1_000,
        )
        .context("Failed to create Certificate Authority")?;

        tracing::info!("Http MITM Proxy listen on: http://{}", self.0.bind);

        // Start the server
        Proxy::builder()
            .ca(Arc::new(ca))
            .listen_addr(self.0.bind)
            .proxy(self.0.proxy)
            .build()
            .start(shutdown_signal())
            .await
            .map_err(Into::into)
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
}
