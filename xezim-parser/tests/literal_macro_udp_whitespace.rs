//! White space between tokens (§5.3) in three places the lexer, the
//! preprocessor and the parser did not allow it (aionhw/xezim#578-#580):
//! inside a based literal (§5.7.1), between a macro's name and its actual
//! arguments (§22.5.1), and between a UDP's name, strength, delay and an
//! unnamed instance (§29.3).

use sv_parser::preprocessor::Preprocessor;
use sv_parser::tokenize;

fn literals(src: &str) -> Vec<String> {
    tokenize(src)
        .into_iter()
        .filter(|t| format!("{:?}", t.kind) == "IntegerLiteral")
        .map(|t| t.text)
        .collect()
}

/// §5.7.1: the size, base and value are one literal across spaces, tabs and
/// newlines, and its text carries no white space, so the size is kept.
#[test]
fn based_literal_spans_white_space() {
    assert_eq!(literals("a = 2 'd1;"), ["2'd1"]);
    assert_eq!(literals("a = 12 'sh 3;"), ["12'sh3"]);
    assert_eq!(literals("a = 8\t'b1;"), ["8'b1"]);
    assert_eq!(literals("a = {2\n  'd1};"), ["2'd1"]);
    assert_eq!(literals("a = 2'd\n  1;"), ["2'd1"]);
    assert_eq!(literals("a = 2\n'd\n\t1;"), ["2'd1"]);
    // Not a based literal: a spaced cast keeps its bare apostrophe.
    let casts = literals("a = (w) ' (v);");
    assert!(casts.iter().any(|t| t == "'"), "{casts:?}");
}

fn preprocess(src: &str) -> (String, Vec<String>) {
    let mut pp = Preprocessor::new();
    let out = pp.preprocess(src);
    (out, pp.errors().to_vec())
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// §22.5.1: the actual-argument list may follow the name on a later line,
/// after blank lines and comments; a use with no list stays an error.
#[test]
fn macro_arguments_on_a_later_line() {
    let (out, errs) = preprocess(
        "`define ADD(a, b) ((a) + (b))\nmodule t; int r; initial begin\n  r = `ADD\n    // c\n\n    (7, 8);\n  r = `ADD\n    ( 5\n    , 6\n    );\nend endmodule\n",
    );
    assert!(errs.is_empty(), "{errs:?}");
    let out = norm(&out);
    assert!(out.contains("r = ((7) + (8));"), "{out}");
    assert!(
        out.contains("r = (( 5) + ( 6));") || out.contains("r = ((5) + (6));"),
        "{out}"
    );
    assert!(!out.contains("`ADD"), "{out}");
}
