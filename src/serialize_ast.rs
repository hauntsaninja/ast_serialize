//! Serialize the AST for a given Python file as a mypy AST

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::PySourceType;
use ruff_python_ast::{self as ast};
use ruff_python_parser::{ParseOptions, parse};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

const TAG_EXPR_STMT: u8 = 11;
const TAG_CALL_EXPR: u8 = 12;
const TAG_NAME_EXPR: u8 = 13;
const TAG_STR_EXPR: u8 = 14;
const TAG_IMPORT: u8 = 15;

const MIN_SHORT_INT: i64 = -10;
const MIN_TWO_BYTES_INT: i64 = -100;
const MAX_TWO_BYTES_INT: i64 = 16283;  // 2 ** (8 + 6) - 1 - 100
const MIN_FOUR_BYTES_INT: i64 = -10000;
const MAX_FOUR_BYTES_INT: i64 = 536860911;  // 2 ** (3 * 8 + 5) - 1 - 10000

const TWO_BYTES_INT_BIT: i64 = 1;
const FOUR_BYTES_INT_TRAILER: i64 = 3;
const LONG_INT_TRAILER: u8 = 15;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Python file for which to generate the AST.
    #[arg(required = true)]
    file: PathBuf,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let source_type = PySourceType::from(&args.file);
    let source_kind = SourceKind::from_path(&args.file, source_type)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine source kind for file: {}",
            args.file.display()
        )
    })?;
    let start = Instant::now();
    let python_ast =
        parse(source_kind.source_code(), ParseOptions::from(source_type))?.into_syntax();
    let _ = start.elapsed();
    let mut v = Vec::new();
    let line_index = LineIndex::from_source_text(source_kind.source_code());
    let mut state = State { imports: Vec::new() };
    python_ast.serialize(&mut v, &mut state, &line_index, source_kind.source_code()).unwrap();

    io::stdout().write_all(&v)?;

    Ok(())
}

struct Import {
    name: String,
    relative: i32,
    as_name: Option<String>,
}

struct State {
    imports: Vec<Import>
}

trait Ser {
    fn serialize<W: Write>(&self, w: &mut W, state: &mut State, l: &LineIndex, text: &str) -> io::Result<()>;
}

impl Ser for ast::Mod {
    fn serialize<W: Write>(&self, w: &mut W, state: &mut State, l: &LineIndex, text: &str) -> io::Result<()> {
        match self {
            ast::Mod::Module(m) => {
                write_int(w, m.body.len() as i64)?;
                for stmt in &m.body {
                    stmt.serialize(w, state, l, text)?;
                }
            }
            ast::Mod::Expression(_) => {
                panic!("Expression unsupported");
            }
        }
        Ok(())
    }
}

impl Ser for ast::Stmt {
    fn serialize<W: Write>(&self, w: &mut W, state: &mut State, l: &LineIndex, text: &str) -> io::Result<()> {
        match self {
            ast::Stmt::Expr(e) => {
                w.write_all(&[TAG_EXPR_STMT])?;
                e.value.serialize(w, state, l, text)?;
            }
            ast::Stmt::Import(i) => {
                w.write_all(&[TAG_IMPORT])?;
                for name in &i.names {
                    write_bytes(w, name.name.as_bytes())?;
                    state.imports.push(Import { name: name.name.to_string(), relative: 0, as_name: None});
                }
                write_location(w, l, text, i.range())?;                
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        Ok(())
    }
}

impl Ser for ast::Expr {
    fn serialize<W: Write>(&self, w: &mut W, state: &mut State, l: &LineIndex, text: &str) -> io::Result<()> {
        let write_loc = |w: &mut W, r: TextRange| write_location(w, l, text, r);

        match self {
            ast::Expr::Name(n) => {
                w.write_all(&[TAG_NAME_EXPR])?;
                write_bytes(w, n.id.as_bytes())?;
                write_loc(w, n.range())?;
            }
            ast::Expr::StringLiteral(s) => {
                w.write_all(&[TAG_STR_EXPR])?;
                let value = &s.value;
                write_usize(w, value.len())?;
                for part in value.iter() {
                    w.write(part.as_bytes())?;
                }
                write_loc(w, s.range())?;
            }
            ast::Expr::Call(c) => {
                w.write_all(&[TAG_CALL_EXPR])?;
                c.func.serialize(w, state, l, text)?;
                let args = &c.arguments;
                write_int(w, args.len() as i64)?;
                for arg in &args.args {
                    arg.serialize(w, state, l, text)?;
                }
                if args.keywords.len() > 0 {
                    // TODO: Keywords
                    panic!("unsupported: {:?}", args.keywords);
                }
                write_loc(w, c.range())?;
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        Ok(())
    }
}

fn write_int(w: &mut impl Write, i: i64) -> io::Result<()> {
    if i >= MIN_SHORT_INT && i < 128 + MIN_SHORT_INT {
        // 1-byte format
        w.write_all(&[((i - MIN_SHORT_INT) << 1) as u8])
    } else if i >= MIN_TWO_BYTES_INT && i <= MAX_TWO_BYTES_INT {
        // 2-byte format
        let x: u16 = (((i - MIN_TWO_BYTES_INT) << 2) | TWO_BYTES_INT_BIT) as u16;
        w.write_all(&x.to_le_bytes())
    } else if i >= MIN_FOUR_BYTES_INT && i <= MAX_FOUR_BYTES_INT {
        // 4-byte format
        let x: u32 = (((i - MIN_FOUR_BYTES_INT) << 3) | FOUR_BYTES_INT_TRAILER) as u32;
        w.write_all(&x.to_le_bytes())
    } else {
        // Variable-length format
        w.write_all(&[LONG_INT_TRAILER])?;
        let neg = i < 0;
        let absval = if neg { i.wrapping_abs() as u64 } else { i as u64 };
        let bytes = absval.to_le_bytes();
        let mut n = bytes.len();
        while n > 1 && bytes[n - 1] == 0 {
            n -= 1;
        }
        write_int(w, ((n as i64) << 1) | (neg as i64))?;
        w.write_all(&bytes[..n])
    }
}

fn write_usize(w: &mut impl Write, i: usize) -> io::Result<()> {
    write_int(w, i as i64)
}

fn write_bytes(w: &mut impl Write, b: &[u8]) -> io::Result<()> {
    write_usize(w, b.len())?;
    w.write_all(b)
}

fn write_location<W: Write>(w: &mut W, l: &LineIndex, text: &str, range: TextRange) -> io::Result<()> {
    let st_loc = l.line_column(range.start(), text);
    write_int(w, st_loc.line.get() as i64)?;
    write_int(w, st_loc.column.get() as i64)?;
    let end_loc = l.line_column(range.end(), text);
    write_int(w, (end_loc.line.get() - st_loc.line.get()) as i64)?;
    write_int(w, end_loc.column.get() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_val(x: i64) -> u8 {
        return ((x - MIN_SHORT_INT) << 1) as u8;
    }

    #[test]
    fn test_write_short_int() {
        for x in [-10, -1, 0, 1, 117] {
            let mut v: Vec<u8> = Vec::new();
            write_int(&mut v, x).unwrap();
            assert_eq!(v, &[((x - MIN_SHORT_INT) << 1) as u8]);
        }
    }

    #[test]
    fn test_write_2_byte_int() {
        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 118).unwrap();
        assert_eq!(v, &[105, 3]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -11).unwrap();
        assert_eq!(v, &[101, 1]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -100).unwrap();
        assert_eq!(v, &[1, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 16283).unwrap();
        assert_eq!(v, &[253, 255]);
    }

    #[test]
    fn test_write_4_byte_int() {
        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -101).unwrap();
        assert_eq!(v, &[91, 53, 1, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 16284).unwrap();
        assert_eq!(v, &[99, 53, 3, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -10000).unwrap();
        assert_eq!(v, &[3, 0, 0, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 536860911).unwrap();
        assert_eq!(v, &[251, 255, 255, 255]);    
    }

    #[test]
    fn test_write_long_int() {
        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -10001).unwrap();
        assert_eq!(v, &[15, 30, 17, 39]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 536860912).unwrap();
        assert_eq!(v, &[15, 36, 240, 216, 255, 31]);
    }

    #[test]
    fn print_hello() {
        let opt = ParseOptions::from(PySourceType::Python);
        let text = "print('hello')";
        let ast = parse(text, opt).unwrap().into_syntax();
        let mut v = Vec::new();
        let index = LineIndex::from_source_text(text);
        let mut state = State { imports: Vec::new() };
        ast.serialize(&mut v, &mut state, &index, text).unwrap();
        let _ = state;  // TODO: drop when not needed

        let expected = &[
            int_val(1),
            TAG_EXPR_STMT,
            TAG_CALL_EXPR,
            TAG_NAME_EXPR,
            int_val(5),
            b'p',
            b'r',
            b'i',
            b'n',
            b't',
            int_val(1),
            int_val(1),
            int_val(0),
            int_val(6),
            int_val(1),
            TAG_STR_EXPR,
            int_val(5),
            b'h',
            b'e',
            b'l',
            b'l',
            b'o',
            int_val(1),
            int_val(7),
            int_val(0),
            int_val(14),
            int_val(1),
            int_val(1),
            int_val(0),
            int_val(15),
        ];

        assert_eq!(v, expected);
    }
}
