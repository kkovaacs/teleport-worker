use openssl::x509::X509;
use tonic::Request;

pub type Identity = String;

pub fn get_username_from_request<T>(request: &Request<T>) -> Option<Identity> {
    // The situation with tonic is less than ideal:
    // - it uses rustls and webpki to validate peer certificates with a trust root CA
    // - but then provides no structured access to the contents of the peer certificate
    //
    // Because of this we have to parse the DER-encoded certificate with openssl to get access
    // to the SubjectAltName extensions -- we use the email address as the identity for our
    // prototype worker service.
    match request.peer_certs() {
        Some(certs) => match certs.get(0) {
            Some(cert) => get_username_from_certificate(cert.get_ref()),
            None => None,
        },
        None => None,
    }
}

fn get_username_from_certificate(der_encoded_certificate: &[u8]) -> Option<Identity> {
    if let Ok(cert) = X509::from_der(der_encoded_certificate) {
        match &cert.subject_alt_names() {
            Some(san) => san
                .iter()
                .find_map(|name| name.email())
                .map(|s| s.to_owned()),
            None => None,
        }
    } else {
        None
    }
}

#[test]
fn test_get_username_from_certificate() {
    let user1_cert = include_bytes!("../../../data/pki/user1-cert.pem");
    let x509 = X509::from_pem(user1_cert).unwrap();
    let username = get_username_from_certificate(&x509.to_der().unwrap());
    assert_eq!(username, Some("user1@company.com".to_owned()));
}
