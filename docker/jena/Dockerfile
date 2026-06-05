# Apache Jena 5.x CLI (riot + sparql) — GMEOW's RDF 1.2 / RDF* engine.
#
# RDF 1.2 is a REQUIRED, verified GMEOW output (CONSTITUTION Principles 2-3, 7), and
# Apache Jena >= 5.2 is the only mainstream engine that materializes and re-reads the
# triple-term form. No maintained public Jena 5.4 CLI image exists (the `stain/jena`
# tags stop at 5.1.0), so the pinned image (config.JENA_IMAGE) is built from the
# Apache distribution here — `scripts/pull-images.sh` builds it when a pull fails, and
# CI builds it for the statement-metadata gate. The build is reproducible and
# integrity-checked: it pins the Jena version and verifies the downloaded tarball
# against the official Apache SHA-512 checksum before unpacking.
FROM eclipse-temurin:21-jre-jammy

ARG JENA_VERSION=5.4.0
# Official Apache SHA-512 of apache-jena-${JENA_VERSION}.tar.gz — from
# https://archive.apache.org/dist/jena/binaries/apache-jena-5.4.0.tar.gz.sha512
# (update both this and JENA_VERSION together on a Jena bump).
ARG JENA_SHA512=0d1d13dd8bb098b28996ffd9871f3a42f54984ac276c10dab0c0d2ccd8c66d69737f20bc3fc05f1ee3a2ceeacba318ef5f1a1f4fd2cba19041210675fac245ed
# curl + ca-certificates are not guaranteed in the minimal JRE base image; install
# them explicitly so the (HTTPS) download is portable across base-image revisions.
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends curl ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    curl -fSL "https://archive.apache.org/dist/jena/binaries/apache-jena-${JENA_VERSION}.tar.gz" \
        -o /tmp/jena.tar.gz; \
    echo "${JENA_SHA512}  /tmp/jena.tar.gz" | sha512sum -c -; \
    mkdir -p /opt; \
    tar -xzf /tmp/jena.tar.gz -C /opt; \
    rm /tmp/jena.tar.gz; \
    ln -s "/opt/apache-jena-${JENA_VERSION}" /opt/jena

ENV JENA_HOME=/opt/jena
ENV PATH=/opt/jena/bin:$PATH
WORKDIR /work
