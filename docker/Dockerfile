ARG PG_VERSION
ARG VCHORD_VERSION
ARG PG_TOKENIZER_VERSION
ARG TARGETARCH

# remove last '-' of tag in next release
FROM tensorchord/pg_tokenizer-binary:pg${PG_VERSION}-v${PG_TOKENIZER_VERSION}- AS binary

FROM tensorchord/vchord-postgres:pg${PG_VERSION}-v${VCHORD_VERSION}

ARG PG_VERSION
ARG PG_TOKENIZER_VERSION
ARG SEMVER
ARG TARGETARCH

RUN echo ${PG_VERSION}
COPY ./build/postgresql-${PG_VERSION}-vchord-bm25_${SEMVER}-1_${TARGETARCH}.deb /tmp/vchord-bm25.deb
RUN apt-get install -y /tmp/vchord-bm25.deb && rm -f /tmp/vchord-bm25.deb

COPY --from=binary /workspace/postgresql-${PG_VERSION}-pg-tokenizer_${PG_TOKENIZER_VERSION}-1_${TARGETARCH}.deb /tmp/pg-tokenizer.deb
RUN apt-get install -y /tmp/pg-tokenizer.deb && rm -f /tmp/pg-tokenizer.deb

CMD ["postgres", "-c" ,"shared_preload_libraries=vchord.so,pg_tokenizer.so,vchord_bm25.so"]
