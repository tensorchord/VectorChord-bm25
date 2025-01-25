ARG PG_VERSION=17
ARG PGVECTOR=0.8.0

FROM pgvector/pgvector:${PGVECTOR}-pg${PG_VERSION}

ARG PG_VERSION
ARG SEMVER=0.0.0
ARG TARGETARCH

RUN echo ${PG_VERSION}
COPY ./build/vchord-bm25-pg${PG_VERSION}_${SEMVER}_${TARGETARCH}.deb /tmp/vchord-bm25.deb
RUN apt-get install -y /tmp/vchord-bm25.deb && rm -f /tmp/vchord-bm25.deb

CMD ["postgres", "-c" ,"shared_preload_libraries=vchord_bm25.so"]