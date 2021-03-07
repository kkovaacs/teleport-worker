# teleport-worker

A toy _worker_ service written for the Teleport backend challenge.

## Building the project

The project is written in Rust, using Rust 1.50. The code can be built using `cargo`.

Building:
```shell
$ cargo build
```

Running tests:
```shell
$ cargo test
```

A GitHub actions workflow is also present to make sure that tests are run and formatting is checked for pull requests (and pushes to master).

## Starting the service

The prototype service executable uses a hardwired TLS configuration with the certificates and keys in the `data/pki` directory. This means that it is not expecting any arguments.

```shell
$ cargo run --bin service
```

Setting the log level of the service is done via the RUST_LOG enviroment variable. For more details see the [env_logger documentation](https://docs.rs/env_logger/0.8.3/env_logger/).

## Starting the client

The client expects command line options that specify PKI related options. PEM files containing the identity (certificate and private key) to authenticate with should be specified with the `--cert` and `--key` options. The private key is expected to be in PKCS#8 format. A CA certificate used as a trust root when validating the service certificate should be specified with `--ca-certificate`.

Successful operations return an exit status of zero. In case of an error a non-zero exit status is used and the error is printed to stderr.

Subcommands are used to map command lines to service operations.

To submit a new job the `start` subcommand can be used, which then prints the ID of the job to stdout:

```shell
$ cargo run --bin client -- --cert data/pki/user1-cert.pem --key data/pki/user1-key-pkcs8.pem --ca-certificate data/pki/server-ca-cert.pem start /bin/ls /
7c51699e-b9bc-439a-b847-0063626b6ea4
```

This ID can then be used to refer to the job in subsequent invocations. For example, to query the status of the job:

```shell
$ cargo run --bin client -- --cert data/pki/user1-cert.pem --key data/pki/user1-key-pkcs8.pem --ca-certificate data/pki/server-ca-cert.pem query-status 7c51699e-b9bc-439a-b847-0063626b6ea4
```

To remove a job from the service:

```shell
$ cargo run --bin client -- --cert data/pki/user1-cert.pem --key data/pki/user1-key-pkcs8.pem --ca-certificate data/pki/server-ca-cert.pem stop 7c51699e-b9bc-439a-b847-0063626b6ea4
```

## License

This project is licensed under the MIT license.
