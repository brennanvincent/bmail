use nom::branch::alt;
use nom::bytes::complete::tag;

use nom::combinator::map;
use nom::combinator::opt;

use nom::combinator::value;

use nom::multi::fold_many_m_n;

use nom::sequence::preceded;
use nom::sequence::tuple;

use nom::error::VerboseError;
use nom::IResult;

use super::super::error::EmailError;

use super::cfws;
use super::fws;
use super::satisfy_byte;

fn day_of_week(input: &[u8]) -> IResult<&[u8], chrono::Weekday, VerboseError<&[u8]>> {
    use chrono::Weekday;
    map(
        tuple((
            opt(fws),
            alt((
                value(Weekday::Mon, tag(b"Mon")),
                value(Weekday::Tue, tag(b"Tue")),
                value(Weekday::Wed, tag(b"Wed")),
                value(Weekday::Thu, tag(b"Thu")),
                value(Weekday::Fri, tag(b"Fri")),
                value(Weekday::Sat, tag(b"Sat")),
                value(Weekday::Sun, tag(b"Sun")),
            )),
        )),
        |(_, dow)| dow,
    )(input)
}

fn month(input: &[u8]) -> IResult<&[u8], chrono::Month, VerboseError<&[u8]>> {
    use chrono::Month;
    alt((
        value(Month::January, tag(b"Jan")),
        value(Month::February, tag(b"Feb")),
        value(Month::March, tag(b"Mar")),
        value(Month::April, tag(b"Apr")),
        value(Month::May, tag(b"May")),
        value(Month::June, tag(b"Jun")),
        value(Month::July, tag(b"Jul")),
        value(Month::August, tag(b"Aug")),
        value(Month::September, tag(b"Sep")),
        value(Month::October, tag(b"Oct")),
        value(Month::November, tag(b"Nov")),
        value(Month::December, tag(b"Dec")),
    ))(input)
}

fn day(input: &[u8]) -> IResult<&[u8], u8, VerboseError<&[u8]>> {
    map(
        tuple((
            opt(fws),
            fold_many_m_n(1, 2, satisfy_byte(|ch| ch.is_ascii_digit()), 0, |acc, n| {
                acc * 10 + (n - b'0')
            }),
            fws,
        )),
        |(_, day, _)| day,
    )(input)
}

fn year(input: &[u8]) -> IResult<&[u8], u16, VerboseError<&[u8]>> {
    map(
        tuple((
            fws,
            // 9-digit years should be enough for anyone
            fold_many_m_n(4, 9, satisfy_byte(|ch| ch.is_ascii_digit()), 0, |acc, n| {
                acc * 10 + (n - b'0') as u16
            }),
            fws,
        )),
        |(_, day, _)| day,
    )(input)
}

fn date(input: &[u8]) -> IResult<&[u8], chrono::NaiveDate, EmailError> {
    let (i, (day, month, year)) = tuple((day, month, year))(input).map_err(nom::Err::convert)?;
    let date = chrono::NaiveDate::from_ymd_opt(year as i32, month.number_from_month(), day as u32)
        .ok_or_else(|| {
            nom::Err::Error(EmailError::BadDate {
                y: year,
                m: month,
                d: day,
            })
        })?;
    Ok((i, date))
}

fn two_digit(input: &[u8]) -> IResult<&[u8], u8, VerboseError<&[u8]>> {
    fold_many_m_n(2, 2, satisfy_byte(|ch| ch.is_ascii_digit()), 0, |acc, n| {
        acc * 10 + (n - b'0')
    })(input)
}

fn modern_zone(i: &[u8]) -> IResult<&[u8], chrono::offset::FixedOffset, EmailError> {
    let (i, (_, pm, hh, mm)) = tuple((fws, alt((tag(b"+"), tag(b"-"))), two_digit, two_digit))(i)
        .map_err(nom::Err::convert)?;
    let is_east = match pm {
        b"+" => true,
        b"-" => false,
        _ => unreachable!(),
    };
    use chrono::offset::FixedOffset;
    let offset_seconds = hh as i32 * 3600 + mm as i32 * 60;
    let tz = if is_east {
        FixedOffset::east_opt(offset_seconds)
    } else {
        FixedOffset::west_opt(offset_seconds)
    }
    .ok_or(nom::Err::Error(EmailError::BadTZOffset { is_east, hh, mm }))?;
    Ok((i, tz))
}

fn obs_zone(i: &[u8]) -> IResult<&[u8], chrono::offset::FixedOffset, EmailError> {
    // Doesn't support the one-letter military time zones
    use chrono::offset::FixedOffset;
    // The RFC technically doesn't allow whitespace here; see
    // https://www.rfc-editor.org/errata/eid6639
    preceded(
        opt(fws),
        alt((
            value(FixedOffset::east(0), alt((tag(b"UT"), tag(b"GMT")))),
            value(FixedOffset::east(4 * 3600), tag(b"EDT")),
            value(FixedOffset::east(5 * 3600), alt((tag(b"EST"), tag(b"CDT")))),
            value(FixedOffset::east(6 * 3600), alt((tag(b"CST"), tag(b"MDT")))),
            value(FixedOffset::east(7 * 3600), alt((tag(b"MST"), tag(b"PDT")))),
            value(FixedOffset::east(8 * 3600), tag(b"PST")),
        )),
    )(i)
    .map_err(nom::Err::convert)
}

fn zone(i: &[u8]) -> IResult<&[u8], chrono::offset::FixedOffset, EmailError> {
    alt((modern_zone, obs_zone))(i)
}

fn time(
    date: chrono::NaiveDate,
) -> impl Fn(&[u8]) -> IResult<&[u8], chrono::DateTime<chrono::offset::FixedOffset>, EmailError> {
    move |i| {
        use chrono::TimeZone;
        let (i, (h, _, m, s)) = tuple((
            two_digit,
            tag(b":"),
            two_digit,
            opt(map(tuple((tag(b":"), two_digit)), |(_, s)| s)),
        ))(i)
        .map_err(nom::Err::convert)?;
        let (i, tz) = zone(i)?;
        use chrono::offset::LocalResult;
        let date_time = match tz.from_local_date(&date) {
            LocalResult::None => None,
            LocalResult::Single(d) => Some(d),
            LocalResult::Ambiguous(d, _) => Some(d),
        }
        .and_then(|d| d.and_hms_opt(h as u32, m as u32, s.unwrap_or(0) as u32))
        .ok_or(nom::Err::Error(EmailError::BadDateTime {
            date,
            tz,
            h,
            m,
            s,
        }))?;
        Ok((i, date_time))
    }
}

pub fn date_time(
    i: &[u8],
) -> IResult<&[u8], chrono::DateTime<chrono::offset::FixedOffset>, EmailError> {
    let (i, weekday) = opt(tuple((day_of_week, tag(b","))))(i).map_err(nom::Err::convert)?;
    let (i, date) = date(i)?;
    let (i, time) = time(date)(i)?;
    let (i, _) = opt(cfws)(i).map_err(nom::Err::convert)?;
    if let Some((weekday, _)) = weekday {
        use chrono::Datelike;
        if time.weekday() != weekday {
            return Err(nom::Err::Error(EmailError::BadWeekday {
                date_time: time,
                weekday,
            }));
        }
    }
    Ok((i, time))
}
