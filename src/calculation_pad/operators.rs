use exmex::{BinOp, MakeOperators, Operator};

#[derive(Clone, Debug)]
pub(super) struct CheckedOperators;

fn apply_unary(value: f64, operation: fn(f64) -> f64) -> f64 {
    if !value.is_finite() {
        return f64::NAN;
    }
    let result = operation(value);
    if result.is_finite() { result } else { f64::NAN }
}

fn apply_binary(left: f64, right: f64, operation: fn(f64, f64) -> f64) -> f64 {
    if !left.is_finite() || !right.is_finite() {
        return f64::NAN;
    }
    let result = operation(left, right);
    if result.is_finite() { result } else { f64::NAN }
}

macro_rules! checked_unary {
    ($name:ident, $operation:expr) => {
        fn $name(value: f64) -> f64 {
            apply_unary(value, $operation)
        }
    };
}

macro_rules! checked_binary {
    ($name:ident, $operation:expr) => {
        fn $name(left: f64, right: f64) -> f64 {
            apply_binary(left, right, $operation)
        }
    };
}

checked_binary!(add, |left, right| left + right);
checked_binary!(subtract, |left, right| left - right);
checked_binary!(multiply, |left, right| left * right);
checked_binary!(power, |left, right| left.powf(right));
checked_binary!(atan2, |left, right| left.atan2(right));
checked_binary!(minimum, |left, right| left.min(right));
checked_binary!(maximum, |left, right| left.max(right));

fn divide(left: f64, right: f64) -> f64 {
    if right == 0.0 {
        return f64::NAN;
    }
    apply_binary(left, right, |left, right| left / right)
}

checked_unary!(positive, |value| value);
checked_unary!(negative, |value| -value);
checked_unary!(absolute, |value| value.abs());
checked_unary!(signum, |value| value.signum());
checked_unary!(sine, |value| value.sin());
checked_unary!(cosine, |value| value.cos());
checked_unary!(tangent, |value| value.tan());
checked_unary!(arcsine, |value| value.asin());
checked_unary!(arccosine, |value| value.acos());
checked_unary!(arctangent, |value| value.atan());
checked_unary!(sinh, |value| value.sinh());
checked_unary!(cosh, |value| value.cosh());
checked_unary!(tanh, |value| value.tanh());
checked_unary!(asinh, |value| value.asinh());
checked_unary!(acosh, |value| value.acosh());
checked_unary!(atanh, |value| value.atanh());
checked_unary!(floor, |value| value.floor());
checked_unary!(round, |value| value.round());
checked_unary!(ceil, |value| value.ceil());
checked_unary!(trunc, |value| value.trunc());
checked_unary!(fract, |value| value.fract());
checked_unary!(exponential, |value| value.exp());
checked_unary!(square_root, |value| value.sqrt());
checked_unary!(cube_root, |value| value.cbrt());
checked_unary!(natural_log, |value| value.ln());
checked_unary!(log2, |value| value.log2());
checked_unary!(log10, |value| value.log10());

impl MakeOperators<f64> for CheckedOperators {
    fn make<'a>() -> Vec<Operator<'a, f64>> {
        vec![
            Operator::make_bin(
                "^",
                BinOp {
                    apply: power,
                    prio: 4,
                    is_commutative: false,
                },
            ),
            Operator::make_bin(
                "*",
                BinOp {
                    apply: multiply,
                    prio: 2,
                    is_commutative: true,
                },
            ),
            Operator::make_bin(
                "/",
                BinOp {
                    apply: divide,
                    prio: 3,
                    is_commutative: false,
                },
            ),
            Operator::make_bin_unary(
                "+",
                BinOp {
                    apply: add,
                    prio: 0,
                    is_commutative: true,
                },
                positive,
            ),
            Operator::make_bin_unary(
                "-",
                BinOp {
                    apply: subtract,
                    prio: 1,
                    is_commutative: false,
                },
                negative,
            ),
            Operator::make_bin(
                "atan2",
                BinOp {
                    apply: atan2,
                    prio: 0,
                    is_commutative: false,
                },
            ),
            Operator::make_bin(
                "min",
                BinOp {
                    apply: minimum,
                    prio: 0,
                    is_commutative: false,
                },
            ),
            Operator::make_bin(
                "max",
                BinOp {
                    apply: maximum,
                    prio: 0,
                    is_commutative: false,
                },
            ),
            Operator::make_unary("abs", absolute),
            Operator::make_unary("signum", signum),
            Operator::make_unary("sin", sine),
            Operator::make_unary("cos", cosine),
            Operator::make_unary("tan", tangent),
            Operator::make_unary("asin", arcsine),
            Operator::make_unary("acos", arccosine),
            Operator::make_unary("atan", arctangent),
            Operator::make_unary("sinh", sinh),
            Operator::make_unary("cosh", cosh),
            Operator::make_unary("tanh", tanh),
            Operator::make_unary("asinh", asinh),
            Operator::make_unary("acosh", acosh),
            Operator::make_unary("atanh", atanh),
            Operator::make_unary("floor", floor),
            Operator::make_unary("round", round),
            Operator::make_unary("ceil", ceil),
            Operator::make_unary("trunc", trunc),
            Operator::make_unary("fract", fract),
            Operator::make_unary("exp", exponential),
            Operator::make_unary("sqrt", square_root),
            Operator::make_unary("cbrt", cube_root),
            Operator::make_unary("ln", natural_log),
            Operator::make_unary("log2", log2),
            Operator::make_unary("log10", log10),
            Operator::make_unary("log", natural_log),
            Operator::make_constant("PI", std::f64::consts::PI),
            Operator::make_constant("π", std::f64::consts::PI),
            Operator::make_constant("E", std::f64::consts::E),
            Operator::make_constant("e", std::f64::consts::E),
            Operator::make_constant("TAU", std::f64::consts::TAU),
            Operator::make_constant("τ", std::f64::consts::TAU),
        ]
    }
}
