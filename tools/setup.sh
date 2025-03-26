#!/usr/bin/env bash
set -xeu

printf "POSTGRES_VERSION = ${POSTGRES_VERSION}\n"
printf "PG_TOKENIZER_VERSION = ${PG_TOKENIZER_VERSION}\n"
printf "PLATFORM = ${PLATFORM}\n"

sudo apt-get update
sudo apt-get remove -y '^postgres.*' '^libpq.*'
sudo apt-get purge -y '^postgres.*' '^libpq.*'
sudo apt-get install -y postgresql-common
sudo /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y
sudo apt-get install -y postgresql-server-dev-${POSTGRES_VERSION}
sudo apt-get install -y postgresql-${POSTGRES_VERSION}

echo "local all all trust" | sudo tee /etc/postgresql/${POSTGRES_VERSION}/main/pg_hba.conf
echo "host all all 127.0.0.1/32 trust" | sudo tee -a /etc/postgresql/${POSTGRES_VERSION}/main/pg_hba.conf
echo "host all all ::1/128 trust" | sudo tee -a /etc/postgresql/${POSTGRES_VERSION}/main/pg_hba.conf
sudo -iu postgres createuser -s -r $USER
sudo -iu postgres createdb -O $USER $USER
sudo -iu postgres psql -c 'ALTER SYSTEM SET shared_preload_libraries = "pg_tokenizer.so","vchord_bm25.so"'
sudo -iu postgres psql -c 'ALTER SYSTEM SET search_path TO "\$user", public, tokenizer_catalog, bm25_catalog;'
sudo systemctl stop postgresql

curl -fsSL https://github.com/tensorchord/pg_tokenizer.rs/releases/download/${PG_TOKENIZER_VERSION}/postgresql-${POSTGRES_VERSION}-pg-tokenizer_${PG_TOKENIZER_VERSION}-1_${PLATFORM}.deb -o pg_tokenizer.deb
sudo apt install -y ./pg_tokenizer.deb
rm pg_tokenizer.deb

curl -fsSL https://github.com/tensorchord/pgrx/releases/download/v0.13.1/cargo-pgrx-v0.13.1-$(uname -m)-unknown-linux-gnu.tar.gz | tar -xOzf - ./cargo-pgrx | install -m 755 /dev/stdin /usr/local/bin/cargo-pgrx
cargo pgrx init --pg${POSTGRES_VERSION}=$(which pg_config)

curl -fsSL https://github.com/risinglightdb/sqllogictest-rs/releases/download/v0.26.4/sqllogictest-bin-v0.26.4-$(uname -m)-unknown-linux-musl.tar.gz | tar -xOzf - ./sqllogictest | install -m 755 /dev/stdin /usr/local/bin/sqllogictest
