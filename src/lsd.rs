use std::borrow::Borrow;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::rc::Rc;
use std::str::FromStr;

use indexmap::IndexMap;
use utf8_chars::BufReadCharsExt;

use crate::util::u16_from_4_hex_chars;
use crate::util::BoolGuardExt;

pub type Map<K, V> = Rc<IndexMap<K, V>>;

pub type Value = Rc<str>;
pub type List = Rc<[LSD]>;
pub type Level = IndexMap<Value, LSD>;

#[derive(Debug, Clone)]
pub enum LSD {
    Value(Value),
    Level(Level),
}

//
// Parse
//

#[derive(Debug, Clone)]
pub enum LSDParseError {
    ReadFailure(Rc<io::Error>),

    EmptyWhenExpectedValue,

    UnexpectedNonEmptyInlineLevel,

    UnexpectedLevelEnd,
    UnexpectedAfterLevelEnd,

    UnexpectedListEnd,
    UnexpectedAfterListEnd,

    UnexpectedStringEnd,
    UnexpectedCharEscapeEnd,
    UnexpectedCharEscapeUnicode,

    KeyCollisionValueWhenShouldBeLevel,
    KeyCollisionValueAlreadyExists(Value),
}

impl From<io::Error> for LSDParseError {
    fn from(value: io::Error) -> Self { Self::ReadFailure(Rc::from(value)) }
}

impl LSD {
    pub fn parse<S: Read>(stream: S) -> Result<LSD, LSDParseError> {
        let mut reader = BufReader::new(stream);
        let mut buf = String::new();
        // TODO allow values as root of lsd file
        Ok(LSD::Level(parse_level_inner(
            &mut reader,
            &mut buf,
            false,
        )?))
    }
}

fn parse_level<'a, S: Read>(
    reader: &mut BufReader<S>,
    buf: &'a mut String,
) -> Result<Level, LSDParseError> {
    use LSDParseError::*;

    // try empty level: read current line until `}`
    let mut inner_of_same_line_as_open = String::new();

    loop {
        match read(reader)? {
            // single line level (hopefully empty) { ... }
            Some('}') =>
                return match inner_of_same_line_as_open.trim() {
                    "" => Ok(Level::default()),
                    _ => Err(UnexpectedNonEmptyInlineLevel),
                },

            // eof case: { ...
            None => return Err(UnexpectedLevelEnd),

            // proper level: { ... \n
            Some('\n') => break,

            // still reading: { ...
            Some(c) => inner_of_same_line_as_open.push(c),
        }
    }

    parse_level_inner(reader, buf, true)
}

fn merge_level(insert_into: &mut Level, level: Level) -> Result<(), LSDParseError> {
    use LSDParseError::*;
    for (key, value) in level.into_iter() {
        match value {
            LSD::Value(value) => insert_into
                .insert(key.clone(), LSD::Value(value))
                .is_none()
                .ok_or_else(|| KeyCollisionValueAlreadyExists(key))?,
            LSD::Level(lvl) => match insert_into
                .entry(key)
                .or_insert_with(|| LSD::Level(Level::default()))
            {
                LSD::Value(_) => return Err(KeyCollisionValueWhenShouldBeLevel)?,
                LSD::Level(ref mut insert_into) => merge_level(insert_into, lvl)?,
            },
        }
    }
    return Ok(());
}

fn as_level(key_parts: Vec<&str>, value: LSD) -> Level {
    let mut result = Level::new();
    let mut insert_into = &mut result;

    for (i, key_part) in key_parts
        .iter()
        .enumerate()
    {
        if key_parts.len() - 1 == i {
            insert_into.insert((*key_part).into(), value);
            return result;
        } else {
            insert_into = match result
                .entry((*key_part).into())
                .or_insert_with(|| LSD::Level(Level::default()))
            {
                LSD::Value(_) => unreachable!(),
                LSD::Level(ref mut lvl) => lvl,
            }
        }
    }

    result
}

fn parse_level_inner<'a, S: Read>(
    reader: &mut BufReader<S>,
    buf: &'a mut String,
    level_ends_with_close: bool,
) -> Result<Level, LSDParseError> {
    use LSDParseError::*;

    let mut results = IndexMap::default();
    loop {
        let Some(key_first_char) = read_filled(reader)? else {
            return if level_ends_with_close {
                // wanted a `}`, got eof
                Err(UnexpectedLevelEnd)
            } else {
                // wanted eof (probably root only)
                Ok(results.into())
            };
        };

        // check if level ended
        if key_first_char == '}' {
            return if !level_ends_with_close {
                // found an unwanted `}`
                Err(UnexpectedLevelEnd)
            } else {
                // properly ended with a `}<eof>` or `}<whitespace>\n`
                Ok(results.into())
            };
        }

        // check if list ended here somehow
        if key_first_char == ']' {
            return Err(UnexpectedListEnd);
        }

        // read `"key with spaces"` or `key`
        let key = match key_first_char {
            '"' | '\'' => parse_string(reader, key_first_char)?,
            _ => {
                format!(
                    "{}{}",
                    key_first_char,
                    read_until_whitespace(reader, buf)?
                )
            },
        };

        let value = parse_value(reader, buf, true)?;
        let level = as_level(
            key.split('.')
                .collect(),
            value,
        );

        merge_level(&mut results, level)?;
    }
}

fn parse_list<'a, S: Read>(
    reader: &mut BufReader<S>,
    buf: &'a mut String,
) -> Result<Level, LSDParseError> {
    use LSDParseError::*;

    let mut results = IndexMap::default();
    loop {
        let Some(first_char) = read_filled(reader)? else {
            return Err(UnexpectedListEnd);
        };

        // check if list ended
        if first_char == ']' {
            return Ok(results.into());
        }

        // check if level ended here somehow
        if first_char == '}' {
            return Err(UnexpectedLevelEnd);
        }

        // insert `key text...\n` or `key { ... }\n` pair
        results.insert(
            Value::from(
                results
                    .len()
                    .to_string(),
            ),
            parse_value_with_first_char(reader, buf, false, first_char)?,
        );
    }
}

fn parse_string<S: Read>(
    reader: &mut BufReader<S>,
    closing_char: char,
) -> Result<String, LSDParseError> {
    use LSDParseError::*;

    let mut result = String::new();
    let mut escaping = false;
    loop {
        let ch = read(reader)?.ok_or(UnexpectedStringEnd)?;
        match escaping {
            true => {
                match ch {
                    '"' | '\\' | '\'' => result.push(ch),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    '0' => result.push('\0'),
                    'b' => result.push('\x08'),
                    'f' => result.push('\x0c'),
                    'u' | 'x' => result.push(
                        char::decode_utf16([u16_from_4_hex_chars(
                            read(reader)?.ok_or(UnexpectedStringEnd)?,
                            read(reader)?.ok_or(UnexpectedStringEnd)?,
                            read(reader)?.ok_or(UnexpectedStringEnd)?,
                            read(reader)?.ok_or(UnexpectedStringEnd)?,
                        )
                        .map_err(|()| UnexpectedCharEscapeUnicode)?])
                        .next()
                        .unwrap()
                        .map_err(|_| UnexpectedCharEscapeUnicode)?, // TODO support surrogate pairs
                    ),
                    _ => return Err(UnexpectedCharEscapeEnd)?,
                }
                escaping = false;
            },
            false => match ch {
                ch if ch == closing_char => return Ok(result),
                '\\' => escaping = true,
                ch => result.push(ch),
            },
        }
    }
}

fn parse_value<S: Read>(
    reader: &mut BufReader<S>,
    buf: &mut String,
    value_ends_with_newline: bool,
) -> Result<LSD, LSDParseError> {
    use LSDParseError::*;

    let Some(first_char) = read_filled(reader)? else {
        return Err(EmptyWhenExpectedValue);
    };

    parse_value_with_first_char(
        reader,
        buf,
        value_ends_with_newline,
        first_char,
    )
}

fn parse_value_with_first_char<S: Read>(
    reader: &mut BufReader<S>,
    buf: &mut String,
    value_ends_with_newline: bool,
    first_char: char,
) -> Result<LSD, LSDParseError> {
    use LSDParseError::*;
    Ok(match first_char {
        '{' => {
            // read Level level value [until line end]
            let level = parse_level(reader, buf)?;

            if value_ends_with_newline {
                // ensure with nothing after
                if read_line(reader, buf)?.is_some() {
                    return Err(UnexpectedAfterLevelEnd);
                }
            }

            LSD::Level(level)
        },
        '[' => {
            // read Level list value [until line end]
            let list = parse_list(reader, buf)?;

            if value_ends_with_newline {
                // ensure with nothing after
                if read_line(reader, buf)?.is_some() {
                    return Err(UnexpectedAfterListEnd);
                }
            }

            LSD::Level(list)
        },
        '"' | '\'' => {
            // read Value string until closing "

            let string = parse_string(reader, first_char)?;

            if value_ends_with_newline {
                // ensure with nothing after
                if read_line(reader, buf)?.is_some() {
                    return Err(UnexpectedAfterListEnd);
                }
            }

            LSD::Value(Value::from(string))
        },
        _ => {
            // read Value value until line/word end

            // from(first_char) guarantees `resulting_line` cannot be empty
            let mut resulting_line = String::from(first_char);
            resulting_line.push_str(
                (if value_ends_with_newline {
                    read_line(reader, buf).map(Option::unwrap_or_default)
                } else {
                    read_until_whitespace(reader, buf)
                })?,
            );

            LSD::Value(Value::from(resulting_line))
        },
    })
}

fn read<'a, S: Read>(reader: &mut BufReader<S>) -> Result<Option<char>, io::Error> {
    reader.read_char()
}

fn read_line<'a, S: Read>(
    reader: &mut BufReader<S>,
    buf: &'a mut String,
) -> Result<Option<&'a str>, io::Error> {
    buf.clear();
    Ok((reader.read_line(buf)? > 0)
        .then_some(buf.trim())
        .filter(|line| !line.is_empty()))
}

fn read_filled<'a, S: Read>(reader: &mut BufReader<S>) -> Result<Option<char>, io::Error> {
    loop {
        let Some(first_char) = read(reader)? else {
            return Ok(None);
        };
        if !first_char.is_whitespace() {
            return Ok(Some(first_char));
        }
    }
}

fn read_until_whitespace<'a, S: Read>(
    reader: &mut BufReader<S>,
    buf: &'a mut String,
) -> Result<&'a str, io::Error> {
    buf.clear();
    loop {
        let char = reader.read_char()?;
        match char {
            Some(c) if c.is_whitespace() => return Ok(buf),
            Some(c) => buf.push(c),
            None => return Ok(buf),
        }
    }
}

//
// KeyPath and values
//

pub type KeyPath = [Value];

#[macro_export]
macro_rules! key {
    (@$($collected:expr),*;) => { vec![$($collected),*] };
    (@$($collected:expr),*; $part:ident $($rest:tt)*) => {
        $crate::key!(
            @$($collected,)* $crate::lsd::Value::from(stringify!($part));
            $($rest)*
        )
    };
    (@$($collected:expr),*; $part:literal $($rest:tt)*) => {
        $crate::key!(
            @$($collected,)* $crate::lsd::Value::from($part);
            $($rest)*
        )
    };

    () => { vec![] };
    ($($rest:tt)*) => { $crate::key!(@; $($rest)*) };
}

impl LSD {
    pub fn to_value(&self) -> Option<Value> {
        match self.clone() {
            LSD::Value(value) => Some(value),
            _ => None,
        }
    }

    pub fn to_list(&self) -> Option<List> {
        match self.clone() {
            LSD::Level(level) => Some(
                level
                    .values()
                    .cloned()
                    .collect(),
            ),
            _ => None,
        }
    }

    pub fn to_level(&self) -> Option<Level> {
        match self.clone() {
            LSD::Level(level) => Some(level),
            _ => None,
        }
    }
}

pub trait LSDGetExt {
    fn get_inner(&self, parts: impl Borrow<KeyPath>) -> Option<LSD>;

    fn get_value<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<Value>, E>;

    fn get_parse<T: FromStr, E: Clone>(
        &self,
        parts: impl Borrow<KeyPath>,
        invalid: E,
    ) -> Result<Option<T>, E>;

    fn get_list<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<List>, E>;

    fn get_level<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<Level>, E>;

    fn is_list(&self) -> bool;
}

impl LSDGetExt for LSD {
    fn get_inner(&self, parts: impl Borrow<KeyPath>) -> Option<LSD> {
        let parts = parts.borrow();
        return match parts.split_first() {
            None => Some(self.clone()),
            Some((key, rest)) => match self {
                LSD::Level(map) => map
                    .get(key)
                    .and_then(|lsd| lsd.get_inner(rest)),
                _ => None,
            },
        };
    }

    fn get_value<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<Value>, E> {
        self.get_inner(parts)
            .as_ref()
            .map(LSD::to_value)
            .map(|v| v.ok_or(invalid))
            .transpose()
    }

    fn get_parse<T: FromStr, E: Clone>(
        &self,
        parts: impl Borrow<KeyPath>,
        invalid: E,
    ) -> Result<Option<T>, E> {
        self.get_value(parts, invalid.clone())?
            .as_ref()
            .map(Rc::as_ref)
            .map(str::parse)
            .map(|v| v.map_err(|_| invalid))
            .transpose()
    }

    fn get_list<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<List>, E> {
        self.get_inner(parts)
            .as_ref()
            .map(LSD::to_list)
            .map(|v| v.ok_or(invalid))
            .transpose()
    }

    fn get_level<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<Level>, E> {
        self.get_inner(parts)
            .as_ref()
            .map(LSD::to_level)
            .map(|v| v.ok_or(invalid))
            .transpose()
    }

    fn is_list(&self) -> bool {
        self.to_level()
            .as_ref()
            .is_some_and(Level::is_list)
    }
}

impl LSDGetExt for Level {
    fn get_inner(&self, parts: impl Borrow<KeyPath>) -> Option<LSD> {
        LSD::Level(self.clone()).get_inner(parts)
    }

    fn get_value<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<Value>, E> {
        LSD::Level(self.clone()).get_value(parts, invalid)
    }

    fn get_parse<T: FromStr, E: Clone>(
        &self,
        parts: impl Borrow<KeyPath>,
        invalid: E,
    ) -> Result<Option<T>, E> {
        LSD::Level(self.clone()).get_parse(parts, invalid)
    }

    fn get_list<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<List>, E> {
        LSD::Level(self.clone()).get_list(parts, invalid)
    }

    fn get_level<E>(&self, parts: impl Borrow<KeyPath>, invalid: E) -> Result<Option<Level>, E> {
        LSD::Level(self.clone()).get_level(parts, invalid)
    }

    fn is_list(&self) -> bool {
        self.keys()
            .all(|key| usize::from_str(key).is_ok())
    }
}
