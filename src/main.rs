use std::io;
use std::io::Bytes;
use std::io::Read;
use std::iter::Peekable;
use std::string::FromUtf8Error;

use failure::bail;
use failure::ensure;
use failure::err_msg;
use failure::format_err;
use failure::Error;
use failure::ResultExt;
use insideout::InsideOut;
use itertools::Itertools;
use serde_json::json;
use serde_json::Value;

#[derive(Debug)]
struct Block {
    sigil: u8,
    data: Vec<u8>,
}

fn take_block<R: Read>(input: &mut Peekable<Bytes<R>>) -> Result<Option<Block>, Error> {
    match input.peek() {
        Some(Ok(b'\n')) | None => return Ok(None),
        _ => (),
    }

    let len = input
        .peeking_take_while(|c| {
            c.as_ref()
                .map(|&c| char::from(c).is_numeric())
                .unwrap_or(false)
        })
        .map(|c| c.map(char::from))
        .collect::<Result<String, io::Error>>()?
        .parse::<usize>()
        .with_context(|_| {
            format_err!(
                "reading length near {:?}",
                String::from_utf8_lossy(&input.take(50).flat_map(|x| x.ok()).collect::<Vec<_>>())
            )
        })?;

    ensure!(
        b':' == input
            .next()
            .ok_or_else(|| err_msg("eof in colon after length"))??,
        "missing colon after length"
    );

    let data = input
        .take(len)
        .collect::<Result<Vec<u8>, io::Error>>()
        .with_context(|_| format_err!("reading block data {}", len))?;

    ensure!(
        data.len() == len,
        "short read, wanted: {}, got: {}",
        len,
        data.len()
    );

    let sigil = input
        .next()
        .ok_or_else(|| format_err!("no trailing type after block of len: {}", len))?
        .with_context(|_| format_err!("reading sigil"))?;

    Ok(Some(Block { sigil, data }))
}

struct Deconstruct<R: Read> {
    inner: Peekable<Bytes<R>>,
}

impl<R: Read> Deconstruct<R> {
    fn new(inner: R) -> Deconstruct<R> {
        Deconstruct {
            inner: inner.bytes().peekable(),
        }
    }
}

impl<R: Read> Iterator for Deconstruct<R> {
    type Item = Result<Value, Error>;

    fn next(&mut self) -> Option<Result<Value, Error>> {
        take_block(&mut self.inner)
            .inside_out()
            .map(|option| option.and_then(|block| expand_block(block)))
    }
}

fn expand_block(block: Block) -> Result<Value, Error> {
    Ok(match block.sigil {
        b']' => Value::Array(
            Deconstruct::new(io::Cursor::new(block.data))
                .collect::<Result<_, Error>>()
                .with_context(|_| format_err!("destructuring array"))?,
        ),

        b'}' => Value::Object(
            Deconstruct::new(io::Cursor::new(block.data))
                .collect::<Result<Vec<Value>, Error>>()
                .with_context(|_| format_err!("destructuring object"))?
                .into_iter()
                .tuples()
                .map(|(key, value)| -> Result<_, Error> {
                    Ok((
                        key.as_str()
                            .ok_or_else(|| format_err!("invalid non-string key: {:?}", key))?
                            .to_string(),
                        value,
                    ))
                })
                .collect::<Result<_, Error>>()?,
        ),

        b',' => match String::from_utf8(block.data) {
            Ok(s) => Value::String(s),
            Err(e) => json!({
                "base64": base64::encode(e.as_bytes()),
            }),
        },

        // ';' means "well known value", I believe. Could be "utf-8" or something.
        // '^' means "unix timestamp with nanos", which can't fit in a JS number
        // '~' appears to mean "empty string"
        a @ b';' | a @ b'^' | a @ b'~' => Value::String(
            String::from_utf8(block.data)
                .map_err(string_error)
                .with_context(|_| format_err!("reading string type {:?}", char::from(a)))?,
        ),
        b'#' => Value::Number(
            String::from_utf8(block.data)
                .with_context(|_| format_err!("reading number"))?
                .parse()?,
        ),
        b'!' => match String::from_utf8(block.data)
            .with_context(|_| format_err!("reading boolean"))?
            .as_ref()
        {
            "false" => Value::Bool(false),
            "true" => Value::Bool(true),
            other => bail!("invalid boolean: {:?}", other),
        },
        other => bail!(
            "unimplemented: {} ({:?})",
            char::from(other),
            String::from_utf8_lossy(&block.data)
        ),
    })
}

fn main() -> Result<(), Error> {
    for doc in Deconstruct::new(io::stdin().lock()) {
        serde_json::to_writer_pretty(io::stdout().lock(), &doc?)?;
    }
    Ok(())
}

fn string_error(e: FromUtf8Error) -> Error {
    let start = e.utf8_error().valid_up_to().saturating_sub(20);
    let end = (start + 60).min(e.as_bytes().len());
    format_err!(
        "bad string: {:?}...",
        String::from_utf8_lossy(&e.as_bytes()[start..end])
    )
}

#[test]
fn trivial() -> Result<(), Error> {
    assert_eq!(
        vec![json!({
            "headers": [
                ["Cache-Control", "no-transform"],
                ["Pragma", "no-cache"],
            ],
        })],
        Deconstruct::new(io::Cursor::new(
            b"75:7:headers;61:33:13:Cache-Control,12:no-transform,]20:6:Pragma,8:no-cache,]]}"
                .to_vec(),
        ))
        .collect::<Result<Vec<Value>, Error>>()?
    );
    Ok(())
}
