use super::*;

// test expr_literals
// fn foo() {
//     let _ = true;
//     let _ = false;
//     let _ = 1;
//     let _ = 2.0;
//     let _ = b'a';
//     let _ = 'b';
//     let _ = "c";
//     let _ = r"d";
//     let _ = b"e";
//     let _ = br"f";
// }
const LITERAL_FIRST: TokenSet =
    token_set![TRUE_KW, FALSE_KW, INT_NUMBER, FLOAT_NUMBER, BYTE, CHAR,
               STRING, RAW_STRING, BYTE_STRING, RAW_BYTE_STRING];

pub(super) fn literal(p: &mut Parser) -> Option<CompletedMarker> {
    if !LITERAL_FIRST.contains(p.current()) {
        return None;
    }
    let m = p.start();
    p.bump();
    Some(m.complete(p, LITERAL))
}

const EXPR_FIRST: TokenSet = PREFIX_EXPR_FIRST;

pub(super) fn expr(p: &mut Parser) {
    expr_bp(p, 1)
}

fn bp_of(op: SyntaxKind) -> u8 {
    match op {
        EQEQ | NEQ => 1,
        MINUS | PLUS => 2,
        STAR | SLASH => 3,
        _ => 0
    }
}


// test expr_binding_power
// fn foo() {
//     1 + 2 * 3 == 1 * 2 + 3
// }

// Parses expression with binding power of at least bp.
fn expr_bp(p: &mut Parser, bp: u8) {
    let mut lhs = match unary_expr(p) {
        Some(lhs) => lhs,
        None => return,
    };

    loop {
        let op_bp = bp_of(p.current());
        if op_bp < bp {
            break;
        }
        lhs = bin_expr(p, lhs, op_bp);
    }
}

fn unary_expr(p: &mut Parser) -> Option<CompletedMarker> {
    let done = match p.current() {
        AMPERSAND => ref_expr(p),
        STAR => deref_expr(p),
        EXCL => not_expr(p),
        _ => {
            let lhs = atom_expr(p)?;
            postfix_expr(p, lhs)
        }
    };
    Some(done)
}

fn postfix_expr(p: &mut Parser, mut lhs: CompletedMarker) -> CompletedMarker {
    loop {
        lhs = match p.current() {
            L_PAREN => call_expr(p, lhs),
            DOT if p.nth(1) == IDENT => if p.nth(2) == L_PAREN {
                method_call_expr(p, lhs)
            } else {
                field_expr(p, lhs)
            },
            DOT if p.nth(1) == INT_NUMBER => field_expr(p, lhs),
            QUESTION => try_expr(p, lhs),
            _ => break,
        }
    }
    lhs
}

// test block
// fn a() {}
// fn b() { let _ = 1; }
// fn c() { 1; 2; }
// fn d() { 1; 2 }
pub(super) fn block(p: &mut Parser) {
    if !p.at(L_CURLY) {
        p.error("expected block");
        return;
    }
    block_expr(p);
}

// test let_stmt;
// fn foo() {
//     let a;
//     let b: i32;
//     let c = 92;
//     let d: i32 = 92;
// }
fn let_stmt(p: &mut Parser) {
    assert!(p.at(LET_KW));
    let m = p.start();
    p.bump();
    patterns::pattern(p);
    if p.at(COLON) {
        types::ascription(p);
    }
    if p.eat(EQ) {
        expressions::expr(p);
    }
    p.expect(SEMI);
    m.complete(p, LET_STMT);
}

const PREFIX_EXPR_FIRST: TokenSet =
    token_set_union![
        token_set![AMPERSAND, STAR, EXCL],
        ATOM_EXPR_FIRST,
    ];

// test ref_expr
// fn foo() {
//     let _ = &1;
//     let _ = &mut &f();
// }
fn ref_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(AMPERSAND));
    let m = p.start();
    p.bump();
    p.eat(MUT_KW);
    expr(p);
    m.complete(p, REF_EXPR)
}

// test deref_expr
// fn foo() {
//     **&1;
// }
fn deref_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(STAR));
    let m = p.start();
    p.bump();
    expr(p);
    m.complete(p, DEREF_EXPR)
}

// test not_expr
// fn foo() {
//     !!true;
// }
fn not_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(EXCL));
    let m = p.start();
    p.bump();
    expr(p);
    m.complete(p, NOT_EXPR)
}

const ATOM_EXPR_FIRST: TokenSet =
    token_set_union![
        LITERAL_FIRST,
        token_set![L_PAREN, PIPE, MOVE_KW, IF_KW, UNSAFE_KW, L_CURLY, RETURN_KW],
    ];

fn atom_expr(p: &mut Parser) -> Option<CompletedMarker> {
    match literal(p) {
        Some(m) => return Some(m),
        None => (),
    }
    if paths::is_path_start(p) {
        return Some(path_expr(p));
    }
    let la = p.nth(1);
    let done = match p.current() {
        L_PAREN => tuple_expr(p),
        PIPE => lambda_expr(p),
        MOVE_KW if la == PIPE => lambda_expr(p),
        IF_KW => if_expr(p),
        MATCH_KW => match_expr(p),
        UNSAFE_KW if la == L_CURLY => block_expr(p),
        L_CURLY => block_expr(p),
        RETURN_KW => return_expr(p),
        _ => {
            p.err_and_bump("expected expression");
            return None;
        }
    };
    Some(done)
}

fn tuple_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(L_PAREN));
    let m = p.start();
    p.expect(L_PAREN);
    p.expect(R_PAREN);
    m.complete(p, TUPLE_EXPR)
}

// test lambda_expr
// fn foo() {
//     || ();
//     || -> i32 { 92 };
//     |x| x;
//     move |x: i32,| x;
// }
fn lambda_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(PIPE) || (p.at(MOVE_KW) && p.nth(1) == PIPE));
    let m = p.start();
    p.eat(MOVE_KW);
    params::param_list_opt_types(p);
    if fn_ret_type(p) {
        block(p);
    } else {
        expr(p)
    }
    m.complete(p, LAMBDA_EXPR)
}

// test if_expr
// fn foo() {
//     if true {};
//     if true {} else {};
//     if true {} else if false {} else {}
// }
fn if_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(IF_KW));
    let m = p.start();
    if_head(p);
    block(p);
    if p.at(ELSE_KW) {
        p.bump();
        if p.at(IF_KW) {
            if_expr(p);
        } else {
            block(p);
        }
    }
    m.complete(p, IF_EXPR)
}

fn if_head(p: &mut Parser) {
    assert!(p.at(IF_KW));
    p.bump();
    expr(p);
}

// test match_expr
// fn foo() {
//     match () { };
// }
fn match_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(MATCH_KW));
    let m = p.start();
    p.bump();
    expr(p);
    p.eat(L_CURLY);
    while !p.at(EOF) && !p.at(R_CURLY) {
        match_arm(p);
        if !p.at(R_CURLY) {
            p.expect(COMMA);
        }
    }
    p.expect(R_CURLY);
    m.complete(p, MATCH_EXPR)
}

// test match_arm
// fn foo() {
//     match () {
//         _ => (),
//         X | Y if Z => (),
//     };
// }
fn match_arm(p: &mut Parser) {
    let m = p.start();
    loop {
        patterns::pattern(p);
        if !p.eat(PIPE) {
            break;
        }
    }
    if p.at(IF_KW) {
        if_head(p)
    }
    p.expect(FAT_ARROW);
    expr(p);
    m.complete(p, MATCH_ARM);
}

// test block_expr
// fn foo() {
//     {};
//     unsafe {};
// }
fn block_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(L_CURLY) || p.at(UNSAFE_KW) && p.nth(1) == L_CURLY);
    let m = p.start();
    p.eat(UNSAFE_KW);
    p.bump();
    while !p.at(EOF) && !p.at(R_CURLY) {
        match p.current() {
            LET_KW => let_stmt(p),
            _ => {
                // test block_items
                // fn a() { fn b() {} }
                let m = p.start();
                match items::maybe_item(p) {
                    items::MaybeItem::Item(kind) => {
                        m.complete(p, kind);
                    }
                    items::MaybeItem::Modifiers => {
                        m.abandon(p);
                        p.error("expected an item");
                    }
                    // test pub_expr
                    // fn foo() { pub 92; } //FIXME
                    items::MaybeItem::None => {
                        expressions::expr(p);
                        if p.eat(SEMI) {
                            m.complete(p, EXPR_STMT);
                        } else {
                            m.abandon(p);
                        }
                    }
                }
            }
        }
    }
    p.expect(R_CURLY);
    m.complete(p, BLOCK_EXPR)
}

// test return_expr
// fn foo() {
//     return;
//     return 92;
// }
fn return_expr(p: &mut Parser) -> CompletedMarker {
    assert!(p.at(RETURN_KW));
    let m = p.start();
    p.bump();
    if EXPR_FIRST.contains(p.current()) {
        expr(p);
    }
    m.complete(p, RETURN_EXPR)
}

// test call_expr
// fn foo() {
//     let _ = f();
//     let _ = f()(1)(1, 2,);
// }
fn call_expr(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    assert!(p.at(L_PAREN));
    let m = lhs.precede(p);
    arg_list(p);
    m.complete(p, CALL_EXPR)
}

// test method_call_expr
// fn foo() {
//     x.foo();
//     y.bar(1, 2,);
// }
fn method_call_expr(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    assert!(p.at(DOT) && p.nth(1) == IDENT && p.nth(2) == L_PAREN);
    let m = lhs.precede(p);
    p.bump();
    name_ref(p);
    arg_list(p);
    m.complete(p, METHOD_CALL_EXPR)
}

// test field_expr
// fn foo() {
//     x.foo;
//     x.0.bar;
// }
fn field_expr(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    assert!(p.at(DOT) && (p.nth(1) == IDENT || p.nth(1) == INT_NUMBER));
    let m = lhs.precede(p);
    p.bump();
    if p.at(IDENT) {
        name_ref(p)
    } else {
        p.bump()
    }
    m.complete(p, FIELD_EXPR)
}

// test try_expr
// fn foo() {
//     x?;
// }
fn try_expr(p: &mut Parser, lhs: CompletedMarker) -> CompletedMarker {
    assert!(p.at(QUESTION));
    let m = lhs.precede(p);
    p.bump();
    m.complete(p, TRY_EXPR)
}

fn arg_list(p: &mut Parser) {
    assert!(p.at(L_PAREN));
    let m = p.start();
    p.bump();
    while !p.at(R_PAREN) && !p.at(EOF) {
        expr(p);
        if !p.at(R_PAREN) && !p.expect(COMMA) {
            break;
        }
    }
    p.eat(R_PAREN);
    m.complete(p, ARG_LIST);
}

// test path_expr
// fn foo() {
//     let _ = a;
//     let _ = a::b;
//     let _ = ::a::<b>;
// }
fn path_expr(p: &mut Parser) -> CompletedMarker {
    assert!(paths::is_path_start(p));
    let m = p.start();
    paths::expr_path(p);
    if p.at(L_CURLY) {
        struct_lit(p);
        m.complete(p, STRUCT_LIT)
    } else {
        m.complete(p, PATH_EXPR)
    }
}

// test struct_lit
// fn foo() {
//     S {};
//     S { x, y: 32, };
//     S { x, y: 32, ..Default::default() };
// }
fn struct_lit(p: &mut Parser) {
    assert!(p.at(L_CURLY));
    p.bump();
    while !p.at(EOF) && !p.at(R_CURLY) {
        match p.current() {
            IDENT => {
                let m = p.start();
                name_ref(p);
                if p.eat(COLON) {
                    expr(p);
                }
                m.complete(p, STRUCT_LIT_FIELD);
            }
            DOTDOT => {
                p.bump();
                expr(p);
            }
            _ => p.err_and_bump("expected identifier"),
        }
        if !p.at(R_CURLY) {
            p.expect(COMMA);
        }
    }
    p.expect(R_CURLY);
}

fn bin_expr(p: &mut Parser, lhs: CompletedMarker, bp: u8) -> CompletedMarker {
    assert!(match p.current() {
        MINUS | PLUS | STAR | SLASH | EQEQ | NEQ => true,
        _ => false,
    });
    let m = lhs.precede(p);
    p.bump();
    expr_bp(p, bp);
    m.complete(p, BIN_EXPR)
}