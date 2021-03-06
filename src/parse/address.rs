use std::borrow::Cow;

use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::combinator::map;
use nom::combinator::opt;
use nom::combinator::value;
use nom::error::VerboseError;
use nom::multi::many0;
use nom::multi::separated_list1;
use nom::sequence::delimited;
use nom::sequence::tuple;
use nom::IResult;

use super::cfws;
use super::dot_atom;
use super::fws;
use super::phrase;
use super::quoted_string;
use super::satisfy_byte;

use crate::headers::address::{AddrSpec, Address, Domain, Group, Mailbox};
use crate::{ByteStr, ByteString};

fn local_part(input: &[u8]) -> IResult<&[u8], Cow<'_, ByteStr>, VerboseError<&[u8]>> {
    alt((map(dot_atom, Cow::Borrowed), map(quoted_string, Cow::Owned)))(input)
}

fn is_dtext(ch: u8) -> bool {
    (33 <= ch && ch <= 90) || (94 <= ch && ch <= 126)
}
fn domain_literal(input: &[u8]) -> IResult<&[u8], ByteString, VerboseError<&[u8]>> {
    map(
        delimited(
            tuple((cfws, tag(b"["))),
            many0(delimited(opt(fws), satisfy_byte(is_dtext), opt(fws))),
            tuple((tag(b"]"), cfws)),
        ),
        ByteString,
    )(input)
}

pub fn domain(input: &[u8]) -> IResult<&[u8], Domain, VerboseError<&[u8]>> {
    alt((
        map(dot_atom, Domain::Name),
        map(domain_literal, Domain::Literal),
    ))(input)
}

pub fn addr_spec(input: &[u8]) -> IResult<&[u8], AddrSpec, VerboseError<&[u8]>> {
    map(
        tuple((local_part, tag(b"@"), domain)),
        |(local_part, _, domain)| AddrSpec { local_part, domain },
    )(input)
}

pub fn angle_addr(input: &[u8]) -> IResult<&[u8], Option<AddrSpec>, VerboseError<&[u8]>> {
    delimited(
        tuple((opt(cfws), tag(b"<"))),
        opt(addr_spec), // [RFC] WTF? Yep, `Reply-To: Foo <>` seen in the wild!
        tuple((tag(b">"), opt(cfws))),
    )(input)
}

pub fn mailbox(input: &[u8]) -> IResult<&[u8], Mailbox, VerboseError<&[u8]>> {
    let name_addr = tuple((opt(phrase), angle_addr));

    map(
        alt((name_addr, map(addr_spec, |spec| (None, Some(spec))))),
        |(display_name, addr_spec)| Mailbox {
            display_name: display_name.unwrap_or(vec![]),
            addr_spec,
        },
    )(input)
}

pub fn group(input: &[u8]) -> IResult<&[u8], Group, VerboseError<&[u8]>> {
    map(
        tuple((
            phrase,
            tag(b":"),
            opt(alt((
                separated_list1(tag(b","), mailbox),
                value(vec![], cfws),
            ))),
            tag(b";"),
            opt(cfws),
        )),
        |(display_name, _, mailboxes, _, _)| {
            let mailboxes = mailboxes.unwrap_or(vec![]);
            Group {
                display_name,
                mailboxes,
            }
        },
    )(input)
}

pub fn address(input: &[u8]) -> IResult<&[u8], Address, VerboseError<&[u8]>> {
    alt((map(mailbox, Address::Mailbox), map(group, Address::Group)))(input)
}
