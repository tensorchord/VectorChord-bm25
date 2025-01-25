FROM scratch

ARG SEMVER
ARG PG_VERSION
ARG TARGETARCH

WORKDIR /workspace
COPY ./build/vchord-bm25-pg${PG_VERSION}_${SEMVER}_${TARGETARCH}.deb /workspace/