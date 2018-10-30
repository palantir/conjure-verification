FROM debian:stretch-slim

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends dumb-init; \
    rm -rf /var/lib/apt/lists/*; \
    mkdir -p /opt/palantir/services/conjure-verification-server/service/bin; \
    mkdir -p /opt/palantir/services/conjure-verification-server/var/conf

ADD conjure-verification-server /opt/palantir/services/conjure-verification-server/service/bin
ADD test-cases.json /opt/palantir/services/conjure-verification-server/var/conf
ADD verification-server-api.conjure.json /opt/palantir/services/conjure-verification-server/var/conf

RUN chmod +x /opt/palantir/services/conjure-verification-server/service/bin/conjure-verification-server

EXPOSE 8000

ENV RUST_LOG conjure_verification_server=info,conjure_verification_http=info

WORKDIR /opt/palantir/services/conjure-verification-server

ENTRYPOINT ["dumb-init", "--", "service/bin/conjure-verification-server"]
CMD ["var/conf/test-cases.json", "var/conf/verification-server-api.conjure.json"]
