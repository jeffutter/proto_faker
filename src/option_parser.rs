use std::collections::HashMap;
use winnow::ascii::{self, Caseless};
use winnow::error::{AddContext, ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::take_until;
use winnow::{
    ascii::{alphanumeric1, digit1},
    combinator::{alt, delimited, preceded, repeat, separated, separated_pair},
    token::{one_of, take_while},
};

use crate::PoolConfig;

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    ListInt(Vec<i64>),
    ListStr(Vec<String>),
    ListBool(Vec<bool>),
    Range(i64, i64),
    Distribution(Distribution),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueType {
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    String,
    Bytes,
    Uuid,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Distribution {
    Uniform,
    Normal(f64, f64),
    LogNormal(f64, f64),
    Pareto(f64, f64),
}

fn parse_value_type(input: &mut &str) -> winnow::error::ModalResult<ValueType> {
    let e = alt((
        Caseless("i32"),
        Caseless("i64"),
        Caseless("u32"),
        Caseless("u64"),
        Caseless("f32"),
        Caseless("f64"),
        Caseless("string"),
        Caseless("bytes"),
        Caseless("uuid"),
    ))
    .parse_next(input)?;

    match e.to_lowercase().as_str() {
        "i32" => Ok(ValueType::I32),
        "i64" => Ok(ValueType::I64),
        "u32" => Ok(ValueType::U32),
        "u64" => Ok(ValueType::U64),
        "f32" => Ok(ValueType::F32),
        "f64" => Ok(ValueType::F64),
        "string" => Ok(ValueType::String),
        "bytes" => Ok(ValueType::Bytes),
        "uuid" => Ok(ValueType::Uuid),
        _ => Err(ErrMode::Backtrack(ContextError::new().add_context(
            input,
            &input.checkpoint(),
            winnow::error::StrContext::Label("a ValueType"),
        ))),
    }
}

fn key<'i>(input: &mut &'i str) -> winnow::error::ModalResult<&'i str> {
    alphanumeric1.parse_next(input)
}

fn quoted_string(input: &mut &str) -> winnow::error::ModalResult<String> {
    delimited(
        '"',
        repeat(
            0..,
            alt((
                take_while(1.., |c| c != '"' && c != '\\').map(|s: &str| s.to_string()),
                preceded(
                    '\\',
                    one_of(['\"', 'n', '\\'])
                        .map(|c| match c {
                            'n' => '\n',
                            '"' => '"',
                            '\\' => '\\',
                            _ => c,
                        })
                        .map(|c| c.to_string()),
                ),
            )),
        )
        .map(|parts: Vec<String>| parts.concat()),
        '"',
    )
    .parse_next(input)
}

fn parse_int(input: &mut &str) -> winnow::error::ModalResult<i64> {
    digit1.parse_to().parse_next(input)
}

fn parse_range(input: &mut &str) -> winnow::error::ModalResult<Value> {
    let mut input_copy = *input;
    let start = parse_int(&mut input_copy)?;

    // Check if the next characters are ".."
    if input_copy.starts_with("..") {
        input_copy = &input_copy[2..]; // Skip the ".."
        if let Ok(end) = parse_int(&mut input_copy) {
            // Update the original input position
            *input = input_copy;
            return Ok(Value::Range(start, end));
        }
    }

    Err(ErrMode::Backtrack(ContextError::new().add_context(
        input,
        &input.checkpoint(),
        winnow::error::StrContext::Label("a range"),
    )))
}

fn parse_f64(input: &mut &str) -> winnow::error::ModalResult<f64> {
    ascii::float.parse_next(input)
}

fn parse_distribution(input: &mut &str) -> winnow::error::ModalResult<Distribution> {
    let distribution = alt((
        Caseless("uniform").map(|_| Distribution::Uniform),
        (Caseless("pareto"), "(", parse_f64, ",", parse_f64, "]")
            .map(|(_, _, a, _, b, _)| Distribution::Pareto(a, b)),
        (Caseless("normal"), "(", parse_f64, ",", parse_f64, "]")
            .map(|(_, _, a, _, b, _)| Distribution::Normal(a, b)),
        (Caseless("log_normal"), "(", parse_f64, ",", parse_f64, "]")
            .map(|(_, _, a, _, b, _)| Distribution::LogNormal(a, b)),
    ))
    .parse_next(input)?;

    Ok(distribution)
}

fn parse_bool(input: &mut &str) -> winnow::error::ModalResult<bool> {
    alt(("true".value(true), "false".value(false))).parse_next(input)
}

fn parse_bare_str(input: &mut &str) -> winnow::error::ModalResult<String> {
    take_while(1.., |c: char| {
        !c.is_whitespace() && c != ',' && c != ']' && c != '='
    })
    .map(str::to_string)
    .parse_next(input)
}

fn list_value(input: &mut &str) -> winnow::error::ModalResult<Value> {
    delimited(
        "[",
        alt((
            separated(1.., parse_bool, ",").map(Value::ListBool),
            separated(1.., parse_int, ",").map(Value::ListInt),
            separated(1.., quoted_string, ",").map(Value::ListStr),
        )),
        "]",
    )
    .parse_next(input)
}

fn value(input: &mut &str) -> winnow::error::ModalResult<Value> {
    alt((
        parse_distribution.map(Value::Distribution),
        quoted_string.map(Value::Str),
        list_value,
        parse_bool.map(Value::Bool),
        parse_range,
        parse_int.map(Value::Int),
        parse_bare_str.map(Value::Str),
    ))
    .parse_next(input)
}

fn key_value_pair(input: &mut &str) -> winnow::error::ModalResult<(String, Value)> {
    separated_pair(key, "=", value)
        .map(|(k, v)| (k.to_string(), v))
        .parse_next(input)
}

/// Parse a string containing key-value pairs and return a HashMap of the results
pub fn parse_options(input: &str) -> HashMap<String, Value> {
    let mut result = HashMap::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        // Try to parse a key-value pair at the current position
        let kv_result = key_value_pair.parse_next(&mut remaining);
        if let Ok((key, value)) = kv_result {
            result.insert(key, value);
        } else {
            // If we can't parse a key-value pair, advance by one character
            if !remaining.is_empty() {
                remaining = &remaining[1..];
            }
        }
    }

    result
}

pub fn parse_pool_config(input: &str) -> anyhow::Result<crate::PoolConfig> {
    let mut input = input;

    let (name, _, items, _, value) = (
        take_until(0.., ":").map(str::to_string),
        ":",
        digit1.parse_to::<usize>(),
        ":",
        parse_value_type,
    )
        .parse_next(&mut input)
        .map_err(|e| anyhow::format_err!("{e}"))?;

    Ok(PoolConfig { name, items, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_options() {
        let input = "noise key1=42 other key2=\"hello world\" key3=true key4=[false,true,false] key5=[1,2,3] key6=[\"a\",\"b\",\"c\"] skip-this key7=unquoted";
        let options = parse_options(input);

        assert_eq!(options.len(), 7);
        assert_eq!(options.get("key1"), Some(&Value::Int(42)));
        assert_eq!(
            options.get("key2"),
            Some(&Value::Str("hello world".to_string()))
        );
        assert_eq!(options.get("key3"), Some(&Value::Bool(true)));
        assert_eq!(
            options.get("key4"),
            Some(&Value::ListBool(vec![false, true, false]))
        );
        assert_eq!(options.get("key5"), Some(&Value::ListInt(vec![1, 2, 3])));
        assert_eq!(
            options.get("key6"),
            Some(&Value::ListStr(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string()
            ]))
        );
        assert_eq!(
            options.get("key7"),
            Some(&Value::Str("unquoted".to_string()))
        );
    }

    #[test]
    fn test_parse_complex_options() {
        let input = "command --flag key1=\"quoted string with \\\"escaped quotes\\\"\" key2=[1,2,3,4] key3=simple";
        let options = parse_options(input);

        assert_eq!(options.len(), 3);
        assert_eq!(
            options.get("key1"),
            Some(&Value::Str(
                "quoted string with \"escaped quotes\"".to_string()
            ))
        );
        assert_eq!(options.get("key2"), Some(&Value::ListInt(vec![1, 2, 3, 4])));
        assert_eq!(options.get("key3"), Some(&Value::Str("simple".to_string())));
    }

    #[test]
    fn test_parse_empty_and_malformed() {
        // Empty string
        let options = parse_options("");
        assert_eq!(options.len(), 0);

        // No key-value pairs
        let options = parse_options("just some random text without key-value pairs");
        assert_eq!(options.len(), 0);

        // Malformed key-value pairs should be skipped
        let input = "key1=42 malformed= key2=true";
        let options = parse_options(input);
        assert_eq!(options.len(), 2);
        assert_eq!(options.get("key1"), Some(&Value::Int(42)));
        assert_eq!(options.get("key2"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_parse_ranges() {
        let input = "range1=1..5 range2=10..20 not_range=5 mixed=1..10 text=hello";
        let options = parse_options(input);

        assert_eq!(options.len(), 5);
        assert_eq!(options.get("range1"), Some(&Value::Range(1, 5)));
        assert_eq!(options.get("range2"), Some(&Value::Range(10, 20)));
        assert_eq!(options.get("not_range"), None);
        assert_eq!(options.get("mixed"), Some(&Value::Range(1, 10)));
        assert_eq!(options.get("text"), Some(&Value::Str("hello".to_string())));

        // Test with spaces around the range operator
        let input2 = "range3=1 .. 5";
        let options2 = parse_options(input2);
        assert_eq!(options2.get("range3"), Some(&Value::Int(1)));
    }
}
