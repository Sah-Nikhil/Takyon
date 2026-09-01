//! Tokenizer, parser and evaluator. The arithmetic, and nothing else.
//!
//! Hand-rolled rather than a crate — TBC-0011 carries the reasoning and what
//! would send us to one. Precedence climbing, so precedence is a table rather
//! than a shape of nested functions.
//!
//! It builds a tree instead of folding as it goes, for one reason: `%` means
//! "of the left operand" after `+` and `-`, so the operator has to be able to
//! *look at* its right operand rather than just take its value.
//!
//! **An unknown word is a hard error, never a variable.** That one rule is what
//! keeps `1password` out of the calculator: every Source sees every keystroke,
//! and a Calc Entry wins its tier outright.

/// Multiplication and division bind tighter than addition; `^` tighter still and
/// to the right, so `2^3^2` is 512. Unary minus sits between `*` and `^`, which
/// is what makes `-2^2` equal -4.
const BP_ADD: u8 = 1;
const BP_MUL: u8 = 2;
const BP_NEG: u8 = 3;
const BP_POW: u8 = 4;

/// Functions the calculator knows. **A `(` must follow**, so a bare `log` stays
/// an app search rather than becoming an error row.
const FUNCTIONS: &[&str] = &[
    "sqrt", "cbrt", "abs", "round", "floor", "ceil", "ln", "log", "log2", "exp", "sin", "cos",
    "tan", "min", "max",
];

/// The only bare word that is a value. `e` is deliberately absent: `2e5` is
/// scientific notation, and one spelling cannot mean both.
const PI: &str = "pi";

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Word(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Pct,
    LParen,
    RParen,
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f64),
    Neg(Box<Node>),
    /// A trailing `%`. Kept as a node rather than divided on the spot, because
    /// what it means depends on the operator to its left.
    Pct(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

/// What one input evaluated to, plus the one fact policy needs from the parse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eval {
    pub value: f64,
    /// The whole input was a single numeric literal — `2024`, not `2024+1`.
    ///
    /// Policy alone decides what to do about that (`mod.rs`); the parser only
    /// reports it, because it is the one shape fact the value cannot carry.
    pub literal: bool,
}

/// Evaluate a complete expression, or refuse.
///
/// `None` covers every failure alike — unknown word, trailing operator, a
/// division producing infinity. The Palette has no error row to put a reason in,
/// so there is nothing to distinguish them for.
pub fn eval(input: &str) -> Option<Eval> {
    let toks = lex(input)?;
    if toks.is_empty() {
        return None;
    }
    let literal = matches!(toks.as_slice(), [Tok::Num(_)]);

    let mut p = Parser { toks: &toks, at: 0 };
    let node = p.expr(0)?;
    // Trailing tokens mean the input was only *partly* an expression. `45 lb` is
    // not arithmetic, and answering 45 for it is how a launcher starts lying.
    if p.at != toks.len() {
        return None;
    }

    let value = run(&node)?;
    value.is_finite().then_some(Eval { value, literal })
}

/// Split input into tokens. A character outside the grammar fails the whole
/// input rather than being skipped.
fn lex(input: &str) -> Option<Vec<Tok>> {
    let chars: Vec<char> = input.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let (num, next) = lex_number(&chars, i)?;
            toks.push(Tok::Num(num));
            i = next;
            continue;
        }

        if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            toks.push(Tok::Word(word.to_lowercase()));
            continue;
        }

        // Typographic operators are accepted because they are what a paste from a
        // document or a phone keyboard actually contains.
        toks.push(match c {
            '+' => Tok::Plus,
            '-' | '\u{2212}' => Tok::Minus,
            '*' | '\u{00d7}' | '\u{22c5}' => Tok::Star,
            '/' | '\u{00f7}' => Tok::Slash,
            '^' => Tok::Caret,
            '%' => Tok::Pct,
            '(' | '[' => Tok::LParen,
            ')' | ']' => Tok::RParen,
            ',' => Tok::Comma,
            _ => return None,
        });
        i += 1;
    }
    Some(toks)
}

/// One numeric literal, including `1e6` notation.
///
/// The exponent is only consumed when digits actually follow, so `2exp` lexes as
/// `2` then a word — which then fails as an expression, which is right.
fn lex_number(chars: &[char], from: usize) -> Option<(f64, usize)> {
    let mut i = from;
    let mut seen_dot = false;
    while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && !seen_dot)) {
        seen_dot |= chars[i] == '.';
        i += 1;
    }

    if matches!(chars.get(i), Some('e' | 'E')) {
        let mut j = i + 1;
        if matches!(chars.get(j), Some('+' | '-')) {
            j += 1;
        }
        if chars.get(j).is_some_and(char::is_ascii_digit) {
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }

    let text: String = chars[from..i].iter().collect();
    Some((text.parse().ok()?, i))
}

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        let hit = self.peek() == Some(t);
        self.at += usize::from(hit);
        hit
    }

    /// Precedence climbing. `min_bp` is the weakest operator this call may absorb.
    fn expr(&mut self, min_bp: u8) -> Option<Node> {
        let mut lhs = self.prefix()?;

        loop {
            let (op, bp, right_assoc) = match self.peek() {
                Some(Tok::Plus) => (Op::Add, BP_ADD, false),
                Some(Tok::Minus) => (Op::Sub, BP_ADD, false),
                Some(Tok::Star) => (Op::Mul, BP_MUL, false),
                Some(Tok::Slash) => (Op::Div, BP_MUL, false),
                Some(Tok::Caret) => (Op::Pow, BP_POW, true),
                _ => break,
            };
            if bp < min_bp {
                break;
            }
            self.at += 1;

            let rhs = self.expr(if right_assoc { bp } else { bp + 1 })?;
            lhs = Node::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Some(lhs)
    }

    /// A value, with any unary signs, and a trailing `%` if one follows.
    fn prefix(&mut self) -> Option<Node> {
        let node = match self.peek()? {
            Tok::Minus => {
                self.at += 1;
                Node::Neg(Box::new(self.expr(BP_NEG)?))
            }
            Tok::Plus => {
                self.at += 1;
                self.expr(BP_NEG)?
            }
            _ => self.atom()?,
        };
        Some(if self.eat(&Tok::Pct) {
            Node::Pct(Box::new(node))
        } else {
            node
        })
    }

    fn atom(&mut self) -> Option<Node> {
        match self.peek()?.clone() {
            Tok::Num(n) => {
                self.at += 1;
                Some(Node::Num(n))
            }
            Tok::LParen => {
                self.at += 1;
                let inner = self.expr(0)?;
                self.eat(&Tok::RParen).then_some(inner)
            }
            Tok::Word(w) => {
                self.at += 1;
                if w == PI {
                    return Some(Node::Num(std::f64::consts::PI));
                }
                // A function name is only a function when a `(` follows it, so a
                // half-typed `log` is nothing rather than an error.
                if !FUNCTIONS.contains(&w.as_str()) || !self.eat(&Tok::LParen) {
                    return None;
                }
                let mut args = vec![self.expr(0)?];
                while self.eat(&Tok::Comma) {
                    args.push(self.expr(0)?);
                }
                if !self.eat(&Tok::RParen) {
                    return None;
                }
                Some(Node::Call(w, args))
            }
            _ => None,
        }
    }
}

/// Walk the tree.
///
/// The only interesting arm is a `%` on the right of an operator: `12+30%` is
/// 15.6, so the percentage is *of the left operand*. Every programming language
/// reads `%` as remainder and would answer 12.
fn run(node: &Node) -> Option<f64> {
    Some(match node {
        Node::Num(n) => *n,
        Node::Neg(inner) => -run(inner)?,
        Node::Pct(inner) => run(inner)? / 100.0,
        Node::Call(name, args) => {
            let args: Option<Vec<f64>> = args.iter().map(run).collect();
            apply(name, &args?)?
        }
        Node::Bin(op, l, r) => {
            let lhs = run(l)?;
            if let Node::Pct(pct) = r.as_ref() {
                let pct = run(pct)?;
                return Some(match op {
                    // "12 plus 30 percent" means 30% *of 12*.
                    Op::Add => lhs + lhs * pct / 100.0,
                    Op::Sub => lhs - lhs * pct / 100.0,
                    // "200 times 10 percent" is 20, and nobody reads it as 220 —
                    // so here the percentage is just the fraction.
                    Op::Mul => lhs * pct / 100.0,
                    Op::Div => lhs / (pct / 100.0),
                    Op::Pow => lhs.powf(pct / 100.0),
                });
            }
            let rhs = run(r)?;
            match op {
                Op::Add => lhs + rhs,
                Op::Sub => lhs - rhs,
                Op::Mul => lhs * rhs,
                Op::Div => lhs / rhs,
                Op::Pow => lhs.powf(rhs),
            }
        }
    })
}

/// Evaluate one known function. Wrong arity is a refusal, never a default.
fn apply(name: &str, args: &[f64]) -> Option<f64> {
    let one = |f: fn(f64) -> f64| (args.len() == 1).then(|| f(args[0]));
    match name {
        "sqrt" => one(f64::sqrt),
        "cbrt" => one(f64::cbrt),
        "abs" => one(f64::abs),
        "round" => one(f64::round),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "ln" => one(f64::ln),
        "log" => one(f64::log10),
        "log2" => one(f64::log2),
        "exp" => one(f64::exp),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "min" => (args.len() >= 2).then(|| args.iter().copied().fold(f64::INFINITY, f64::min)),
        "max" => (args.len() >= 2).then(|| args.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(input: &str) -> f64 {
        eval(input)
            .unwrap_or_else(|| panic!("{input} did not evaluate"))
            .value
    }

    /// The plan's worked example, and step 1 of the manual verification script.
    /// Floating point makes this 14.159999999999998; `fmt` is what turns it back
    /// into 14.16, so the value is compared with a tolerance here.
    #[test]
    fn v0_4_the_worked_example_from_the_plan() {
        assert!((v("12*1.18") - 14.16).abs() < 1e-9);
    }

    #[test]
    fn v0_4_precedence_and_parentheses() {
        assert_eq!(v("2+3*4"), 14.0);
        assert_eq!(v("(2+3)*4"), 20.0);
        assert_eq!(v("100/4/5"), 5.0);
        assert_eq!(v("10-3-2"), 5.0);
    }

    /// Right-associative, and binding tighter than unary minus. Both are the
    /// conventions every calculator uses, and neither falls out of a parser by
    /// accident.
    #[test]
    fn v0_4_exponent_is_right_associative_and_outranks_unary_minus() {
        assert_eq!(v("2^3^2"), 512.0);
        assert_eq!(v("-2^2"), -4.0);
        assert_eq!(v("(-2)^2"), 4.0);
        assert_eq!(v("2^-2"), 0.25);
    }

    /// The rule from the live Raycast screenshots: `10+30%` is 13 and `12+30%` is
    /// 15.6, so a percentage after `+` or `-` is a percentage **of the left
    /// operand**. Read as remainder, those would be 10 and 12.
    #[test]
    fn v0_4_a_percentage_after_plus_or_minus_is_relative_to_the_left_operand() {
        assert!((v("10+30%") - 13.0).abs() < 1e-9);
        assert!((v("12+30%") - 15.6).abs() < 1e-9);
        assert!((v("200-10%") - 180.0).abs() < 1e-9);
    }

    /// After `*` and `/` the same sign means the plain fraction, because "200
    /// times 10 percent" is 20 and nobody reads it as 220.
    #[test]
    fn v0_4_a_percentage_after_times_or_divide_is_the_plain_fraction() {
        assert!((v("200*10%") - 20.0).abs() < 1e-9);
        assert!((v("20/10%") - 200.0).abs() < 1e-9);
    }

    #[test]
    fn v0_4_a_standalone_percentage_is_a_fraction() {
        assert!((v("30%") - 0.3).abs() < 1e-9);
        assert!((v("(30%)*2") - 0.6).abs() < 1e-9);
    }

    /// Only the *right* operand of an operator gets the relative reading. A
    /// percentage on the left is already a number by the time the operator sees
    /// it, or `30%+10` would be circular.
    #[test]
    fn v0_4_a_percentage_on_the_left_is_an_ordinary_number() {
        assert!((v("30%+1") - 1.3).abs() < 1e-9);
        assert!((v("100-10%+5") - 95.0).abs() < 1e-9);
    }

    /// The trap the whole module is shaped around: every Source sees every
    /// keystroke, and a Calc Entry beats every app outright. An app name starting
    /// with a digit must not become arithmetic.
    #[test]
    fn v0_4_an_app_name_starting_with_a_digit_is_not_arithmetic() {
        for name in ["1password", "7zip", "3dsmax", "4k video downloader"] {
            assert!(eval(name).is_none(), "{name} evaluated");
        }
    }

    /// An unknown word is an error, never a variable defaulting to zero. This one
    /// rule makes the test above hold for names nobody thought to list.
    #[test]
    fn v0_4_an_unknown_word_is_refused_rather_than_treated_as_a_variable() {
        assert!(eval("x264").is_none());
        assert!(eval("foo+1").is_none());
        assert!(eval("2*bar").is_none());
    }

    /// Half-typed input is refused. Raycast answers `45+` with 45; we do not,
    /// because the Stability rule already stops the top row flickering, and
    /// answering a trailing operator commits to a number nobody finished writing.
    #[test]
    fn v0_4_an_incomplete_expression_is_refused() {
        for input in ["45+", "*3", "(1+2", "1++", "2^"] {
            assert!(eval(input).is_none(), "{input} evaluated");
        }
    }

    /// `45 lb` is a quantity, not a calculation. Answering 45 and dropping the
    /// unit is worse than not answering.
    #[test]
    fn v0_4_trailing_tokens_fail_the_whole_input() {
        assert!(eval("45 lb").is_none());
        assert!(eval("2 2").is_none());
    }

    /// Not an error row, and not `inf` either — a Calc Entry reading `inf` is
    /// noise sitting on top of the app the user was reaching for.
    #[test]
    fn v0_4_a_non_finite_result_is_no_result() {
        assert!(eval("1/0").is_none());
        assert!(eval("0/0").is_none());
    }

    /// Only policy can tell `2024` from `2024+1`, and only the parser knows which
    /// one it saw.
    #[test]
    fn v0_4_the_parser_reports_whether_the_input_was_a_bare_number() {
        assert!(eval("2024").unwrap().literal);
        assert!(!eval("2024+1").unwrap().literal);
        assert!(!eval("(2024)").unwrap().literal);
    }

    /// A function is only a function with a `(` after it. Without that, typing the
    /// first three letters of "Logitech Options" would be an unfinished function
    /// call rather than a search.
    #[test]
    fn v0_4_a_function_name_alone_is_not_a_function() {
        assert!(eval("log").is_none());
        assert!(eval("min").is_none());
        assert_eq!(v("log(1000)"), 3.0);
        assert_eq!(v("min(4,2,9)"), 2.0);
        assert_eq!(v("sqrt(16)"), 4.0);
    }

    /// Wrong arity refuses rather than filling in a default, which would answer
    /// confidently with a number nobody asked for.
    #[test]
    fn v0_4_a_function_called_with_the_wrong_arity_is_refused() {
        assert!(eval("sqrt(1,2)").is_none());
        assert!(eval("min(1)").is_none());
    }

    /// `2e5` is a number. It has to be, or scientific notation is unreachable —
    /// and that is why `e` is not a constant.
    #[test]
    fn v0_4_scientific_notation_lexes_as_one_number() {
        assert_eq!(v("2e5"), 200000.0);
        assert_eq!(v("1.5e-3"), 0.0015);
        assert!(eval("e").is_none());
        // The exponent is taken only when digits follow, so this stays a word.
        assert!(eval("2exp").is_none());
    }

    /// Pasted from a document or typed on a phone keyboard, these are the same
    /// expression. Refusing them looks like a bug in the calculator.
    #[test]
    fn v0_4_typographic_operators_are_accepted() {
        assert_eq!(v("6\u{00d7}7"), 42.0);
        assert_eq!(v("84\u{00f7}2"), 42.0);
        assert_eq!(v("50\u{2212}8"), 42.0);
    }
}
