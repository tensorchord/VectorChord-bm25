ARG PG_VERSION=17
ARG VCHORD_VERSION=0.2.1

FROM tensorchord/vchord-postgres:pg${PG_VERSION}-v${VCHORD_VERSION}

ARG PG_VERSION
ARG SEMVER=0.0.0
ARG TARGETARCH

RUN echo ${PG_VERSION}
COPY ./build/postgresql-${PG_VERSION}-vchord-bm25_${SEMVER}-1_${TARGETARCH}.deb /tmp/vchord-bm25.deb
RUN apt-get install -y /tmp/vchord-bm25.deb && rm -f /tmp/vchord-bm25.deb

CMD ["postgres", "-c" ,"shared_preload_libraries=vchord.so,vchord_bm25.so"]
