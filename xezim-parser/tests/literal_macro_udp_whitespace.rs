//! White space between tokens (§5.3) in three places the lexer, the
//! preprocessor and the parser did not allow it (aionhw/xezim#578-#580):
//! inside a based literal (§5.7.1), between a macro's name and its actual
//! arguments (§22.5.1), and between a UDP's name, strength, delay and an
//! unnamed instance (§29.3).

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
