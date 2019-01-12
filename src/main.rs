use std::io;
use std::io::BufRead;
use std::io::Bytes;
use std::iter::Peekable;
use std::string::FromUtf8Error;

use failure::bail;
use failure::ensure;
use failure::err_msg;
use failure::format_err;
use failure::Error;
use failure::ResultExt;
use itertools::Itertools;
use serde_json::json;
use serde_json::Value;

#[derive(Debug)]
struct Block {
    sigil: u8,
    data: Vec<u8>,
}

fn take_block<R: BufRead>(input: &mut Peekable<Bytes<R>>) -> Result<Option<Block>, Error> {
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

fn deconstruct<R: BufRead, F: FnMut(Value)>(input: R, mut cb: F) -> Result<(), Error> {
    let mut input = input.bytes().peekable();

    while let Some(block) = take_block(&mut input).with_context(|_| format_err!("reading block"))? {
        cb(match block.sigil {
            b']' => {
                let mut contents = Vec::new();
                deconstruct(io::Cursor::new(block.data), |v| contents.push(v))
                    .with_context(|_| format_err!("destructuring array"))?;
                Value::Array(contents)
            }
            b'}' => {
                let mut contents: Vec<Value> = Vec::new();
                deconstruct(io::Cursor::new(block.data), |v| contents.push(v))
                    .with_context(|_| format_err!("destructuring object"))?;
                Value::Object(
                    contents
                        .into_iter()
                        .tuples()
                        .map(|(key, value)| -> Result<_, Error> {
                            Ok((
                                key.as_str()
                                    .ok_or_else(|| {
                                        format_err!("invalid non-string key: {:?}", key)
                                    })?
                                    .to_string(),
                                value,
                            ))
                        })
                        .collect::<Result<_, Error>>()?,
                )
            }

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
        });
    }

    Ok(())
}

fn main() -> Result<(), Error> {
    deconstruct(io::stdin().lock(), |doc| {
        serde_json::to_writer_pretty(io::stdout().lock(), &doc).unwrap()
    })?;

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
    let mut bodies = Vec::new();
    deconstruct(
        io::Cursor::new(
            b"75:7:headers;61:33:13:Cache-Control,12:no-transform,]20:6:Pragma,8:no-cache,]]}"
                .to_vec(),
        ),
        |v| bodies.push(v),
    )?;

    assert_eq!(
        vec![json!({
            "headers": [
                ["Cache-Control", "no-transform"],
                ["Pragma", "no-cache"],
            ],
        })],
        bodies
    );
    Ok(())
}
