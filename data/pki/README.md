# PKI hierarchy

This directory contains the private keys and certificates for use by the prototype worker. These are definitely not intended for any serious use, and committing private keys is generally an awful idea. Make sure to generate your own set of keys and certificates and _do not re-use these keys for anything serious_.

The following commands were used to generate these files:

```shell
# generate user CA
openssl ecparam -name prime256v1 -genkey -noout -out user-ca-key.pem
openssl req -new -x509 -key user-ca-key.pem -out user-ca-cert.pem -days 1460

# generate key & certificate for user1
openssl ecparam -name prime256v1 -genkey -noout -out user1-key.pem
openssl pkcs8 -topk8 -nocrypt -in user1-key.pem -out user1-key-pkcs8.pem
openssl req -new -key user1-key.pem -subj '/CN=user1' -addext 'subjectAltName = email:user1@company.com' -out user1-csr.pem
openssl x509 -req -in user1-csr.pem -extfile user1-cert.cnf -CA user-ca-cert.pem -CAkey user-ca-key.pem -CAcreateserial -days 365 -out user1-cert.pem
# repeat for other users...

# generate server CA
openssl ecparam -name prime256v1 -genkey -noout -out server-ca-key.pem
openssl req -new -x509 -key server-ca-key.pem -out server-ca-cert.pem -days 1460

# generate certificate for service
openssl ecparam -name prime256v1 -genkey -noout -out server-key.pem
openssl pkcs8 -topk8 -nocrypt -in server-key.pem -out server-key-pkcs8.pem
openssl req -new -key server-key.pem -subj '/CN=Worker Service' -addext 'subjectAltName = DNS:localhost' -out server-csr.pem
openssl x509 -req -in server-csr.pem -extfile server-cert.cnf -CA server-ca-cert.pem -CAkey server-ca-key.pem -CAcreateserial -days 365 -out server-cert.pem
```
