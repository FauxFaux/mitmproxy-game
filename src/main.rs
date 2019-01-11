use std::iter::Peekable;
use std::str::Chars;

use failure::bail;
use failure::err_msg;
use failure::Error;

#[derive(Copy, Clone, Debug)]
enum Sigil {
    Plain,
    Array,
    Object,
}

#[derive(Debug)]
struct Block {
    sigil: Sigil,
    data: String,
}

fn take_block(input: &mut Peekable<Chars>) -> Result<Block, Error> {
    let sigil = match *input.peek().ok_or_else(|| err_msg("eof in header: good"))? {
        c if c.is_numeric() => Sigil::Plain,

        ']' => {
            input.next().unwrap();
            Sigil::Array
        }
        ';' => {
            input.next().unwrap();
            Sigil::Object
        }
        other => {
            bail!("unrecognised sigil: {:?}", other);
        }
    };

    let number: usize = input
        .take_while(|c| c.is_numeric())
        .collect::<String>()
        .parse()?;
    let data = input.take(number).collect::<String>();

    Ok(Block { sigil, data })
}

fn main() -> Result<(), Error> {
    let input = include_str!("../sample");
    let mut input = input.chars().peekable();
    loop {
        println!("{:?}", take_block(&mut input)?);
    }
    Ok(())
}
