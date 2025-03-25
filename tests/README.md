## Tests for vchord_bm25

We use [sqllogictest-rs](https://github.com/risinglightdb/sqllogictest-rs) to test the SQL queries.

To run all tests, use the following command:
```shell
sqllogictest './tests/**/*.slt'
```

Each time you modify the source code, you can run the following command to clean up the test data and reload the extension:
```shell
psql -f ./tests/init.sql
```

Tests for vchord_bm25 is dependent on [pg_tokenizer.rs](https://github.com/tensorchord/pg_tokenizer.rs). You need to install it first.
