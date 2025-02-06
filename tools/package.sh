#!/usr/bin/env bash
set -eu

printf "SEMVER = ${SEMVER}\n"
printf "VERSION = ${VERSION}\n"
printf "ARCH = ${ARCH}\n"
printf "PLATFORM = ${PLATFORM}\n"

cargo build --lib --features pg$VERSION --release
cargo pgrx schema --features pg$VERSION --out ./target/schema.sql

rm -rf ./build

mkdir -p ./build/zip
[[ -d ./sql/upgrade ]] && cp -a ./sql/upgrade/. ./build/zip/
cp ./target/schema.sql ./build/zip/vchord_bm25--$SEMVER.sql
sed -e "s/@CARGO_VERSION@/$SEMVER/g" < ./vchord_bm25.control > ./build/zip/vchord_bm25.control
cp ./target/release/libvchord_bm25.so ./build/zip/vchord_bm25.so
zip ./build/postgresql-${VERSION}-vchord-bm25_${SEMVER}_${ARCH}-linux-gnu.zip -j ./build/zip/*

mkdir -p ./build/deb
mkdir -p ./build/deb/DEBIAN
mkdir -p ./build/deb/usr/share/postgresql/$VERSION/extension/
mkdir -p ./build/deb/usr/lib/postgresql/$VERSION/lib/
for file in $(ls ./build/zip/*.sql | xargs -n 1 basename); do
    cp ./build/zip/$file ./build/deb/usr/share/postgresql/$VERSION/extension/$file
done
for file in $(ls ./build/zip/*.control | xargs -n 1 basename); do
    cp ./build/zip/$file ./build/deb/usr/share/postgresql/$VERSION/extension/$file
done
for file in $(ls ./build/zip/*.so | xargs -n 1 basename); do
    cp ./build/zip/$file ./build/deb/usr/lib/postgresql/$VERSION/lib/$file
done
echo "Package: postgresql-${VERSION}-vchord-bm25
Version: ${SEMVER}
Section: database
Priority: optional
Architecture: ${PLATFORM}
Maintainer: Tensorchord <support@tensorchord.ai>
Description: Native BM25 Ranking Index in PostgreSQL
Homepage: https://github.com/tensorchord/VectorChord-bm25/
License: AGPL-3.0-only or Elastic-2.0" \
> ./build/deb/DEBIAN/control
(cd ./build/deb && md5sum usr/share/postgresql/$VERSION/extension/* usr/lib/postgresql/$VERSION/lib/*) > ./build/deb/DEBIAN/md5sums
dpkg-deb --root-owner-group -Zxz --build ./build/deb/ ./build/postgresql-${VERSION}-vchord-bm25_${SEMVER}-1_${PLATFORM}.deb
