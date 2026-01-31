use ruff_python_ast as ast;

/// Inferred truth value of an expression during reachability analysis.
///
/// These values match the constants in mypy.reachability:
/// - ALWAYS_TRUE: Expression is always true
/// - MYPY_TRUE: True in mypy, False at runtime
/// - ALWAYS_FALSE: Expression is always false
/// - MYPY_FALSE: False in mypy, True at runtime
/// - TRUTH_VALUE_UNKNOWN: Truth value cannot be determined
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TruthValue {
    AlwaysTrue = 1,
    MypyTrue = 2,
    AlwaysFalse = 3,
    MypyFalse = 4,
    TruthValueUnknown = 5,
}

impl TruthValue {
    /// Returns the inverted truth value (for handling `not` expressions).
    pub fn invert(self) -> Self {
        match self {
            TruthValue::AlwaysTrue => TruthValue::AlwaysFalse,
            TruthValue::AlwaysFalse => TruthValue::AlwaysTrue,
            TruthValue::MypyTrue => TruthValue::MypyFalse,
            TruthValue::MypyFalse => TruthValue::MypyTrue,
            TruthValue::TruthValueUnknown => TruthValue::TruthValueUnknown,
        }
    }
}

/// Consider whether expr is a comparison involving sys.version_info.
pub fn consider_sys_version_info(
    _expr: &ast::Expr,
    _python_version: (u32, u32),
) -> TruthValue {
    // TODO: Implement sys.version_info inference
    TruthValue::TruthValueUnknown
}

/// Consider whether expr is a comparison involving sys.platform.
pub fn consider_sys_platform(_expr: &ast::Expr, _platform: &str) -> TruthValue {
    // TODO: Implement sys.platform inference
    TruthValue::TruthValueUnknown
}

/// Infer whether the given condition is always true/false.
pub fn infer_condition_value(
    _expr: &ast::Expr,
    _python_version: (u32, u32),
    _platform: &str,
    _always_true: &[String],
    _always_false: &[String],
) -> TruthValue {
    // TODO: Implement condition value inference
    TruthValue::TruthValueUnknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_values() {
        // Verify the enum values match the Python constants
        assert_eq!(TruthValue::AlwaysTrue as u8, 1);
        assert_eq!(TruthValue::MypyTrue as u8, 2);
        assert_eq!(TruthValue::AlwaysFalse as u8, 3);
        assert_eq!(TruthValue::MypyFalse as u8, 4);
        assert_eq!(TruthValue::TruthValueUnknown as u8, 5);
    }

    #[test]
    fn test_invert() {
        assert_eq!(TruthValue::AlwaysTrue.invert(), TruthValue::AlwaysFalse);
        assert_eq!(TruthValue::AlwaysFalse.invert(), TruthValue::AlwaysTrue);
        assert_eq!(TruthValue::MypyTrue.invert(), TruthValue::MypyFalse);
        assert_eq!(TruthValue::MypyFalse.invert(), TruthValue::MypyTrue);
        assert_eq!(
            TruthValue::TruthValueUnknown.invert(),
            TruthValue::TruthValueUnknown
        );
    }

    #[test]
    fn test_infer_condition_value_placeholder() {
        use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};

        // Parse a simple expression
        let code = "foo";
        let parsed = parse_unchecked(code, ParseOptions::from(Mode::Expression));
        let ast::Mod::Expression(expr_mod) = parsed.into_syntax() else {
            panic!("Expected expression");
        };

        // Call infer_condition_value with the parsed expression
        let result = infer_condition_value(
            &expr_mod.body,
            (3, 10), // Python 3.10
            "linux",
            &[],
            &[],
        );

        assert_eq!(result, TruthValue::TruthValueUnknown);
    }
}
