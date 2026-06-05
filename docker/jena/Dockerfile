# Apache Jena 5.x CLI (riot + sparql) — GMEOW's RDF 1.2 / RDF* engine.
#
# RDF 1.2 is a REQUIRED, verified GMEOW output (CONSTITUTION Principles 2-3, 7), and
# Apache Jena >= 5.2 is the only mainstream engine that materializes and re-reads the
# triple-term form. No maintained public Jena 5.4 CLI image exists (the `stain/jena`
# tags stop at 5.1.0), so the pinned image (config.JENA_IMAGE) is built from the
# Apache distribution here — `scripts/pull-images.sh` builds it when a pull fails, and
# CI builds it for the statement-metadata gate. The build is reproducible: it pins the
# Jena version and fetches the signed Apache release.
FROM eclipse-temurin:21-jre-jammy

ARG JENA_VERSION=5.4.0
RUN set -eux; \
    curl -fSL "https://archive.apache.org/dist/jena/binaries/apache-jena-${JENA_VERSION}.tar.gz" \
        -o /tmp/jena.tar.gz; \
    mkdir -p /opt; \
    tar -xzf /tmp/jena.tar.gz -C /opt; \
    rm /tmp/jena.tar.gz; \
    ln -s "/opt/apache-jena-${JENA_VERSION}" /opt/jena

ENV JENA_HOME=/opt/jena
ENV PATH=/opt/jena/bin:$PATH
WORKDIR /work
