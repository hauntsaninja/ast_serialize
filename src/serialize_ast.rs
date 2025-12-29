//! Serialize the AST for a given Python file as a mypy AST

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, Number};
use ruff_python_ast::{self as ast};
use ruff_python_parser::{ParseOptions, parse};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

// Fixed tags for primitive types (must match mypy/cache.py)
const TAG_LITERAL_FALSE: u8   = 0;
const TAG_LITERAL_TRUE: u8    = 1;
const TAG_LITERAL_NONE: u8    = 2;
const TAG_LITERAL_INT: u8     = 3;
const TAG_LITERAL_STR: u8     = 4;
const TAG_LITERAL_BYTES: u8   = 5;
const TAG_LITERAL_FLOAT: u8   = 6;
const TAG_LITERAL_COMPLEX: u8 = 7;

// Fixed tags for collections (must match mypy/cache.py)
const TAG_LIST_GEN: u8      = 20;
const TAG_LIST_INT: u8      = 21;
const TAG_LIST_STR: u8      = 22;
const TAG_LIST_BYTES: u8    = 23;
const TAG_DICT_STR_GEN: u8  = 30;

// End tag for composite objects
const TAG_END: u8 = 255;

const TAG_LOCATION: u8 = 152;
const TAG_EXPR_STMT: u8 = 160;
const TAG_CALL_EXPR: u8 = 161;
const TAG_NAME_EXPR: u8 = 162;
const TAG_STR_EXPR: u8 = 163;
const TAG_IMPORT: u8 = 164;
const TAG_MEMBER_EXPR: u8 = 165;
const TAG_OP_EXPR: u8 = 166;
const TAG_INT_EXPR: u8 = 167;
const TAG_IF: u8 = 168;
const TAG_ASSIGN: u8 = 169;
const TAG_TUPLE_EXPR: u8 = 170;
const TAG_BLOCK: u8 = 171;

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
    let line_index = LineIndex::from_source_text(source_kind.source_code());
    let mut ser = Serializer { bytes: Vec::new(), imports: Vec::new(), line_index: &line_index, text: source_kind.source_code() };
    python_ast.serialize(&mut ser);

    io::stdout().write_all(&ser.bytes)?;

    Ok(())
}

struct Import {
    name: String,
    relative: i32,
    as_name: Option<String>,
}

struct Serializer<'a> {
    bytes: Vec<u8>,
    imports: Vec<Import>,
    line_index: & 'a LineIndex,
    text: & 'a str
}

impl<'a> Serializer<'a> {
    #[inline]
    fn write_tag(&mut self, i: u8) {
        self.bytes.push(i);
    }

    #[inline]
    fn write_end_tag(&mut self) {
        self.write_tag(TAG_END);
    }

    #[inline]
    fn write_tagged_int(&mut self, i: i64) {
        self.write_tag(TAG_LITERAL_INT);
        write_int(&mut self.bytes, i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        write_int(&mut self.bytes, i as i64);
    }

    fn write_bytes(&mut self, b: &[u8]) {
        self.write_tag(TAG_LITERAL_STR);
        self.write_usize(b.len());
        self.bytes.extend_from_slice(b);
    }

    fn write_location(&mut self, range: TextRange) {
        self.write_tag(TAG_LOCATION);
        let st_loc = self.line_index.line_column(range.start(), self.text);
        let st_line = st_loc.line.get() as i64;
        let st_column = st_loc.column.get() as i64;
        write_int(&mut self.bytes, st_line);
        write_int(&mut self.bytes, st_column);
        let end_loc = self.line_index.line_column(range.end(), self.text);
        write_int(&mut self.bytes, (end_loc.line.get() as i64) - st_line);
        write_int(&mut self.bytes, (end_loc.column.get() as i64) - st_column);
    }
}

trait Ser {
    fn serialize(&self, ser: &mut Serializer);
}

impl Ser for ast::Mod {
    fn serialize(&self, ser: &mut Serializer) {
        match self {
            ast::Mod::Module(m) => {
                ser.write_tagged_int(m.body.len() as i64);
                for stmt in &m.body {
                    stmt.serialize(ser);
                }
            }
            ast::Mod::Expression(_) => {
                panic!("Expression unsupported");
            }
        }
    }
}

impl Ser for ast::Stmt {
    fn serialize(&self, ser: &mut Serializer) {
        match self {
            ast::Stmt::Expr(e) => {
                ser.write_tag(TAG_EXPR_STMT);
                e.value.serialize(ser);
            }
            ast::Stmt::Import(i) => {
                ser.write_tag(TAG_IMPORT);
                for name in &i.names {
                    ser.write_bytes(name.name.as_bytes());
                    ser.imports.push(Import { name: name.name.to_string(), relative: 0, as_name: None});
                }
                ser.write_location(i.range());
            }
            ast::Stmt::If(s) => {
                ser.write_tag(TAG_IF);
                s.test.serialize(ser);
                serialize_block(ser, &s.body);
                ser.write_usize(s.elif_else_clauses.len());
                for ee in &s.elif_else_clauses {
                    match &ee.test {
                        Some(e) => {
                            e.serialize(ser);
                            serialize_block(ser, &ee.body);
                        }
                        None => {
                            serialize_block(ser, &ee.body);
                        }
                    }
                }
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        ser.write_end_tag()
    }
}

impl Ser for ast::Expr {
    fn serialize(&self, ser: &mut Serializer) {
        match self {
            ast::Expr::Name(n) => {
                ser.write_tag(TAG_NAME_EXPR);
                ser.write_bytes(n.id.as_bytes());
                ser.write_location(n.range());
            }
            ast::Expr::Attribute(a) => {
                ser.write_tag(TAG_MEMBER_EXPR);
                a.value.serialize(ser);
                ser.write_bytes(a.attr.as_bytes());
                ser.write_location(a.range());
            }
            ast::Expr::StringLiteral(s) => {
                ser.write_tag(TAG_STR_EXPR);
                let value = &s.value;
                ser.write_tag(TAG_LITERAL_STR);
                ser.write_usize(value.len());
                for part in value.iter() {
                    ser.bytes.extend_from_slice(part.as_bytes());
                }
                ser.write_location(s.range());
            }
            ast::Expr::Call(c) => {
                ser.write_tag(TAG_CALL_EXPR);
                c.func.serialize(ser);
                let args = &c.arguments;
                ser.write_tag(TAG_LIST_GEN);
                write_int(&mut ser.bytes, args.len() as i64);
                for arg in &args.args {
                    arg.serialize(ser);
                }
                if args.keywords.len() > 0 {
                    // TODO: Keywords
                    panic!("unsupported: {:?}", args.keywords);
                }
                ser.write_location(c.range());
            }
            ast::Expr::BinOp(b) => {
                ser.write_tag(TAG_OP_EXPR);
                ser.bytes.push(b.op as u8);
                b.left.serialize(ser);
                b.right.serialize(ser);
            }
            ast::Expr::NumberLiteral(n) => {
                match &n.value {
                    Number::Int(n) => {
                        match n.as_i64() {
                            Some(x) => {
                                ser.write_tag(TAG_INT_EXPR);
                                ser.write_tagged_int(x);
                            }
                            _ => {
                                panic!("unsupported big int: {self:?}");
                            }
                        }
                    }
                    _ => {
                        panic!("unsupported number: {self:?}");
                    }
                }
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        ser.write_end_tag()
    }
}

fn serialize_block(ser: &mut Serializer, block: &Vec<ast::Stmt>) {
    ser.write_tag(TAG_BLOCK);
    ser.write_usize(block.len());
    for stmt in block {
        stmt.serialize(ser);
    }
    ser.write_end_tag();
}

fn write_int(w: &mut Vec<u8>, i: i64) {
    if i >= MIN_SHORT_INT && i < 128 + MIN_SHORT_INT {
        // 1-byte format
        w.push(((i - MIN_SHORT_INT) << 1) as u8);
    } else if i >= MIN_TWO_BYTES_INT && i <= MAX_TWO_BYTES_INT {
        // 2-byte format
        let x: u16 = (((i - MIN_TWO_BYTES_INT) << 2) | TWO_BYTES_INT_BIT) as u16;
        w.extend_from_slice(&x.to_le_bytes());
    } else if i >= MIN_FOUR_BYTES_INT && i <= MAX_FOUR_BYTES_INT {
        // 4-byte format
        let x: u32 = (((i - MIN_FOUR_BYTES_INT) << 3) | FOUR_BYTES_INT_TRAILER) as u32;
        w.extend_from_slice(&x.to_le_bytes());
    } else {
        // Variable-length format
        w.push(LONG_INT_TRAILER);
        let neg = i < 0;
        let absval = if neg { i.wrapping_abs() as u64 } else { i as u64 };
        let bytes = absval.to_le_bytes();
        let mut n = bytes.len();
        while n > 1 && bytes[n - 1] == 0 {
            n -= 1;
        }
        write_int(w, ((n as i64) << 1) | (neg as i64));
        w.extend_from_slice(&bytes[..n]);
    }
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
            write_int(&mut v, x);
            assert_eq!(v, &[((x - MIN_SHORT_INT) << 1) as u8]);
        }
    }

    #[test]
    fn test_write_2_byte_int() {
        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 118);
        assert_eq!(v, &[105, 3]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -11);
        assert_eq!(v, &[101, 1]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -100);
        assert_eq!(v, &[1, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 16283);
        assert_eq!(v, &[253, 255]);
    }

    #[test]
    fn test_write_4_byte_int() {
        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -101);
        assert_eq!(v, &[91, 53, 1, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 16284);
        assert_eq!(v, &[99, 53, 3, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -10000);
        assert_eq!(v, &[3, 0, 0, 0]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 536860911);
        assert_eq!(v, &[251, 255, 255, 255]);
    }

    #[test]
    fn test_write_long_int() {
        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, -10001);
        assert_eq!(v, &[15, 30, 17, 39]);

        let mut v: Vec<u8> = Vec::new();
        write_int(&mut v, 536860912);
        assert_eq!(v, &[15, 36, 240, 216, 255, 31]);
    }

    #[test]
    fn print_hello() {
        let opt = ParseOptions::from(PySourceType::Python);
        let text = "print('hello')";
        let ast = parse(text, opt).unwrap().into_syntax();
        let index = LineIndex::from_source_text(text);
        let mut ser = Serializer { bytes: Vec::new(), imports: Vec::new(), line_index: &index, text: text };
        ast.serialize(&mut ser);
        let _ = ser;  // TODO: drop when not needed

        let expected = &[
            TAG_LITERAL_INT,
            int_val(1),
            TAG_EXPR_STMT,
            TAG_CALL_EXPR,
            TAG_NAME_EXPR,
            TAG_LITERAL_STR,
            int_val(5),
            b'p',
            b'r',
            b'i',
            b'n',
            b't',
            TAG_LOCATION,
            int_val(1),
            int_val(1),
            int_val(0),
            int_val(5),
            TAG_END,
            TAG_LIST_GEN,
            int_val(1),
            TAG_STR_EXPR,
            TAG_LITERAL_STR,
            int_val(5),
            b'h',
            b'e',
            b'l',
            b'l',
            b'o',
            TAG_LOCATION,
            int_val(1),
            int_val(7),
            int_val(0),
            int_val(7),
            TAG_END,
            TAG_LOCATION,
            int_val(1),
            int_val(1),
            int_val(0),
            int_val(14),
            TAG_END,
            TAG_END,
        ];

        assert_eq!(ser.bytes, expected);
    }
}
