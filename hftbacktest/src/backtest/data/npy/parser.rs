use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{char, digit1, one_of},
    combinator::{cut, map, opt, value},
    error::{ContextError, ParseError, context},
    multi::separated_list0,
    sequence::{delimited, preceded, separated_pair, terminated},
};

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Value {
    String(String),
    Integer(usize),
    Bool(bool),
    List(Vec<Value>),
    Dict(HashMap<String, Value>),
}

impl Value {
    pub fn get_string(&self) -> std::io::Result<&str> {
        match self {
            Value::String(s) => Ok(s),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "must be a string".to_string(),
            )),
        }
    }

    pub fn get_integer(&self) -> std::io::Result<usize> {
        match self {
            Value::Integer(n) => Ok(*n),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "must be an unsigned integer".to_string(),
            )),
        }
    }

    pub fn get_bool(&self) -> std::io::Result<bool> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "must be a bool".to_string(),
            )),
        }
    }

    pub fn get_list(&self) -> std::io::Result<&Vec<Value>> {
        match self {
            Value::List(list) => Ok(list),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "must be a list".to_string(),
            )),
        }
    }

    pub fn get_dict(&self) -> std::io::Result<&HashMap<String, Value>> {
        match self {
            Value::Dict(dict) => Ok(dict),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "must be a dict".to_string(),
            )),
        }
    }
}

fn sp<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let chars = " \t\r\n";
    take_while(move |c| chars.contains(c))(input)
}

fn sp_with_comma<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let chars = " \t\r\n,";
    take_while(move |c| chars.contains(c))(input)
}

pub fn parse_str<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    escaped(
        take_while1(|c: char| c.is_alphanumeric() || "<>_".contains(c)),
        '\\',
        one_of("\"n\\n\'"),
    )(input)
}

pub fn parse_usize<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, usize, E> {
    let (remaining, digit_str) = digit1(input)?;
    match digit_str.parse::<usize>() {
        Ok(number) => Ok((remaining, number)),
        Err(_) => Err(nom::Err::Error(E::from_error_kind(
            input,
            nom::error::ErrorKind::Digit,
        ))),
    }
}

pub fn parse_boolean<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, bool, E> {
    let parse_true = value(true, tag("True"));
    let parse_false = value(false, tag("False"));
    alt((parse_true, parse_false))(input)
}

pub fn parse_string<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    context(
        "string",
        preceded(
            alt((char('\"'), char('\''))),
            cut(terminated(parse_str, alt((char('\"'), char('\''))))),
        ),
    )(input)
}

pub fn parse_list<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Vec<Value>, E> {
    context(
        "array",
        preceded(
            alt((char('['), char('('))),
            cut(terminated(
                separated_list0(preceded(sp, char(',')), parse_value),
                preceded(sp_with_comma, alt((char(']'), char(')')))),
            )),
        ),
    )(input)
}

pub fn parse_key_value<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, (&'a str, Value), E> {
    separated_pair(
        preceded(sp, parse_string),
        cut(preceded(sp, char(':'))),
        parse_value,
    )(input)
}

pub fn parse_dict<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, HashMap<String, Value>, E> {
    context(
        "map",
        preceded(
            char('{'),
            cut(terminated(
                map(
                    separated_list0(preceded(sp, char(',')), parse_key_value),
                    |tuple_vec| {
                        tuple_vec
                            .into_iter()
                            .map(|(k, v)| (String::from(k), v))
                            .collect()
                    },
                ),
                preceded(sp_with_comma, char('}')),
            )),
        ),
    )(input)
}

pub fn parse_value<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Value, E> {
    preceded(
        sp,
        alt((
            map(parse_dict, Value::Dict),
            map(parse_list, Value::List),
            map(parse_string, |s| Value::String(String::from(s))),
            map(parse_usize, Value::Integer),
            map(parse_boolean, Value::Bool),
        )),
    )(input)
}

pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Value, E> {
    delimited(
        sp,
        alt((map(parse_dict, Value::Dict), map(parse_list, Value::List))),
        opt(sp),
    )(input)
}
