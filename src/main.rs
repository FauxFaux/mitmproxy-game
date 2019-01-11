use std::iter::Peekable;
use std::str::Chars;

use failure::ensure;
use failure::err_msg;
use failure::Error;

#[derive(Debug)]
struct Block {
    sigil: char,
    data: String,
}

fn take_block(input: &mut Peekable<Chars>) -> Result<Option<Block>, Error> {
    match input.peek() {
        Some('}') => {
            input.next().unwrap();
            return Ok(Some(Block {
                sigil: '💖',
                data: String::new(),
            }));
        }
        None => return Ok(None),
        _ => (),
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

fn deconstruct(input: &str, prefix: &str) -> Result<(), Error> {
    let mut input = input.chars().peekable();

    while let Some(block) = take_block(&mut input)? {
        match block.sigil {
            ']' | '}' => {
                println!("{}{}:", prefix, block.sigil);
                deconstruct(&block.data, &format!("{}   ", prefix))?;
            }
            _ => println!("{}{:?}", prefix, block),
        }
    }

    Ok(())
}

fn main() -> Result<(), Error> {
    deconstruct(include_str!("../sample").trim_end(), "")?;

    Ok(())
}
