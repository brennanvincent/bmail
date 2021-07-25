use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::bytes::complete::tag_no_case;

use nom::bytes::complete::take_while1;
use nom::character::complete::crlf;

use nom::combinator::consumed;
use nom::combinator::map;
use nom::combinator::opt;

use nom::combinator::value;

use nom::multi::separated_list1;
use nom::sequence::terminated;

use nom::IResult;

use crate::error::EmailError;
use crate::headers::address::Address;
use crate::headers::{HeaderField, HeaderFieldInner, HeaderFieldKind};
use crate::ByteStr;

use super::address::{address, mailbox};
use super::cfws;
use super::date_time::date_time;
use super::mime::content_type;
use super::unstructured;

fn is_ftext(ch: u8) -> bool {
    (33 <= ch && ch <= 57) || (59 <= ch && ch <= 126)
}
fn header_name(input: &[u8]) -> IResult<&[u8], HeaderFieldKind> {
    use HeaderFieldKind::*;
    alt((
        value(OrigDate, tag_no_case("date")),
        value(From, tag_no_case("from")),
        value(Sender, tag_no_case("sender")),
        value(ReplyTo, tag_no_case("reply-to")),
        value(To, tag_no_case("to")),
        value(Cc, tag_no_case("cc")),
        value(Bcc, tag_no_case("bcc")),
        value(ContentType, tag_no_case("content-type")),
        value(Unstructured, take_while1(is_ftext)),
    ))(input)
}

fn optional_address_list(i: &[u8]) -> IResult<&[u8], Vec<Address>> {
    map(
        opt(alt((
            separated_list1(tag(b","), address),
            value(vec![], cfws),
        ))),
        |maybe_list| maybe_list.unwrap_or(vec![]),
    )(i)
}

#[test]
fn test_weird_cc() {
    let input = b"Bcc: \r\n";
    use nom::combinator::complete;
    complete(header_field)(input).unwrap();
}

fn header_inner(
    hfk: HeaderFieldKind,
) -> impl Fn(&[u8]) -> IResult<&[u8], HeaderFieldInner, EmailError> {
    use HeaderFieldKind::*;

    move |i| match hfk {
        Unstructured => map(unstructured, |cooked| {
            HeaderFieldInner::Unstructured(cooked)
        })(i)
        .map_err(nom::Err::convert),
        OrigDate => map(date_time, |dt| HeaderFieldInner::OrigDate(dt))(i),
        From => map(separated_list1(tag(b","), mailbox), HeaderFieldInner::From)(i)
            .map_err(nom::Err::convert),
        Sender => map(mailbox, HeaderFieldInner::Sender)(i).map_err(nom::Err::convert),
        ReplyTo => map(
            separated_list1(tag(b","), address),
            HeaderFieldInner::ReplyTo,
        )(i)
        .map_err(nom::Err::convert),
        To => map(separated_list1(tag(b","), address), HeaderFieldInner::To)(i)
            .map_err(nom::Err::convert),
        // [RFC] seen in the wild - empty CC.  Parse it like BCC.
        Cc => map(optional_address_list, HeaderFieldInner::Cc)(i).map_err(nom::Err::convert),
        Bcc => map(optional_address_list, HeaderFieldInner::Bcc)(i).map_err(nom::Err::convert),
        ContentType => {
            map(content_type, HeaderFieldInner::ContentType)(i).map_err(nom::Err::convert)
        }
    }
}

pub fn header_field(input: &[u8]) -> IResult<&[u8], HeaderField, EmailError> {
    let (i, (name, hfk)) =
        terminated(consumed(header_name), tag(b":"))(input).map_err(nom::Err::convert)?;
    let (i, (raw_value, inner)) = terminated(consumed(header_inner(hfk)), crlf)(i)?;

    Ok((
        i,
        HeaderField::new(ByteStr::from_slice(name), raw_value, inner),
    ))
}

#[test]
fn test_from() {
    use nom::combinator::complete;

    let test = r#"From: Brennan Vincent <brennan@umanwizard.com>
"#
    .replace('\n', "\r\n");
    let hs = complete(header_field)(test.as_bytes());
    eprintln!("{:?}", hs);
}
