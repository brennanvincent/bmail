use nom::branch::alt;
use nom::bytes::complete::tag;

use nom::bytes::complete::take_while1;
use nom::character::complete::crlf;

use nom::combinator::consumed;
use nom::combinator::map;
use nom::combinator::opt;

use nom::combinator::value;

use nom::multi::separated_list1;
use nom::sequence::terminated;

use nom::error::VerboseError;
use nom::{IResult, Parser};

use crate::error::EmailError;
use crate::headers::address::Address;
use crate::headers::{HeaderField, HeaderFieldInner, HeaderFieldKind};
use crate::ByteStr;

use super::address::{address, mailbox};
use super::cfws;
use super::date_time::date_time;
use super::mime::{content_transfer_encoding, content_type};
use super::unstructured;

fn is_ftext(ch: u8) -> bool {
    (33 <= ch && ch <= 57) || (59 <= ch && ch <= 126)
}

fn header_name(input: &[u8]) -> IResult<&[u8], HeaderFieldKind, VerboseError<&[u8]>> {
    use HeaderFieldKind::*;
    let (i, val) = take_while1(is_ftext)(input)?;
    let val = if val.eq_ignore_ascii_case(b"date") {
        OrigDate
    } else if val.eq_ignore_ascii_case(b"from") {
        From
    } else if val.eq_ignore_ascii_case(b"sender") {
        Sender
    } else if val.eq_ignore_ascii_case(b"reply-to") {
        ReplyTo
    } else if val.eq_ignore_ascii_case(b"to") {
        To
    } else if val.eq_ignore_ascii_case(b"cc") {
        Cc
    } else if val.eq_ignore_ascii_case(b"bcc") {
        Bcc
    } else if val.eq_ignore_ascii_case(b"content-type") {
        ContentType
    } else if val.eq_ignore_ascii_case(b"content-transfer-encoding") {
        ContentTransferEncoding
    } else {
        Unstructured
    };
    Ok((i, val))
}

fn optional_address_list(i: &[u8]) -> IResult<&[u8], Vec<Address>, VerboseError<&[u8]>> {
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

fn header_inner_permissive<'a>(
    hfk: HeaderFieldKind,
) -> impl Parser<&'a [u8], HeaderFieldInner<'a>, EmailError<'a>> {
    alt((
        header_inner(hfk),
        map(
            nom::Parser::into(unstructured),
            HeaderFieldInner::Unstructured,
        ),
    ))
}

fn header_inner(
    hfk: HeaderFieldKind,
) -> impl Fn(&[u8]) -> IResult<&[u8], HeaderFieldInner, EmailError> {
    use HeaderFieldKind::*;

    move |i| match hfk {
        Unstructured => {
            map(unstructured, HeaderFieldInner::Unstructured)(i).map_err(nom::Err::convert)
        }
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
        ContentTransferEncoding => map(
            content_transfer_encoding,
            HeaderFieldInner::ContentTransferEncoding,
        )(i)
        .map_err(nom::Err::convert),
    }
}

pub fn header_field(input: &[u8]) -> IResult<&[u8], HeaderField, EmailError> {
    let (i, (name, hfk)) =
        terminated(consumed(header_name), tag(b":"))(input).map_err(nom::Err::convert)?;
    let (i, (raw_value, inner)) = terminated(consumed(header_inner_permissive(hfk)), crlf)(i)?;

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
