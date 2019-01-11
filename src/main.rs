use std::io;
use std::io::Bytes;
use std::io::Read;
use std::iter::Peekable;

use failure::bail;
use failure::ensure;
use failure::err_msg;
use failure::format_err;
use failure::Error;
use itertools::Itertools;
use serde_json::Value;

#[derive(Debug)]
struct Block {
    sigil: u8,
    data: Vec<u8>,
}

fn take_block(input: &mut Peekable<Bytes<&[u8]>>) -> Result<Option<Block>, Error> {
    if input.peek().is_none() {
        return Ok(None);
    }

    let len = input
        .peeking_take_while(|c| {
            c.as_ref()
                .map(|&c| char::from(c).is_numeric())
                .unwrap_or(false)
        })
        .map(|c| c.map(char::from))
        .collect::<Result<String, io::Error>>()?
        .parse()?;

    let data = input.take(len).collect::<Result<Vec<u8>, io::Error>>()?;
    ensure!(
        data.len() == len,
        "short read, wanted: {}, got: {}",
        len,
        data.len()
    );

    let sigil = input.next().ok_or_else(|| err_msg("no trailing type"))??;

    Ok(Some(Block { sigil, data }))
}

fn deconstruct(input: Vec<u8>) -> Result<Vec<Value>, Error> {
    let mut input = input.bytes().peekable();

    let mut ret = Vec::new();

    while let Some(block) = take_block(&mut input)? {
        ret.push(match block.sigil {
            b']' => Value::Array(deconstruct(block.data)?),
            b'}' => Value::Object(
                deconstruct(block.data)?
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

            // ';' means "well known value", I believe. Could be "utf-8" or something.
            // ',' means "string"
            // '^' means "unix timestamp with nanos", which can't fit in a JS number
            // '~' appears to mean "empty string"
            b';' | b',' | b'^' | b'~' => Value::String(String::from_utf8(block.data)?),
            b'#' => Value::Number(String::from_utf8(block.data)?.parse()?),
            b'!' => match String::from_utf8(block.data)?.as_ref() {
                "false" => Value::Bool(false),
                "true" => Value::Bool(true),
                other => bail!("invalid boolean: {:?}", other),
            },
            other => bail!("unimplemented: {} ({:?})", other, block.data),
        });
    }

    Ok(ret)
}

fn main() -> Result<(), Error> {
    let mut input = Vec::new();
    io::stdin().lock().read_to_end(&mut input)?;
    let doc = deconstruct(input)?;

    serde_json::to_writer_pretty(io::stdout().lock(), &doc)?;

    Ok(())
}
