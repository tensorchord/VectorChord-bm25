FROM scratch

ARG SEMVER
ARG PG_VERSION
ARG TARGETARCH

WORKDIR /workspace
COPY ./build/postgresql-${PG_VERSION}-vchord-bm25_${SEMVER}-1_${TARGETARCH}.deb /workspace/