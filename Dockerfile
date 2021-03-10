FROM rust:slim AS builder

RUN apt-get update && apt-get -y install libssl-dev pkg-config protobuf-compiler
RUN rustup component add rustfmt

COPY . /source/
WORKDIR /source
RUN cargo build


FROM debian:latest

COPY --from=builder /source/target/debug/service /usr/local/bin
RUN apt-get update && apt-get -y install libssl1.1

EXPOSE 10000

CMD ["/usr/local/bin/service"]
