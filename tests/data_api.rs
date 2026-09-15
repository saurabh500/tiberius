use futures_util::io::{AsyncRead, AsyncWrite};
use futures_util::{FutureExt, TryStreamExt};
use once_cell::sync::Lazy;
use runtimes_macro::test_on_runtimes;
use std::borrow::Cow;
use std::env;
use std::panic::AssertUnwindSafe;
use std::sync::Once;
use tiberius::numeric::Numeric;
#[cfg(feature = "tds73")]
use tiberius::time::{Date, DateTime2, DateTimeOffset, Time};
use tiberius::time::{DateTime, SmallDateTime};
use tiberius::xml::XmlData;
use tiberius::{
    Column, ColumnData, ColumnType, FromSql, FromSqlOwned, IntoRow, IntoSql, Query, Result, ToSql,
    TokenRow, Uuid,
};

#[allow(dead_code)]
static LOGGER_SETUP: Once = Once::new();

static CONN_STR: Lazy<String> = Lazy::new(|| {
    env::var("TIBERIUS_TEST_CONNECTION_STRING").unwrap_or_else(|_| {
        "server=tcp:localhost,1433;user=SA;password=<YourStrong@Passw0rd>;TrustServerCertificate=true"
            .to_owned()
    })
});

fn table_name() -> String {
    format!("##data_api_{}", Uuid::new_v4().simple())
}

#[test]
fn public_value_conversions_cover_values_nulls_and_errors() -> Result<()> {
    macro_rules! check_value {
        ($value:expr, $variant:path, $ty:ty) => {{
            let value: $ty = $value;
            let data = value.into_sql();
            assert!(matches!(data, $variant(Some(_))));
            assert_eq!(
                Some(value),
                <$ty as FromSqlOwned>::from_sql_owned(data.clone())?
            );
            assert_eq!(Some(value), <$ty as FromSql>::from_sql(&data)?);
            assert!(matches!(Option::<$ty>::None.into_sql(), $variant(None)));
            assert!(matches!(Option::<$ty>::None.to_sql(), $variant(None)));
        }};
    }

    check_value!(true, ColumnData::Bit, bool);
    check_value!(u8::MAX, ColumnData::U8, u8);
    check_value!(-12, ColumnData::I16, i16);
    check_value!(-34, ColumnData::I32, i32);
    check_value!(-56, ColumnData::I64, i64);
    check_value!(1.25, ColumnData::F32, f32);
    check_value!(2.5, ColumnData::F64, f64);

    let uuid = Uuid::new_v4();
    assert_eq!(Some(uuid), Uuid::from_sql_owned(uuid.into_sql())?);
    assert!(matches!(
        Option::<Uuid>::None.into_sql(),
        ColumnData::Guid(None)
    ));
    assert!(matches!(
        Option::<Uuid>::None.to_sql(),
        ColumnData::Guid(None)
    ));

    let numeric = Numeric::new_with_scale(-12345, 2);
    assert_eq!(Some(numeric), Numeric::from_sql_owned(numeric.into_sql())?);
    assert!(matches!(
        Option::<Numeric>::None.into_sql(),
        ColumnData::Numeric(None)
    ));

    let owned_string = String::from("owned");
    assert_eq!(
        Some(owned_string.clone()),
        String::from_sql_owned(owned_string.clone().into_sql())?
    );
    let borrowed_string = ColumnData::String(Some(Cow::Borrowed("borrowed")));
    assert_eq!(Some("borrowed"), <&str>::from_sql(&borrowed_string)?);
    assert!(matches!(
        Option::<String>::None.into_sql(),
        ColumnData::String(None)
    ));
    assert!(matches!(
        Option::<&String>::None.into_sql(),
        ColumnData::String(None)
    ));
    assert!(matches!(
        Some(&owned_string).into_sql(),
        ColumnData::String(Some(_))
    ));
    assert!(matches!(
        Option::<Cow<'_, str>>::None.into_sql(),
        ColumnData::String(None)
    ));

    let owned_bytes = vec![1, 2, 3];
    assert_eq!(
        Some(owned_bytes.clone()),
        Vec::<u8>::from_sql_owned(owned_bytes.clone().into_sql())?
    );
    let borrowed_bytes = ColumnData::Binary(Some(Cow::Borrowed(&[4, 5])));
    assert_eq!(Some(&[4, 5][..]), <&[u8]>::from_sql(&borrowed_bytes)?);
    assert!(matches!(
        Option::<Vec<u8>>::None.into_sql(),
        ColumnData::Binary(None)
    ));
    assert!(matches!(
        Option::<Cow<'_, [u8]>>::None.into_sql(),
        ColumnData::Binary(None)
    ));
    assert!(matches!(
        Some(Cow::Borrowed(&[6, 7][..])).into_sql(),
        ColumnData::Binary(Some(_))
    ));

    let xml = XmlData::new("<root />");
    assert_eq!(
        Some(xml.clone()),
        XmlData::from_sql_owned(xml.clone().into_sql())?
    );
    assert!(matches!(
        Option::<XmlData>::None.into_sql(),
        ColumnData::Xml(None)
    ));
    assert!(matches!(
        Option::<&XmlData>::None.into_sql(),
        ColumnData::Xml(None)
    ));
    assert!(matches!(Some(&xml).into_sql(), ColumnData::Xml(Some(_))));

    assert!(i32::from_sql(&ColumnData::String(Some(Cow::Borrowed("wrong")))).is_err());
    assert!(String::from_sql_owned(ColumnData::I32(Some(1))).is_err());
    assert!(Vec::<u8>::from_sql_owned(ColumnData::I32(Some(1))).is_err());
    assert!(XmlData::from_sql_owned(ColumnData::I32(Some(1))).is_err());
    assert!(<&[u8]>::from_sql(&ColumnData::I32(Some(1))).is_err());
    assert!(<&XmlData>::from_sql(&ColumnData::I32(Some(1))).is_err());

    Ok(())
}

#[test]
fn public_numeric_time_xml_and_token_row_helpers() {
    let numeric = Numeric::new_with_scale(12345, 2);
    assert_eq!(123, numeric.int_part());
    assert_eq!(45, numeric.dec_part());
    assert_eq!(2, numeric.scale());
    assert_eq!(12345, numeric.value());
    assert_eq!(5, numeric.precision());
    assert_eq!("123.45", numeric.to_string());
    assert_eq!(123_i128, i128::from(numeric));
    assert_eq!(123_u128, u128::from(numeric));
    assert_eq!(123.45_f64, f64::from(numeric));
    assert_eq!(numeric, Numeric::new_with_scale(123450, 3));
    assert_ne!(numeric, Numeric::new_with_scale(12346, 2));
    assert_eq!(3, Numeric::new_with_scale(0, 2).precision());

    let datetime = DateTime::new(-1, 300);
    assert_eq!(-1, datetime.days());
    assert_eq!(300, datetime.seconds_fragments());
    let small_datetime = SmallDateTime::new(2, 3);
    assert_eq!(2, small_datetime.days());
    assert_eq!(3, small_datetime.seconds_fragments());
    #[cfg(feature = "tds73")]
    {
        let date = Date::new(42);
        assert_eq!(42, date.days());
        let time = Time::new(1_234_567, 7);
        assert_eq!(1_234_567, time.increments());
        assert_eq!(7, time.scale());
        assert_eq!(Time::new(10, 1), Time::new(100, 2));
        let datetime2 = DateTime2::new(date, time);
        assert_eq!(date, datetime2.date());
        assert_eq!(time, datetime2.time());
        let offset = DateTimeOffset::new(datetime2, -60);
        assert_eq!(datetime2, offset.datetime2());
        assert_eq!(-60, offset.offset());
    }

    let xml = XmlData::new("<root>value</root>");
    assert_eq!(None, xml.schema());
    assert_eq!("<root>value</root>", xml.as_ref());
    assert_eq!("<root>value</root>", xml.to_string());
    assert_eq!("<root>value</root>", xml.into_string());

    let column = Column::new("answer".to_owned(), ColumnType::Int4);
    assert_eq!("answer", column.name());
    assert_eq!(ColumnType::Int4, column.column_type());

    let mut row = TokenRow::with_capacity(2);
    assert!(row.is_empty());
    row.push(1i32.into_sql());
    row.push("two".into_sql());
    assert_eq!(2, row.len());
    assert!(matches!(row.get(0), Some(ColumnData::I32(Some(1)))));
    assert_eq!(2, row.iter().count());
    assert_eq!(2, row.clone().into_iter().count());
    row.clear();
    assert!(row.is_empty());

    assert_eq!(1, 1i32.into_row().len());
    assert_eq!(2, (1i32, 2i32).into_row().len());
    assert_eq!(3, (1i32, 2i32, 3i32).into_row().len());
    assert_eq!(4, (1i32, 2i32, 3i32, 4i32).into_row().len());
    assert_eq!(5, (1i32, 2i32, 3i32, 4i32, 5i32).into_row().len());
    assert_eq!(6, (1i32, 2i32, 3i32, 4i32, 5i32, 6i32).into_row().len());
    assert_eq!(
        7,
        (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32).into_row().len()
    );
    assert_eq!(
        8,
        (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32)
            .into_row()
            .len()
    );
    assert_eq!(
        9,
        (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32)
            .into_row()
            .len()
    );
    assert_eq!(
        10,
        (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32)
            .into_row()
            .len()
    );
}

#[test_on_runtimes]
async fn row_metadata_stream_and_collection_behavior<S>(mut conn: tiberius::Client<S>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let sql = "
        SELECT CAST(7 AS int) AS duplicate, CAST(NULL AS int) AS nullable,
               CAST(8 AS int) AS duplicate;
        SELECT CAST(1 AS int) AS empty WHERE 1 = 0;
        SELECT CAST(9 AS int) AS final";

    let mut stream = conn.simple_query(sql).await?;
    assert!(format!("{stream:?}").contains("QueryStream"));
    let columns = stream.columns().await?.unwrap();
    assert_eq!(
        ["duplicate", "nullable", "duplicate"],
        [columns[0].name(), columns[1].name(), columns[2].name(),]
    );

    let metadata_item = stream.try_next().await?.unwrap();
    assert!(metadata_item.as_row().is_none());
    let metadata = metadata_item.as_metadata().unwrap();
    assert_eq!(0, metadata.result_index());
    assert_eq!(ColumnType::Int4, metadata.columns()[0].column_type());
    let metadata = metadata_item.into_metadata().unwrap();
    assert_eq!(0, metadata.result_index());

    let row_item = stream.try_next().await?.unwrap();
    assert!(row_item.as_metadata().is_none());
    let row = row_item.as_row().unwrap();
    assert_eq!(0, row.result_index());
    assert_eq!(3, row.len());
    assert_eq!(Some(7), row.get(0));
    assert_eq!(Some(7), row.get("duplicate"));
    assert_eq!(None, row.get::<i32, _>("nullable"));
    assert_eq!(3, row.cells().count());
    assert!(row.try_get::<i32, _>("missing").is_err());
    assert!(row.try_get::<&str, _>(0).is_err());
    let out_of_range = AssertUnwindSafe(async { row.try_get::<i32, _>(99) })
        .catch_unwind()
        .await;
    assert!(out_of_range.is_err());
    let row = row_item.into_row().unwrap();
    assert_eq!(Some(8), row.get(2));
    assert_eq!(3, row.into_iter().count());

    let empty_metadata = stream.try_next().await?.unwrap().into_metadata().unwrap();
    assert_eq!(1, empty_metadata.result_index());
    let final_metadata = stream.try_next().await?.unwrap().into_metadata().unwrap();
    assert_eq!(2, final_metadata.result_index());
    let final_row = stream.try_next().await?.unwrap().into_row().unwrap();
    assert_eq!(2, final_row.result_index());
    assert_eq!(Some(9), final_row.get("final"));
    assert!(stream.try_next().await?.is_none());
    assert!(stream.columns().await?.is_some());
    drop(stream);

    let results = conn.simple_query(sql).await?.into_results().await?;
    // Consecutive metadata replaces the pending empty result, so into_results
    // omits an empty result set that is visible in the streaming API.
    assert_eq!(2, results.len());
    assert_eq!([1, 1], [results[0].len(), results[1].len()]);
    assert_eq!(Some(7), results[0][0].get("duplicate"));
    assert_eq!(Some(9), results[1][0].get("final"));

    let first = conn.simple_query(sql).await?.into_first_result().await?;
    assert_eq!(1, first.len());
    assert_eq!(Some(7), first[0].get(0));

    let one = conn.simple_query(sql).await?.into_row().await?.unwrap();
    assert_eq!(Some(7), one.get(0));
    assert!(conn
        .simple_query("PRINT 'no rows'")
        .await?
        .into_row()
        .await?
        .is_none());

    let rows: Vec<_> = conn
        .simple_query("SELECT 1 AS n UNION ALL SELECT 2")
        .await?
        .into_row_stream()
        .try_collect()
        .await?;
    assert_eq!(2, rows.len());

    Ok(())
}

#[test_on_runtimes]
async fn execute_dynamic_query_and_bulk_behavior<S>(mut conn: tiberius::Client<S>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let table = table_name();
    conn.execute(
        format!("CREATE TABLE {table} (id int NOT NULL, value nvarchar(8) NULL)"),
        &[],
    )
    .await?;

    let mut query = Query::new(format!(
        "INSERT INTO {table} (id, value) VALUES (@P1, @P2), (@P3, @P4)"
    ));
    query.bind(1i32);
    query.bind("one");
    query.bind(2i32);
    query.bind(Option::<&str>::None);
    let result = query.execute(&mut conn).await?;
    assert_eq!(&[2], result.rows_affected());
    assert_eq!(2, result.into_iter().sum::<u64>());

    let mut query = Query::new(format!(
        "SELECT id, value FROM {table} WHERE id >= @P1 ORDER BY id"
    ));
    query.bind(1i32);
    let rows: Vec<_> = query
        .query(&mut conn)
        .await?
        .into_row_stream()
        .try_collect()
        .await?;
    assert_eq!(Some("one"), rows[0].get("value"));
    assert_eq!(None, rows[1].get::<&str, _>("value"));

    let mut bulk = conn.bulk_insert(&table).await?;
    bulk.send((3i32, "three").into_row()).await?;
    bulk.send((4i32, Option::<&str>::None).into_row()).await?;
    let result = bulk.finalize().await?;
    assert_eq!(&[2], result.rows_affected());

    let mut invalid = conn.bulk_insert(&table).await?;
    let error = invalid
        .send((5i32, "too long for nvarchar(8)").into_row())
        .await
        .unwrap_err();
    assert!(matches!(error, tiberius::error::Error::BulkInput(_)));

    Ok(())
}

#[cfg(all(feature = "chrono", feature = "tds73"))]
#[test]
fn chrono_values_round_trip_through_public_conversions() -> Result<()> {
    use chrono::{FixedOffset, NaiveDate, TimeZone, Utc};
    use tiberius::time::chrono::{DateTime as ChronoDateTime, NaiveDateTime, NaiveTime};

    let date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
    let time = NaiveTime::from_hms_nano_opt(12, 34, 56, 123_456_700).unwrap();
    let datetime = NaiveDateTime::new(date, time);
    assert_eq!(Some(date), NaiveDate::from_sql_owned(date.into_sql())?);
    assert_eq!(Some(time), NaiveTime::from_sql_owned(time.into_sql())?);
    assert_eq!(
        Some(datetime),
        NaiveDateTime::from_sql_owned(datetime.into_sql())?
    );

    let utc: ChronoDateTime<Utc> = Utc.from_utc_datetime(&datetime);
    assert_eq!(
        Some(utc),
        ChronoDateTime::<Utc>::from_sql_owned(utc.into_sql())?
    );
    let fixed = FixedOffset::east_opt(90 * 60)
        .unwrap()
        .from_local_datetime(&datetime)
        .unwrap();
    assert_eq!(
        Some(fixed),
        ChronoDateTime::<FixedOffset>::from_sql_owned(fixed.into_sql())?
    );

    Ok(())
}

#[cfg(all(feature = "time", feature = "tds73"))]
#[test]
fn time_values_round_trip_through_public_conversions() -> Result<()> {
    use tiberius::time::time::{
        Date as TimeDate, Month, OffsetDateTime, PrimitiveDateTime, Time as TimeTime, UtcOffset,
    };

    let date = TimeDate::from_calendar_date(2024, Month::February, 29).unwrap();
    let time = TimeTime::from_hms_nano(12, 34, 56, 123_456_700).unwrap();
    let datetime = PrimitiveDateTime::new(date, time);
    let data = match date.to_sql() {
        ColumnData::Date(value) => ColumnData::Date(value),
        _ => unreachable!(),
    };
    assert_eq!(Some(date), TimeDate::from_sql_owned(data)?);
    let data = match time.to_sql() {
        ColumnData::Time(value) => ColumnData::Time(value),
        _ => unreachable!(),
    };
    assert_eq!(Some(time), TimeTime::from_sql_owned(data)?);
    let data = match datetime.to_sql() {
        ColumnData::DateTime2(value) => ColumnData::DateTime2(value),
        _ => unreachable!(),
    };
    assert_eq!(Some(datetime), PrimitiveDateTime::from_sql_owned(data)?);

    let offset = UtcOffset::from_hms(1, 30, 0).unwrap();
    let datetime = OffsetDateTime::new_in_offset(date, time, offset);
    let data = match datetime.to_sql() {
        ColumnData::DateTimeOffset(value) => ColumnData::DateTimeOffset(value),
        _ => unreachable!(),
    };
    assert_eq!(Some(datetime), OffsetDateTime::from_sql_owned(data)?);

    Ok(())
}
