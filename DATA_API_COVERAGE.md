# Public data API integration coverage

Upstream baseline commit: `c1d2e741ee6fe0935091826d91fb5886b6a4082a`.

## Scope

The denominator is the public data API and the minimum private implementation
needed to provide it:

- `src/from_sql.rs`
- `src/to_sql.rs`
- `src/query.rs`
- `src/result.rs`
- `src/row.rs`
- `src/tds/stream/query.rs`
- `src/tds/codec/bulk_load.rs`
- `src/tds/codec/token/token_row.rs`
- `src/tds/codec/token/token_row/into_row.rs`

`token_row.rs` and `into_row.rs` are included because `TokenRow` and `IntoRow`
are public bulk-data APIs. `column_data.rs`, numeric/time/XML codecs, token
decoders, and RPC encoding are excluded because their executable lines
implement TDS encoding and protocol mechanics rather than the public API
behavior measured here. Public numeric, time, XML, and `ColumnData` conversions
are still exercised as API behavior. Connection establishment, TLS,
authentication, SQL Browser, pooling, transport, and packet framing are
excluded.

The feature set covers TDS 7.3, Chrono, time, rust_decimal, and bigdecimal.
SQL Browser, integrated authentication, and alternate TLS backends are outside
the data API scope.

## Reproduction

PowerShell:

```powershell
docker compose -f docker-compose.coverage.yml up -d
docker exec tiberius-data-coverage-29c03293 /opt/mssql-tools18/bin/sqlcmd `
  -S localhost -U sa -P 'TiberiusCov!2026' -C -Q 'SELECT 1' -b

$env:TIBERIUS_TEST_CONNECTION_STRING = `
  'server=tcp:127.0.0.1,15443;user=SA;password=TiberiusCov!2026;TrustServerCertificate=true'
cargo llvm-cov clean --workspace
cargo llvm-cov --features 'chrono,time,rust_decimal,bigdecimal' `
  --test query --test bulk --test data_api `
  --json --output-path coverage-final-integration.json

docker compose -f docker-compose.coverage.yml down
```

Omit `--test data_api` at the upstream commit to reproduce the baseline.
These selectors run integration-test binaries only; they do not run library
unit tests.

## Results

| Source file | Baseline | Final |
|---|---:|---:|
| `src/from_sql.rs` | 15/48 (31.25%) | 48/48 (100.00%) |
| `src/to_sql.rs` | 33/48 (68.75%) | 39/48 (81.25%) |
| `src/query.rs` | 34/34 (100.00%) | 34/34 (100.00%) |
| `src/result.rs` | 20/24 (83.33%) | 23/24 (95.83%) |
| `src/row.rs` | 72/124 (58.06%) | 85/124 (68.55%) |
| `src/tds/stream/query.rs` | 127/159 (79.87%) | 153/159 (96.23%) |
| `src/tds/codec/bulk_load.rs` | 49/53 (92.45%) | 49/53 (92.45%) |
| `src/tds/codec/token/token_row.rs` | 64/89 (71.91%) | 81/89 (91.01%) |
| `src/tds/codec/token/token_row/into_row.rs` | not instantiated | 95/95 (100.00%) |
| **Aggregate** | **414/579 (71.50%)** | **607/674 (90.06%)** |

LLVM has no coverage regions for generic implementations that no compiled test
instantiates. The baseline therefore has no `into_row.rs` entry; the final
denominator includes it after integration tests instantiate every supported
tuple arity. This is an increase in measured implementation, not a removed
baseline line.

## Compatibility observations

- `Row::try_get` reports a missing string column as `Error::Conversion`, but a
  numeric index beyond the row length panics because it unwraps `TokenRow::get`.
  `row_metadata_stream_and_collection_behavior` demonstrates both paths. A
  bridge can intentionally return an error for both without copying the panic.
- Streaming exposes metadata for an empty middle result set and advances result
  indexes through it. `QueryStream::into_results`, however, drops that empty
  set when two metadata items are consecutive. The same test pins both
  behaviors; a bridge should preserve empty result sets unless strict Tiberius
  compatibility requires the omission.
- Duplicate column-name lookup returns the first matching column. The row test
  pins this ordering.

## Tests to port

- **Conversions:** common primitive, string, binary, UUID, numeric, XML,
  Chrono, and time values; owned/borrowed values; NULLs; wrong-type failures.
- **Rows and metadata:** name/index lookup, duplicate names, missing and
  out-of-range indexes, cells, consuming iteration, column types, and result
  indexes.
- **Results and streams:** metadata/row variants, `columns`, raw streaming,
  `into_row_stream`, `into_results`, `into_first_result`, `into_row`, multiple
  sets, and empty sets.
- **Execute and dynamic queries:** owned SQL, dynamic binding, NULL binding,
  per-statement row counts, iteration, and totals.
- **Bulk data:** all `IntoRow` tuple arities, `TokenRow` helpers, successful
  nullable rows, final row counts, and oversized-value failure.

Remaining uncovered lines are mostly rare server type-to-`ColumnType` mappings,
stream error forwarding, and bulk write/flush failures that cannot be induced
through a healthy SQL Server without testing transport or protocol internals.
