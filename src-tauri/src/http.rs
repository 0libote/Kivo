pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    // Share the updater's Ring backend instead of also compiling AWS-LC.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn https_clients_initialize_without_the_updater() {
        assert!(super::client_builder().build().is_ok());
        assert!(super::client_builder().build().is_ok());
    }
}
