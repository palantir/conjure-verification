FROM debian:stretch-slim

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends dumb-init; \
    rm -rf /var/lib/apt/lists/*; \
    mkdir -p /opt/palantir/services/conjure-verification-server

ADD conjure-verification-server /opt/palantir/services/conjure-verification-server/

RUN chmod +x /opt/palantir/services/conjure-verification-server/conjure-verification-server

EXPOSE 8000

WORKDIR /opt/palantir/services/conjure-verification-server
ENTRYPOINT ["dumb-init", "--"]
CMD /opt/palantir/services/conjure-verification-server/conjure-verification-server
