//! Serialize the AST for a given Python file as a mypy AST

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::PySourceType;
use ruff_python_ast::{self as ast};
use ruff_python_parser::{ParseOptions, parse};

const TAG_EXPR_STMT: u8 = 11;
const TAG_CALL_EXPR: u8 = 12;
const TAG_NAME_EXPR: u8 = 13;
const TAG_STR_EXPR: u8 = 14;

const MIN_SHORT_INT: i64 = -10;

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
    python_ast.serialize(&mut v).unwrap();

    io::stdout().write_all(&v)?;

    Ok(())
}

trait Ser {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()>;
}

impl Ser for ast::Mod {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match self {
            ast::Mod::Module(m) => {
                for stmt in &m.body {
                    stmt.serialize(w)?;
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
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match self {
            ast::Stmt::Expr(e) => {
                w.write(&[TAG_EXPR_STMT])?;
                // TODO: Write type tag
                e.value.serialize(w)?;
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        Ok(())
    }
}

impl Ser for ast::Expr {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match self {
            ast::Expr::Name(n) => {
                w.write(&[TAG_NAME_EXPR])?;
                write_bytes(w, n.id.as_bytes())?;
            }
            ast::Expr::StringLiteral(s) => {
                w.write(&[TAG_STR_EXPR])?;
                let value = &s.value;
                write_usize(w, value.len())?;
                for part in value.iter() {
                    w.write(part.as_bytes())?;
                }
            }
            ast::Expr::Call(c) => {
                w.write(&[TAG_CALL_EXPR])?;
                c.func.serialize(w)?;
                let args = &c.arguments;
                write_int(w, args.len() as i64)?;
                for arg in &args.args {
                    arg.serialize(w)?;
                }
                if args.keywords.len() > 0 {
                    // TODO: Keywords
                    panic!("unsupported: {:?}", args.keywords);
                }
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        Ok(())
    }
}

fn write_int(w: &mut impl Write, i: i64) -> io::Result<usize> {
    // TODO: Also support cases that don't fit into 1 byte
    w.write(&[((i - MIN_SHORT_INT) << 1) as u8])
}

fn write_usize(w: &mut impl Write, i: usize) -> io::Result<usize> {
    // TODO: Longer than 127 characters
    w.write(&[(i << 1) as u8])
}

fn write_bytes(w: &mut impl Write, b: &[u8]) -> io::Result<()> {
    write_usize(w, b.len())?;
    w.write(b)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_hello() {
        let opt = ParseOptions::from(PySourceType::Python);
        let ast = parse("print('hello')", opt).unwrap().into_syntax();
        let mut v = Vec::new();
        ast.serialize(&mut v).unwrap();

        let expected = &[
            TAG_EXPR_STMT,
            TAG_CALL_EXPR,
            TAG_NAME_EXPR,
            10,
            b'p',
            b'r',
            b'i',
            b'n',
            b't',
            22,
            TAG_STR_EXPR,
            10,
            b'h',
            b'e',
            b'l',
            b'l',
            b'o',
        ];

        assert_eq!(v, expected);
    }
}
