//! Serialize the AST for a given Python file as a mypy AST

use std::path::Path;

use anyhow::Result;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, Number};
use ruff_python_ast::{self as ast};
use ruff_python_parser::{ParseOptions, parse_unchecked};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

/// Syntax error information with location details
#[derive(Debug, Clone)]
pub struct SyntaxError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

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

const TAG_DECORATOR: u8 = 53;
const TAG_CLASS_DEF: u8 = 60;

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
const TAG_INDEX: u8 = 172;
const TAG_LIST_EXPR: u8 = 173;
const TAG_SET_EXPR: u8 = 174;
const TAG_RETURN: u8 = 175;
const TAG_WHILE: u8 = 176;
const TAG_COMPARISON_EXPR: u8 = 177;
const TAG_BOOL_OP_EXPR: u8 = 178;
const TAG_FUNC_DEF: u8 = 179;
const TAG_PASS_STMT: u8 = 180;
const TAG_FLOAT_EXPR: u8 = 181;
const TAG_UNARY_EXPR: u8 = 182;
const TAG_DICT_EXPR: u8 = 183;
const TAG_COMPLEX_EXPR: u8 = 184;
const TAG_SLICE_EXPR: u8 = 185;
const TAG_TEMP_NODE: u8 = 186;
const TAG_RAISE_STMT: u8 = 187;
const TAG_BREAK_STMT: u8 = 188;
const TAG_CONTINUE_STMT: u8 = 189;
const TAG_GENERATOR_EXPR: u8 = 190;
const TAG_YIELD_EXPR: u8 = 191;
const TAG_YIELD_FROM_EXPR: u8 = 192;
const TAG_LIST_COMPREHENSION: u8 = 193;
const TAG_SET_COMPREHENSION: u8 = 194;
const TAG_DICT_COMPREHENSION: u8 = 195;
const TAG_IMPORT_FROM: u8 = 196;
const TAG_ASSERT_STMT: u8 = 197;
const TAG_FOR_STMT: u8 = 198;
const TAG_WITH_STMT: u8 = 199;
const TAG_OPERATOR_ASSIGNMENT_STMT: u8 = 200;
const TAG_TRY_STMT: u8 = 201;
const TAG_ELLIPSIS_EXPR: u8 = 202;
const TAG_CONDITIONAL_EXPR: u8 = 203;
const TAG_DEL_STMT: u8 = 204;
const TAG_FSTRING_EXPR: u8 = 205;
const TAG_FSTRING_INTERPOLATION: u8 = 206;
const TAG_LAMBDA_EXPR: u8 = 207;
const TAG_NAMED_EXPR: u8 = 208;
const TAG_STAR_EXPR: u8 = 209;
const TAG_BYTES_EXPR: u8 = 210;
const TAG_GLOBAL_DECL: u8 = 211;
const TAG_NONLOCAL_DECL: u8 = 212;
const TAG_AWAIT_EXPR: u8 = 213;
const TAG_UNBOUND_TYPE: u8 = 104;
const TAG_UNION_TYPE: u8 = 115;
const TAG_LIST_TYPE: u8 = 118;
const TAG_ELLIPSIS_TYPE: u8 = 119;
const TAG_RAW_EXPRESSION_TYPE: u8 = 120;

// Argument kinds (must match mypy/nodes.py)
const ARG_POS: i64 = 0;        // Positional argument
const ARG_OPT: i64 = 1;        // Positional argument with default
const ARG_STAR: i64 = 2;       // *args
const ARG_NAMED: i64 = 3;      // Keyword-only argument
const ARG_STAR2: i64 = 4;      // **kwargs
const ARG_NAMED_OPT: i64 = 5;  // Keyword-only argument with default

const MIN_SHORT_INT: i64 = -10;
const MIN_TWO_BYTES_INT: i64 = -100;
const MAX_TWO_BYTES_INT: i64 = 16283;  // 2 ** (8 + 6) - 1 - 100
const MIN_FOUR_BYTES_INT: i64 = -10000;
const MAX_FOUR_BYTES_INT: i64 = 536860911;  // 2 ** (3 * 8 + 5) - 1 - 10000

const TWO_BYTES_INT_BIT: i64 = 1;
const FOUR_BYTES_INT_TRAILER: i64 = 3;
const LONG_INT_TRAILER: u8 = 15;

/// Serialize a Python file to mypy AST format.
///
/// # Arguments
///
/// * `file_path` - Path to the Python file to parse and serialize
///
/// # Returns
///
/// A tuple containing:
/// - A `Vec<u8>` with the serialized AST in mypy's binary format (may be partial if there are syntax errors)
/// - A `Vec<SyntaxError>` containing any syntax errors with line/column information
///
/// # Errors
///
/// Returns an error if the file cannot be read (but not for syntax errors, which are returned in the tuple)
pub(crate) fn serialize_python_file(file_path: &Path) -> Result<(Vec<u8>, Vec<SyntaxError>)> {
    let source_type = PySourceType::from(file_path);
    let source_kind = SourceKind::from_path(file_path, source_type)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine source kind for file: {}",
            file_path.display()
        )
    })?;

    if !source_kind.source_code().is_ascii() {
        panic!("non-ascii source not supported");
    }

    let line_index = LineIndex::from_source_text(source_kind.source_code());

    // Parse the file - this always returns a result, even with syntax errors
    let parsed = parse_unchecked(source_kind.source_code(), ParseOptions::from(source_type));

    // Extract syntax errors with location information
    let syntax_errors: Vec<SyntaxError> = parsed
        .errors()
        .iter()
        .map(|error| {
            let location = line_index.line_column(error.location.start(), source_kind.source_code());
            SyntaxError {
                line: location.line.get(),
                column: location.column.get(),
                message: error.error.to_string(),
            }
        })
        .collect();

    // Serialize the AST (even if partial due to syntax errors)
    let mut ser = Serializer {
        bytes: Vec::new(),
        imports: Vec::new(),
        import_froms: Vec::new(),
        line_index,
        text: source_kind.source_code()
    };
    parsed.syntax().serialize(&mut ser);

    Ok((ser.bytes, syntax_errors))
}

// Used to report which imports are used in a file
struct Import {
    name: String,
    relative: i32,  // Number of dots in relative import 'import ..x'
    as_name: Option<String>,  // Set for 'import x as y'
}

// Used to report which from...import statements are used in a file
struct ImportFrom {
    module: String,  // Module being imported from (empty string for "from . import x")
    relative: i32,   // Number of dots in relative import
    names: Vec<(String, Option<String>)>,  // List of (name, as_name) tuples
}

struct Serializer<'a> {
    bytes: Vec<u8>,
    imports: Vec<Import>,  // Encountered import statements
    import_froms: Vec<ImportFrom>,  // Encountered from...import statements
    line_index: LineIndex,
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
        self.write_int(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.write_int(i as i64);
    }

    fn write_bytes(&mut self, b: &[u8]) {
        self.write_tag(TAG_LITERAL_STR);
        self.write_usize(b.len());
        self.bytes.extend_from_slice(b);
    }

    fn write_bool(&mut self, b: bool) {
        self.bytes.push(if b { TAG_LITERAL_TRUE } else { TAG_LITERAL_FALSE });
    }

    fn write_int(&mut self, i: i64) {
        if i >= MIN_SHORT_INT && i < 128 + MIN_SHORT_INT {
            // 1-byte format
            self.bytes.push(((i - MIN_SHORT_INT) << 1) as u8);
        } else if i >= MIN_TWO_BYTES_INT && i <= MAX_TWO_BYTES_INT {
            // 2-byte format
            let x: u16 = (((i - MIN_TWO_BYTES_INT) << 2) | TWO_BYTES_INT_BIT) as u16;
            self.bytes.extend_from_slice(&x.to_le_bytes());
        } else if i >= MIN_FOUR_BYTES_INT && i <= MAX_FOUR_BYTES_INT {
            // 4-byte format
            let x: u32 = (((i - MIN_FOUR_BYTES_INT) << 3) | FOUR_BYTES_INT_TRAILER) as u32;
            self.bytes.extend_from_slice(&x.to_le_bytes());
        } else {
            // Variable-length format
            self.bytes.push(LONG_INT_TRAILER);
            let neg = i < 0;
            let absval = if neg { i.wrapping_abs() as u64 } else { i as u64 };
            let bytes = absval.to_le_bytes();
            let mut n = bytes.len();
            while n > 1 && bytes[n - 1] == 0 {
                n -= 1;
            }
            self.write_int(((n as i64) << 1) | (neg as i64));
            self.bytes.extend_from_slice(&bytes[..n]);
        }
    }

    fn write_location(&mut self, range: TextRange) {
        self.write_tag(TAG_LOCATION);
        let st_loc = self.line_index.line_column(range.start(), self.text);
        let st_line = st_loc.line.get() as i64;
        let st_column = st_loc.column.get() as i64;
        self.write_int(st_line);
        self.write_int(st_column);
        let end_loc = self.line_index.line_column(range.end(), self.text);
        self.write_int((end_loc.line.get() as i64) - st_line);
        self.write_int((end_loc.column.get() as i64) - st_column);
    }

    fn serialize_block(&mut self, block: &Vec<ast::Stmt>) {
        self.write_tag(TAG_BLOCK);
        self.write_tag(TAG_LIST_GEN);
        self.write_usize(block.len());
        for stmt in block {
            stmt.serialize(self);
        }
        self.write_end_tag();
    }
}

trait Ser {
    fn serialize(&self, ser: &mut Serializer);
}

impl Ser for Vec<ast::Expr> {
    fn serialize(&self, ser: &mut Serializer) {
        ser.write_tag(TAG_LIST_GEN);
        ser.write_int(self.len() as i64);
        for e in self {
            e.serialize(ser);
        }
    }
}

impl Ser for [ast::Expr] {
    fn serialize(&self, ser: &mut Serializer) {
        ser.write_tag(TAG_LIST_GEN);
        ser.write_int(self.len() as i64);
        for e in self {
            e.serialize(ser);
        }
    }
}

impl Ser for Option<Box<ast::Expr>> {
    fn serialize(&self, ser: &mut Serializer) {
        if let Some(v) = &self {
            ser.write_bool(true);
            v.serialize(ser);
        } else {
            ser.write_bool(false);
        }
    }
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

/// Helper function to serialize comprehensions (shared by Generator, ListComp, SetComp)
fn serialize_comprehension(
    ser: &mut Serializer,
    elt: &ast::Expr,
    generators: &[ast::Comprehension],
    range: ruff_text_size::TextRange,
) {
    // Serialize element expression
    elt.serialize(ser);
    // Serialize number of generators
    ser.write_tagged_int(generators.len() as i64);
    // Serialize all indices (targets)
    for comp in generators {
        comp.target.serialize(ser);
    }
    // Serialize all sequences (iters)
    for comp in generators {
        comp.iter.serialize(ser);
    }
    // Serialize all condlists (ifs for each generator)
    for comp in generators {
        comp.ifs.serialize(ser);
    }
    // Serialize all is_async flags
    for comp in generators {
        ser.write_bool(comp.is_async);
    }
    ser.write_location(range);
}

fn serialize_parameters(ser: &mut Serializer, params: &ast::Parameters) {
    // Count total number of arguments
    let mut arg_count = 0;
    arg_count += params.posonlyargs.len();
    arg_count += params.args.len();
    if params.vararg.is_some() {
        arg_count += 1;
    }
    arg_count += params.kwonlyargs.len();
    if params.kwarg.is_some() {
        arg_count += 1;
    }

    // Write argument list
    ser.write_tag(TAG_LIST_GEN);
    ser.write_int(arg_count as i64);

    // Serialize positional-only arguments
    for param in &params.posonlyargs {
        serialize_argument(ser, &param.parameter, param.default.as_deref(), ARG_POS, ARG_OPT, true);
    }

    // Serialize regular positional arguments
    for param in &params.args {
        serialize_argument(ser, &param.parameter, param.default.as_deref(), ARG_POS, ARG_OPT, false);
    }

    // Serialize *args
    if let Some(vararg) = &params.vararg {
        serialize_argument(ser, vararg, None, ARG_STAR, ARG_STAR, false);
    }

    // Serialize keyword-only arguments
    for param in &params.kwonlyargs {
        serialize_argument(ser, &param.parameter, param.default.as_deref(), ARG_NAMED, ARG_NAMED_OPT, false);
    }

    // Serialize **kwargs
    if let Some(kwarg) = &params.kwarg {
        serialize_argument(ser, kwarg, None, ARG_STAR2, ARG_STAR2, false);
    }
}

fn serialize_argument(
    ser: &mut Serializer,
    param: &ast::Parameter,
    default_expr: Option<&ast::Expr>,
    kind_no_default: i64,
    kind_with_default: i64,
    pos_only: bool,
) {
    // Argument name
    ser.write_bytes(param.name.as_bytes());

    // Argument kind
    let kind = if default_expr.is_some() {
        kind_with_default
    } else {
        kind_no_default
    };
    ser.write_tagged_int(kind);

    if let Some(ann) = &param.annotation {
        ser.write_bool(true);
        serialize_type(ser, ann);
    } else {
        ser.write_bool(false);
    }

    // Default value
    if let Some(expr) = default_expr {
        ser.write_bool(true);
        expr.serialize(ser);
    } else {
        ser.write_bool(false);
    }

    // pos_only flag
    ser.write_bool(pos_only);
}

fn serialize_simple_unbound_type(ser: &mut Serializer, name: &[u8]) {
    ser.write_tag(TAG_UNBOUND_TYPE);
    ser.write_bytes(name);
    ser.write_tag(TAG_LIST_GEN);
    ser.write_int(0);
    // Write None for original_str_expr (optional field)
    ser.write_tag(TAG_LITERAL_NONE);
    // Write None for original_str_fallback (optional field)
    ser.write_tag(TAG_LITERAL_NONE);
}

fn get_qualified_type_name(v: &mut Vec<u8>, e: &ast::Expr) {
    match e {
        ast::Expr::Name(e) => {
            v.extend_from_slice(e.id.as_bytes());
        }
        ast::Expr::Attribute(e) => {
            get_qualified_type_name(v, &e.value);
            v.extend_from_slice(b".");
            v.extend_from_slice(e.attr.as_bytes());
        }
        _ => {
            panic!("unimplemented")
        }
    }
}

fn serialize_type(ser: &mut Serializer, t: &ast::Expr) {
    match t {
        ast::Expr::Name(e) => {
            serialize_simple_unbound_type(ser, e.id.as_bytes());
        }
        ast::Expr::Attribute(e) => {
            ser.write_tag(TAG_UNBOUND_TYPE);
            let mut v = Vec::new();
            get_qualified_type_name(&mut v, &t);
            ser.write_bytes(&v);
            ser.write_tag(TAG_LIST_GEN);
            ser.write_int(0);
            // Write None for original_str_expr (optional field)
            ser.write_tag(TAG_LITERAL_NONE);
            // Write None for original_str_fallback (optional field)
            ser.write_tag(TAG_LITERAL_NONE);
        }
        ast::Expr::Subscript(e) => {
            ser.write_tag(TAG_UNBOUND_TYPE);
            let mut v = Vec::new();
            get_qualified_type_name(&mut v, &e.value);
            ser.write_bytes(&v);
            ser.write_tag(TAG_LIST_GEN);
            match e.slice.as_ref() {
                ast::Expr::Tuple(t) => {
                    ser.write_usize(t.len());
                    for item in &t.elts {
                        serialize_type(ser, item);
                    }
                }
                _ => {
                    ser.write_int(1);
                    serialize_type(ser, &e.slice);
                }
            }
            // Write None for original_str_expr (optional field)
            ser.write_tag(TAG_LITERAL_NONE);
            // Write None for original_str_fallback (optional field)
            ser.write_tag(TAG_LITERAL_NONE);
        }
        ast::Expr::NoneLiteral(_) => {
            serialize_simple_unbound_type(ser, b"None");
        }
        ast::Expr::BooleanLiteral(b) => {
            // Serialize as NameExpr with "True" or "False"
            ser.write_tag(TAG_RAW_EXPRESSION_TYPE);
            ser.write_bytes(b"builtins.bool");
            ser.write_bool(b.value);
        }
        ast::Expr::BinOp(e) => {
            // Handle union types (x | y)
            if matches!(e.op, ast::Operator::BitOr) {
                ser.write_tag(TAG_UNION_TYPE);
                // Serialize items list with exactly two items (left and right)
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(2);
                serialize_type(ser, &e.left);
                serialize_type(ser, &e.right);
                // uses_pep604_syntax = true (using | operator)
                ser.write_bool(true);
            } else {
                panic!("unsupported binary operator in type: {:?}", e.op);
            }
        }
        ast::Expr::List(e) => {
            ser.write_tag(TAG_LIST_TYPE);
            // Serialize items list
            ser.write_tag(TAG_LIST_GEN);
            ser.write_int(e.elts.len() as i64);
            for item in &e.elts {
                serialize_type(ser, item);
            }
        }
        ast::Expr::EllipsisLiteral(_) => {
            ser.write_tag(TAG_ELLIPSIS_TYPE);
            // EllipsisType has no attributes
        }
        _ => {
            panic!("unsupported type: {t:?}");
        }
    }
    ser.write_location(t.range());
    ser.write_end_tag();
}

impl Ser for ast::Stmt {
    fn serialize(&self, ser: &mut Serializer) {
        match self {
            ast::Stmt::FunctionDef(f) => {
                if !f.decorator_list.is_empty() {
                    ser.write_tag(TAG_DECORATOR);
                    // Serialize decorators
                    ser.write_tag(TAG_LIST_GEN);
                    ser.write_usize(f.decorator_list.len());
                    for dec in &f.decorator_list {
                        dec.expression.serialize(ser);
                    }
                }

                ser.write_tag(TAG_FUNC_DEF);

                // Function name
                ser.write_bytes(f.name.as_bytes());

                // Arguments
                serialize_parameters(ser, &f.parameters);

                // Body
                ser.serialize_block(&f.body);

                // TODO: Decorators (skip for now)
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(0); // Empty decorator list

                // is_async
                ser.write_bool(f.is_async);

                // TODO: type_params (skip for now)
                ser.write_bool(false); // No type params

                // TODO: Return type annotation (skip for now)
                if let Some(ret) = &f.returns {
                    ser.write_bool(true); // No return annotation
                    serialize_type(ser, ret);
                } else {
                    ser.write_bool(false); // No return annotation
                }

                ser.write_location(f.range());

                if !f.decorator_list.is_empty() {
                    // Extra end tag for the Decorator wrapper in mypy AST
                    ser.write_end_tag();
                }
            }
            ast::Stmt::Expr(e) => {
                ser.write_tag(TAG_EXPR_STMT);
                e.value.serialize(ser);
            }
            ast::Stmt::Assign(a) => {
                ser.write_tag(TAG_ASSIGN);
                a.targets.serialize(ser);
                a.value.serialize(ser);
                // No type annotation
                ser.write_bool(false);
                // new_syntax = false (not using PEP 526 syntax)
                ser.write_bool(false);
                ser.write_location(a.range());
            }
            ast::Stmt::AnnAssign(a) => {
                ser.write_tag(TAG_ASSIGN);
                // Serialize target as a single-element list
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(1);
                a.target.serialize(ser);
                // Serialize value (or TempNode if annotation-only)
                if let Some(value) = &a.value {
                    value.serialize(ser);
                } else {
                    // For annotation-only (x: int), serialize as TempNode
                    ser.write_tag(TAG_TEMP_NODE);
                    ser.write_end_tag();
                }
                // has_type = true
                ser.write_bool(true);
                // Serialize type annotation
                serialize_type(ser, &a.annotation);
                // new_syntax = true (using PEP 526 syntax)
                ser.write_bool(true);
                ser.write_location(a.range());
            }
            ast::Stmt::AugAssign(a) => {
                ser.write_tag(TAG_OPERATOR_ASSIGNMENT_STMT);
                // Serialize operator as string
                ser.write_bytes(a.op.as_str().as_bytes());
                // Serialize lvalue (target)
                a.target.serialize(ser);
                // Serialize rvalue (value)
                a.value.serialize(ser);
                ser.write_location(a.range());
            }
            ast::Stmt::Import(i) => {
                ser.write_tag(TAG_IMPORT);
                // Write number of imports
                ser.write_tagged_int(i.names.len() as i64);
                for name in &i.names {
                    // Write import name
                    ser.write_bytes(name.name.as_bytes());
                    // Write as_name (optional)
                    if let Some(asname) = &name.asname {
                        ser.write_bool(true);
                        ser.write_bytes(asname.as_bytes());
                    } else {
                        ser.write_bool(false);
                    }
                    ser.imports.push(Import {
                        name: name.name.to_string(),
                        relative: 0,  // Not a relative import
                        as_name: name.asname.as_ref().map(|n| n.to_string())
                    });
                }
                ser.write_location(i.range());
            }
            ast::Stmt::ImportFrom(ifrom) => {
                ser.write_tag(TAG_IMPORT_FROM);

                // Write relative import level (number of dots)
                ser.write_tagged_int(ifrom.level as i64);

                // Write module name (empty string for "from . import x")
                ser.write_bytes(ifrom.module.as_ref().map_or(b"", |m| m.as_bytes()));

                // Write number of imported names
                ser.write_tagged_int(ifrom.names.len() as i64);

                // Collect names for dependency tracking
                let mut names = Vec::new();

                // Write each name and optional alias
                for alias in &ifrom.names {
                    ser.write_bytes(alias.name.as_bytes());
                    if let Some(asname) = &alias.asname {
                        ser.write_bool(true);
                        ser.write_bytes(asname.as_bytes());
                    } else {
                        ser.write_bool(false);
                    }

                    // Collect for dependency tracking
                    names.push((
                        alias.name.to_string(),
                        alias.asname.as_ref().map(|n| n.to_string())
                    ));
                }

                // Track in import_froms list for dependency tracking
                ser.import_froms.push(ImportFrom {
                    module: ifrom.module.as_ref().map_or(String::new(), |m| m.to_string()),
                    relative: ifrom.level as i32,
                    names,
                });

                ser.write_location(ifrom.range());
            }
            ast::Stmt::Return(s) => {
                ser.write_tag(TAG_RETURN);
                s.value.serialize(ser);
                ser.write_location(s.range());
            }
            ast::Stmt::Raise(r) => {
                ser.write_tag(TAG_RAISE_STMT);
                // Serialize exception expression (optional)
                r.exc.serialize(ser);
                // Serialize from expression (optional)
                r.cause.serialize(ser);
                ser.write_location(r.range());
            }
            ast::Stmt::Assert(a) => {
                ser.write_tag(TAG_ASSERT_STMT);
                // Serialize test expression
                a.test.serialize(ser);
                // Serialize optional message expression
                a.msg.serialize(ser);
                ser.write_location(a.range());
            }
            ast::Stmt::If(s) => {
                ser.write_tag(TAG_IF);
                s.test.serialize(ser);
                ser.serialize_block(&s.body);
                let has_else = s.elif_else_clauses.last().is_some_and(|v| v.test.is_none());
                let num_elif = s.elif_else_clauses.len() - if has_else { 1 } else { 0 };
                ser.write_tagged_int(num_elif as i64);
                for ee in &s.elif_else_clauses {
                    match &ee.test {
                        Some(e) => {
                            e.serialize(ser);
                            ser.serialize_block(&ee.body);
                        }
                        None => {
                            ser.write_bool(true);
                            ser.serialize_block(&ee.body);
                        }
                    }
                }
                if !has_else {
                    ser.write_bool(false);
                }
                ser.write_location(s.range());
            }
            ast::Stmt::While(s) => {
                ser.write_tag(TAG_WHILE);
                s.test.serialize(ser);
                ser.serialize_block(&s.body);
                ser.serialize_block(&s.orelse);
                ser.write_location(s.range());
            }
            ast::Stmt::For(f) => {
                ser.write_tag(TAG_FOR_STMT);
                // Serialize index (target)
                f.target.serialize(ser);
                // Serialize iterator expression
                f.iter.serialize(ser);
                // Serialize body
                ser.serialize_block(&f.body);
                // Serialize else clause
                ser.serialize_block(&f.orelse);
                ser.write_location(f.range());
            }
            ast::Stmt::With(w) => {
                ser.write_tag(TAG_WITH_STMT);
                // Write number of items
                ser.write_tagged_int(w.items.len() as i64);
                // Serialize each item
                for item in &w.items {
                    // Serialize context expression
                    item.context_expr.serialize(ser);
                    // Serialize optional target
                    item.optional_vars.serialize(ser);
                }
                // Serialize body
                ser.serialize_block(&w.body);
                ser.write_location(w.range());
            }
            ast::Stmt::Pass(s) => {
                ser.write_tag(TAG_PASS_STMT);
                ser.write_location(s.range());
            }
            ast::Stmt::Break(s) => {
                ser.write_tag(TAG_BREAK_STMT);
                ser.write_location(s.range());
            }
            ast::Stmt::Continue(s) => {
                ser.write_tag(TAG_CONTINUE_STMT);
                ser.write_location(s.range());
            }
            ast::Stmt::ClassDef(c) => {
                ser.write_tag(TAG_CLASS_DEF);

                // Class name
                ser.write_bytes(c.name.as_bytes());

                // Body
                ser.serialize_block(&c.body);

                // Base classes (positional arguments in class definition)
                ser.write_tag(TAG_LIST_GEN);
                if let Some(args) = &c.arguments {
                    ser.write_int(args.args.len() as i64);
                    for base in &args.args {
                        base.serialize(ser);
                    }
                } else {
                    ser.write_int(0); // No base classes
                }

                // TODO: Decorators (skip for now)
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(0); // Empty decorator list

                // TODO: Type parameters (skip for now)
                ser.write_bool(false); // No type params

                // TODO: Metaclass (skip for now)
                ser.write_bool(false); // No metaclass

                // TODO: Keywords (skip for now)
                ser.write_tag(TAG_DICT_STR_GEN);
                ser.write_int(0); // Empty keywords dict

                ser.write_location(c.range());
            }
            ast::Stmt::Try(t) => {
                ser.write_tag(TAG_TRY_STMT);

                // Serialize try body
                ser.serialize_block(&t.body);

                // Serialize number of except handlers
                ser.write_tagged_int(t.handlers.len() as i64);

                // Serialize exception types for each handler
                for handler in &t.handlers {
                    match handler {
                        ast::ExceptHandler::ExceptHandler(h) => {
                            if let Some(type_expr) = &h.type_ {
                                ser.write_bool(true);
                                type_expr.serialize(ser);
                            } else {
                                ser.write_bool(false);
                            }
                        }
                    }
                }

                // Serialize variable names for each handler
                for handler in &t.handlers {
                    match handler {
                        ast::ExceptHandler::ExceptHandler(h) => {
                            if let Some(name) = &h.name {
                                ser.write_bool(true);
                                ser.write_bytes(name.as_bytes());
                            } else {
                                ser.write_bool(false);
                            }
                        }
                    }
                }

                // Serialize handler bodies
                for handler in &t.handlers {
                    match handler {
                        ast::ExceptHandler::ExceptHandler(h) => {
                            ser.serialize_block(&h.body);
                        }
                    }
                }

                // Serialize else body (optional)
                if !t.orelse.is_empty() {
                    ser.write_bool(true);
                    ser.serialize_block(&t.orelse);
                } else {
                    ser.write_bool(false);
                }

                // Serialize finally body (optional)
                if !t.finalbody.is_empty() {
                    ser.write_bool(true);
                    ser.serialize_block(&t.finalbody);
                } else {
                    ser.write_bool(false);
                }

                ser.write_location(t.range());
            }
            ast::Stmt::Delete(d) => {
                ser.write_tag(TAG_DEL_STMT);
                // Serialize the target expression
                // If there's only one target, serialize it directly
                // If there are multiple targets, serialize as a tuple
                if d.targets.len() == 1 {
                    d.targets[0].serialize(ser);
                } else {
                    // Serialize as a tuple expression
                    ser.write_tag(TAG_TUPLE_EXPR);
                    d.targets.serialize(ser);
                    ser.write_location(d.range());
                    ser.write_end_tag();
                }
                ser.write_location(d.range());
            }
            ast::Stmt::Global(g) => {
                ser.write_tag(TAG_GLOBAL_DECL);
                // Write number of names
                ser.write_tagged_int(g.names.len() as i64);
                // Write each name
                for name in &g.names {
                    ser.write_bytes(name.as_bytes());
                }
                ser.write_location(g.range());
            }
            ast::Stmt::Nonlocal(n) => {
                ser.write_tag(TAG_NONLOCAL_DECL);
                // Write number of names
                ser.write_tagged_int(n.names.len() as i64);
                // Write each name
                for name in &n.names {
                    ser.write_bytes(name.as_bytes());
                }
                ser.write_location(n.range());
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

                // Serialize all arguments (positional + keyword + **kwargs)
                let total_args = args.args.len() + args.keywords.len();
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(total_args as i64);
                for arg in &args.args {
                    // Unwrap starred expressions
                    match arg {
                        ast::Expr::Starred(starred) => starred.value.serialize(ser),
                        _ => arg.serialize(ser),
                    }
                }
                for kwarg in &args.keywords {
                    kwarg.value.serialize(ser);
                }

                // Serialize argument kinds
                ser.write_tag(TAG_LIST_INT);
                ser.write_int(total_args as i64);
                for arg in &args.args {
                    match arg {
                        ast::Expr::Starred(_) => ser.write_int(ARG_STAR),
                        _ => ser.write_int(ARG_POS),
                    }
                }
                for kwarg in &args.keywords {
                    if kwarg.arg.is_none() {
                        ser.write_int(ARG_STAR2);  // **kwargs
                    } else {
                        ser.write_int(ARG_NAMED);  // keyword arg
                    }
                }

                // Serialize argument names
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(total_args as i64);
                for _ in &args.args {
                    ser.write_tag(TAG_LITERAL_NONE);
                }
                for kwarg in &args.keywords {
                    if let Some(arg_name) = &kwarg.arg {
                        ser.write_bytes(arg_name.as_bytes());
                    } else {
                        ser.write_tag(TAG_LITERAL_NONE);
                    }
                }

                ser.write_location(c.range());
            }
            ast::Expr::BinOp(e) => {
                ser.write_tag(TAG_OP_EXPR);
                ser.write_tagged_int(e.op as i64);
                e.left.serialize(ser);
                e.right.serialize(ser);
            }
            ast::Expr::NumberLiteral(num) => {
                match &num.value {
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
                    Number::Float(f) => {
                        ser.write_tag(TAG_FLOAT_EXPR);
                        ser.write_tag(TAG_LITERAL_FLOAT);
                        ser.bytes.extend_from_slice(&f.to_le_bytes());
                    }
                    Number::Complex { real, imag } => {
                        ser.write_tag(TAG_COMPLEX_EXPR);
                        // Serialize real part
                        ser.write_tag(TAG_LITERAL_FLOAT);
                        ser.bytes.extend_from_slice(&real.to_le_bytes());
                        // Serialize imaginary part
                        ser.write_tag(TAG_LITERAL_FLOAT);
                        ser.bytes.extend_from_slice(&imag.to_le_bytes());
                    }
                }
                ser.write_location(num.range());
            }
            ast::Expr::Subscript(e) => {
                ser.write_tag(TAG_INDEX);
                e.value.serialize(ser);
                e.slice.serialize(ser);
                ser.write_location(e.range());
            }
            ast::Expr::List(e) => {
                ser.write_tag(TAG_LIST_EXPR);
                e.elts.serialize(ser);
                ser.write_location(e.range());
            }
            ast::Expr::Tuple(e) => {
                ser.write_tag(TAG_TUPLE_EXPR);
                e.elts.serialize(ser);
                ser.write_location(e.range());
            }
            ast::Expr::Set(e) => {
                ser.write_tag(TAG_SET_EXPR);
                e.elts.serialize(ser);
                ser.write_location(e.range());
            }
            ast::Expr::Generator(g) => {
                ser.write_tag(TAG_GENERATOR_EXPR);
                serialize_comprehension(ser, &g.elt, &g.generators, g.range());
            }
            ast::Expr::ListComp(lc) => {
                ser.write_tag(TAG_LIST_COMPREHENSION);
                serialize_comprehension(ser, &lc.elt, &lc.generators, lc.range());
            }
            ast::Expr::SetComp(sc) => {
                ser.write_tag(TAG_SET_COMPREHENSION);
                serialize_comprehension(ser, &sc.elt, &sc.generators, sc.range());
            }
            ast::Expr::DictComp(dc) => {
                ser.write_tag(TAG_DICT_COMPREHENSION);
                // Serialize key expression
                dc.key.serialize(ser);
                // Serialize value expression
                dc.value.serialize(ser);
                // Serialize number of generators
                ser.write_tagged_int(dc.generators.len() as i64);
                // Serialize all indices (targets)
                for comp in &dc.generators {
                    comp.target.serialize(ser);
                }
                // Serialize all sequences (iters)
                for comp in &dc.generators {
                    comp.iter.serialize(ser);
                }
                // Serialize all condlists (ifs for each generator)
                for comp in &dc.generators {
                    comp.ifs.serialize(ser);
                }
                // Serialize all is_async flags
                for comp in &dc.generators {
                    ser.write_bool(comp.is_async);
                }
                ser.write_location(dc.range());
            }
            ast::Expr::Yield(y) => {
                ser.write_tag(TAG_YIELD_EXPR);
                // Serialize optional value expression
                y.value.serialize(ser);
                ser.write_location(y.range());
            }
            ast::Expr::YieldFrom(y) => {
                ser.write_tag(TAG_YIELD_FROM_EXPR);
                // Serialize value expression (required for yield from)
                y.value.serialize(ser);
                ser.write_location(y.range());
            }
            ast::Expr::BoolOp(e) => {
                ser.write_tag(TAG_BOOL_OP_EXPR);
                ser.write_tagged_int(match e.op {
                    ast::BoolOp::And => 0,
                    ast::BoolOp::Or => 1,
                });
                e.values.serialize(ser);
                ser.write_location(e.range());
            }
            ast::Expr::Compare(e) => {
                ser.write_tag(TAG_COMPARISON_EXPR);
                e.left.serialize(ser);
                // Serialize operators
                ser.write_tag(TAG_LIST_INT);
                ser.write_usize(e.ops.len());
                for op in &e.ops {
                    ser.write_int(match op {
                        ast::CmpOp::Eq => 0,
                        ast::CmpOp::NotEq => 1,
                        ast::CmpOp::Lt => 2,
                        ast::CmpOp::LtE => 3,
                        ast::CmpOp::Gt => 4,
                        ast::CmpOp::GtE => 5,
                        ast::CmpOp::Is => 6,
                        ast::CmpOp::IsNot => 7,
                        ast::CmpOp::In => 8,
                        ast::CmpOp::NotIn => 9,
                    });
                }
                // Serialize comparators
                e.comparators.serialize(ser);
                ser.write_location(e.range());
            }
            ast::Expr::BooleanLiteral(b) => {
                // Serialize as NameExpr with "True" or "False"
                ser.write_tag(TAG_NAME_EXPR);
                ser.write_bytes(if b.value { b"True" } else { b"False" });
                ser.write_location(b.range());
            }
            ast::Expr::NoneLiteral(n) => {
                // Serialize as NameExpr with "None"
                ser.write_tag(TAG_NAME_EXPR);
                ser.write_bytes(b"None");
                ser.write_location(n.range());
            }
            ast::Expr::EllipsisLiteral(e) => {
                ser.write_tag(TAG_ELLIPSIS_EXPR);
                ser.write_location(e.range());
            }
            ast::Expr::If(i) => {
                ser.write_tag(TAG_CONDITIONAL_EXPR);
                // Serialize if_expr (body - value when condition is true)
                i.body.serialize(ser);
                // Serialize cond (test - the condition)
                i.test.serialize(ser);
                // Serialize else_expr (orelse - value when condition is false)
                i.orelse.serialize(ser);
                ser.write_location(i.range());
            }
            ast::Expr::UnaryOp(u) => {
                ser.write_tag(TAG_UNARY_EXPR);
                // Serialize operator as integer
                ser.write_tagged_int(u.op as i64);
                // Serialize operand
                u.operand.serialize(ser);
                ser.write_location(u.range());
            }
            ast::Expr::Dict(d) => {
                ser.write_tag(TAG_DICT_EXPR);
                // Serialize keys
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(d.items.len() as i64);
                for item in &d.items {
                    if let Some(key) = &item.key {
                        ser.write_bool(true);
                        key.serialize(ser);
                    } else {
                        // Dict unpacking: {**other_dict}
                        ser.write_bool(false);
                    }
                }
                // Serialize values
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(d.items.len() as i64);
                for item in &d.items {
                    item.value.serialize(ser);
                }
                ser.write_location(d.range());
            }
            ast::Expr::Slice(s) => {
                ser.write_tag(TAG_SLICE_EXPR);
                // Serialize lower (begin_index in mypy)
                s.lower.serialize(ser);
                // Serialize upper (end_index in mypy)
                s.upper.serialize(ser);
                // Serialize step (stride in mypy)
                s.step.serialize(ser);
                ser.write_location(s.range());
            }
            ast::Expr::FString(fs) => {
                ser.write_tag(TAG_FSTRING_EXPR);
                ser.write_tagged_int(fs.value.iter().len() as i64);
                for part in fs.value.iter() {
                    match part {
                        ast::FStringPart::FString(fstring_part) => {
                            ser.write_bool(true);
                            serialize_fstring_elements(ser, fstring_part.elements.iter().collect());
                        }
                        ast::FStringPart::Literal(lit) => {
                            ser.write_bool(false);
                            ser.write_bytes(lit.value.as_bytes());
                            ser.write_location(lit.range());
                        }
                    }
                }
                ser.write_location(fs.range());
            }
            ast::Expr::Lambda(lambda) => {
                ser.write_tag(TAG_LAMBDA_EXPR);

                // Arguments (parameters)
                if let Some(params) = &lambda.parameters {
                    serialize_parameters(ser, params);
                } else {
                    // No parameters - empty argument list
                    ser.write_tag(TAG_LIST_GEN);
                    ser.write_int(0);
                }

                // Body - lambda body is a single expression, wrap in return statement
                // Serialize as a block containing a single return statement
                ser.write_tag(TAG_BLOCK);
                ser.write_tag(TAG_LIST_GEN);
                ser.write_int(1); // One statement (the return)

                ser.write_tag(TAG_RETURN);
                // Return statement has an optional value, we always have a value for lambda
                ser.write_bool(true);
                lambda.body.serialize(ser);
                ser.write_location(lambda.body.range());
                ser.write_end_tag(); // End of return statement

                ser.write_end_tag(); // End of block

                ser.write_location(lambda.range());
            }
            ast::Expr::Named(named) => {
                ser.write_tag(TAG_NAMED_EXPR);
                // Serialize target expression
                named.target.serialize(ser);
                // Serialize value expression
                named.value.serialize(ser);
                ser.write_location(named.range());
            }
            ast::Expr::Starred(starred) => {
                ser.write_tag(TAG_STAR_EXPR);
                // Serialize the wrapped expression
                starred.value.serialize(ser);
                ser.write_location(starred.range());
            }
            ast::Expr::BytesLiteral(bytes_lit) => {
                ser.write_tag(TAG_BYTES_EXPR);
                // Convert bytes to string representation with escape sequences
                let mut result = Vec::new();
                for bytes_part in bytes_lit.value.iter() {
                    for &byte in bytes_part.value.iter() {
                        match byte {
                            b'\r' => result.extend_from_slice(b"\\r"),
                            b'\n' => result.extend_from_slice(b"\\n"),
                            b'\t' => result.extend_from_slice(b"\\t"),
                            b'\\' => result.extend_from_slice(b"\\\\"),
                            b'\'' => result.extend_from_slice(b"\\'"),
                            // Printable ASCII characters (space to ~)
                            32..=126 => result.push(byte),
                            // Everything else as hex escape
                            _ => {
                                result.extend_from_slice(b"\\x");
                                result.push(b"0123456789abcdef"[(byte >> 4) as usize]);
                                result.push(b"0123456789abcdef"[(byte & 0xf) as usize]);
                            }
                        }
                    }
                }
                ser.write_bytes(&result);
                ser.write_location(bytes_lit.range());
            }
            ast::Expr::Await(await_expr) => {
                ser.write_tag(TAG_AWAIT_EXPR);
                // Serialize the awaited expression
                await_expr.value.serialize(ser);
                ser.write_location(await_expr.range());
            }
            _ => {
                panic!("unsupported: {self:?}");
            }
        };
        ser.write_end_tag()
    }
}

fn serialize_fstring_elements(ser: &mut Serializer, elems: Vec<&ast::InterpolatedStringElement>) {
    ser.write_tagged_int(elems.len() as i64);
    for elem in elems {
        match elem {
            ast::InterpolatedStringElement::Literal(lit) => {
                ser.write_bytes(lit.value.as_bytes());
                ser.write_location(lit.range());
            }
            ast::InterpolatedStringElement::Interpolation(interp) => {
                ser.write_tag(TAG_FSTRING_INTERPOLATION);
                interp.expression.serialize(ser);
                match interp.conversion {
                    ast::ConversionFlag::None => {
                        ser.write_bool(false);
                    }
                    ast::ConversionFlag::Str => {
                        // !s conversion: f"{name!s}"
                        ser.write_bool(true);
                        ser.write_bytes(b"!s");
                    }
                    ast::ConversionFlag::Repr => {
                        // !r conversion: f"{name!r}"
                        ser.write_bool(true);
                        ser.write_bytes(b"!r");
                    }
                    ast::ConversionFlag::Ascii => {
                        // !a conversion: f"{name!a}"
                        ser.write_bool(true);
                        ser.write_bytes(b"!a");
                    }
                }
                if let Some(format_spec) = &interp.format_spec {
                    ser.write_bool(true);
                    serialize_fstring_elements(ser, format_spec.elements.iter().collect());
                    ser.write_location(format_spec.range());
                } else {
                    ser.write_bool(false);
                }
                ser.write_end_tag();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_val(x: i64) -> u8 {
        return ((x - MIN_SHORT_INT) << 1) as u8;
    }

    fn make_ser<'a>(text: &'a str) -> Serializer<'a> {
        let index = LineIndex::from_source_text(text);
        Serializer { bytes: Vec::new(), imports: Vec::new(), import_froms: Vec::new(), line_index: index, text: text }
    }

    #[test]
    fn test_write_short_int() {
        for x in [-10, -1, 0, 1, 117] {
            let mut ser = make_ser("");
            ser.write_int(x);
            assert_eq!(ser.bytes, &[((x - MIN_SHORT_INT) << 1) as u8]);
        }
    }

    #[test]
    fn test_write_2_byte_int() {
        let mut ser = make_ser("");
        ser.write_int(118);
        assert_eq!(ser.bytes, &[105, 3]);

        let mut ser = make_ser("");
        ser.write_int(-11);
        assert_eq!(ser.bytes, &[101, 1]);

        let mut ser = make_ser("");
        ser.write_int(-100);
        assert_eq!(ser.bytes, &[1, 0]);

        let mut ser = make_ser("");
        ser.write_int(16283);
        assert_eq!(ser.bytes, &[253, 255]);
    }

    #[test]
    fn test_write_4_byte_int() {
        let mut ser = make_ser("");
        ser.write_int(-101);
        assert_eq!(ser.bytes, &[91, 53, 1, 0]);

        let mut ser = make_ser("");
        ser.write_int(16284);
        assert_eq!(ser.bytes, &[99, 53, 3, 0]);

        let mut ser = make_ser("");
        ser.write_int(-10000);
        assert_eq!(ser.bytes, &[3, 0, 0, 0]);

        let mut ser = make_ser("");
        ser.write_int(536860911);
        assert_eq!(ser.bytes, &[251, 255, 255, 255]);
    }

    #[test]
    fn test_write_long_int() {
        let mut ser = make_ser("");
        ser.write_int(-10001);
        assert_eq!(ser.bytes, &[15, 30, 17, 39]);

        let mut ser = make_ser("");
        ser.write_int(536860912);
        assert_eq!(ser.bytes, &[15, 36, 240, 216, 255, 31]);
    }

    #[test]
    fn print_hello() {
        let opt = ParseOptions::from(PySourceType::Python);
        let text = "print('hello')";
        let ast = parse(text, opt).unwrap().into_syntax();
        let index = LineIndex::from_source_text(text);
        let mut ser = Serializer { bytes: Vec::new(), imports: Vec::new(), import_froms: Vec::new(), line_index: index, text: text };
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
            TAG_LIST_INT,
            int_val(1),
            int_val(0),  // ARG_POS
            TAG_LIST_GEN,
            int_val(1),
            TAG_LITERAL_NONE,
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
