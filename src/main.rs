use std::io;
use std::io::Read;
use std::iter::Peekable;
use std::str::Chars;

use failure::bail;
use failure::ensure;
use failure::err_msg;
use failure::format_err;
use failure::Error;
use itertools::Itertools;
use serde_json::Value;

#[derive(Debug)]
struct Block {
    sigil: char,
    data: String,
}

fn take_block(input: &mut Peekable<Chars>) -> Result<Option<Block>, Error> {
    if input.peek().is_none() {
        return Ok(None);
    }

    let len = input
        .take_while(|c| c.is_numeric())
        .collect::<String>()
        .parse()?;

    let data = input.take(len).collect::<String>();
    ensure!(
        data.len() == len,
        "short read, wanted: {}, got: {}",
        len,
        data.len()
    );

    let sigil = input.next().ok_or_else(|| err_msg("no trailing type"))?;

    Ok(Some(Block { sigil, data }))
}

fn deconstruct(input: &str) -> Result<Vec<Value>, Error> {
    let mut input = input.chars().peekable();

    let mut ret = Vec::new();

    while let Some(block) = take_block(&mut input)? {
        ret.push(match block.sigil {
            ']' => Value::Array(deconstruct(&block.data)?),
            '}' => Value::Object(
                deconstruct(&block.data)?
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
            ';' | ',' | '^' | '~' => Value::String(block.data),
            '#' => Value::Number(block.data.parse()?),
            '!' => match block.data.as_str() {
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
    let mut input = String::new();
    io::stdin().lock().read_to_string(&mut input)?;
    let doc = deconstruct(input.trim_end())?;

    serde_json::to_writer_pretty(io::stdout().lock(), &doc)?;

    Ok(())
}
