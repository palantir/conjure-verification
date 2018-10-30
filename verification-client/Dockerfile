FROM rust:1.29.0-stretch

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends dumb-init; \
    rm -rf /var/lib/apt/lists/*; \
    mkdir -p /opt/palantir/services/conjure-verification-client/service/bin; \
    mkdir -p /opt/palantir/services/conjure-verification-client/var/conf

ADD conjure-verification-client /opt/palantir/services/conjure-verification-client/service/bin
ADD test-cases.json /opt/palantir/services/conjure-verification-client/var/conf
ADD verification-client-api.conjure.json /opt/palantir/services/conjure-verification-client/var/conf

RUN chmod +x /opt/palantir/services/conjure-verification-client/service/bin/conjure-verification-client

EXPOSE 8000

ENV RUST_LOG conjure_verification_client=info,conjure_verification_http=info

WORKDIR /opt/palantir/services/conjure-verification-client

ENTRYPOINT ["dumb-init", "--", "service/bin/conjure-verification-client"]
CMD ["var/conf/test-cases.json", "var/conf/verification-client-api.conjure.json"]
