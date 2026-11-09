//! Isolated unit tests for modularized strata_js submodules: ast, parser, scope, dom, and eval.

use strata_js::engine::ast::*;
use strata_js::engine::dom::*;
use strata_js::engine::parser::JsParser;
use strata_js::engine::scope::*;
use strata_js::engine::JsEngine;
use strata_js::JsValue;

#[test]
fn test_isolated_ast_construction_and_invariants() {
    let expr = Expr::Binary {
        op: BinaryOp::Add,
        left: Box::new(Expr::Literal(JsValue::Number(40.0))),
        right: Box::new(Expr::Literal(JsValue::Number(2.0))),
    };

    let stmt = Stmt::VarDecl {
        name: "result".to_string(),
        init: Some(expr),
    };

    if let Stmt::VarDecl { name, init: Some(Expr::Binary { op, .. }) } = &stmt {
        assert_eq!(name, "result");
        assert_eq!(*op, BinaryOp::Add);
    } else {
        panic!("AST structure mismatch: {:?}", stmt);
    }

    // Verify DestructurePattern and UpdateOp
    let update = Expr::Update {
        op: UpdateOp::Increment,
        target: Box::new(Expr::Identifier("counter".to_string())),
        prefix: false,
    };
    assert_eq!(
        update,
        Expr::Update {
            op: UpdateOp::Increment,
            target: Box::new(Expr::Identifier("counter".to_string())),
            prefix: false,
        }
    );
}

#[test]
fn test_isolated_parser_statements_and_expressions() {
    let script = "const answer = 42; if (answer > 40) { return answer + 1; }";
    let mut parser = JsParser::new(script);
    let stmts = parser.parse_program().expect("Parse should succeed");

    assert_eq!(stmts.len(), 2);
    match &stmts[0] {
        Stmt::VarDecl { name, init } => {
            assert_eq!(name, "answer");
            assert_eq!(*init, Some(Expr::Literal(JsValue::Number(42.0))));
        }
        other => panic!("Expected VarDecl, got: {:?}", other),
    }

    match &stmts[1] {
        Stmt::If { cond, then_branch, else_branch } => {
            assert!(matches!(cond, Expr::Binary { op: BinaryOp::Gt, .. }));
            assert!(matches!(**then_branch, Stmt::Block(_)));
            assert!(else_branch.is_none());
        }
        other => panic!("Expected If, got: {:?}", other),
    }
}

#[test]
fn test_isolated_scope_chain_lexical_resolution() {
    let root = Scope::new_root();
    root.borrow_mut().declare("global_val", JsValue::Number(100.0));
    root.borrow_mut().declare("shadowed", JsValue::String("outer".to_string()));

    let child = Scope::new_child(root.clone());
    child.borrow_mut().declare("child_val", JsValue::Number(200.0));
    child.borrow_mut().declare("shadowed", JsValue::String("inner".to_string()));

    // Child resolves child_val and shadowed as inner
    assert_eq!(child.borrow().get("child_val"), Some(JsValue::Number(200.0)));
    assert_eq!(
        child.borrow().get("shadowed"),
        Some(JsValue::String("inner".to_string()))
    );
    // Child resolves root's global_val through parent pointer
    assert_eq!(child.borrow().get("global_val"), Some(JsValue::Number(100.0)));

    // Root retains outer shadowed value
    assert_eq!(
        root.borrow().get("shadowed"),
        Some(JsValue::String("outer".to_string()))
    );
    assert_eq!(root.borrow().get("child_val"), None);

    // Test parameter binding with rest parameter
    let fn_scope = Scope::new_child(root.clone());
    let params = vec!["first".to_string(), "...rest".to_string()];
    let args = vec![
        JsValue::Number(1.0),
        JsValue::Number(2.0),
        JsValue::Number(3.0),
    ];
    bind_parameters(&params, args, &fn_scope);

    assert_eq!(fn_scope.borrow().get("first"), Some(JsValue::Number(1.0)));
    assert_eq!(
        fn_scope.borrow().get("rest"),
        Some(JsValue::new_array(vec![
            JsValue::Number(2.0),
            JsValue::Number(3.0)
        ]))
    );
}

#[test]
fn test_isolated_dom_tree_and_mutation_sink() {
    let doc = Document::new();
    let container = DomNode::element("div", [("id".to_string(), "app".to_string())].into_iter().collect());
    let paragraph = DomNode::element("p", [("class".to_string(), "lead".to_string())].into_iter().collect());
    paragraph.set_text_content("Hello Modular Strata!");

    container.append_child(paragraph.clone());
    doc.body.append_child(container.clone());

    assert_eq!(paragraph.text_content(), "Hello Modular Strata!");
    assert_eq!(paragraph.tag_name(), Some("P".to_string()));
    assert_eq!(container.id(), Some("app".to_string()));

    let found = doc.get_element_by_id("app");
    assert!(found.is_some());
    assert_eq!(found.unwrap().uid(), container.uid());

    // Verify HTML fragment parser
    let fragment = parse_html_fragment("<span class=\"badge\">Active</span><br/>");
    assert_eq!(fragment.len(), 2);
    assert_eq!(fragment[0].tag_name(), Some("SPAN".to_string()));
    assert_eq!(fragment[0].text_content(), "Active");
    assert_eq!(fragment[1].tag_name(), Some("BR".to_string()));
}

#[test]
fn test_modular_engine_integration_eval() {
    let mut engine = JsEngine::new();
    let res = engine
        .eval("const multiply = (a, b) => a * b; multiply(6, 7);")
        .expect("Evaluation should succeed");
    assert_eq!(res, JsValue::Number(42.0));

    // Test prototype method dispatch
    let arr_res = engine
        .eval("const list = [1, 2, 3]; list.push(4); list.length;")
        .expect("Array evaluation should succeed");
    assert_eq!(arr_res, JsValue::Number(4.0));
}
