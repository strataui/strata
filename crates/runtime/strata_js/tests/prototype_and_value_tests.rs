use strata_js::engine::JsEngine;
use strata_js::JsValue;

#[test]
fn test_first_class_functions_and_symbols_typeof() {
    let mut engine = JsEngine::new();

    let script = r#"
        function regularFn(a, b) {
            return a + b;
        }
        var arrowFn = (x) => x * 2;
        var sym = Symbol("custom_id");
        var num = 42;
        var str = "strata";
        var obj = { a: 1 };
        var arr = [1, 2, 3];

        var t_reg = typeof regularFn;
        var t_arr = typeof arrowFn;
        var t_sym = typeof sym;
        var t_num = typeof num;
        var t_str = typeof str;
        var t_obj = typeof obj;
        var t_arr_obj = typeof arr;
        var t_und = typeof undeclaredIdentifier;
    "#;

    engine.eval(script).expect("Execution failed");

    assert_eq!(
        engine.globals.get("t_reg"),
        Some(&JsValue::String("function".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_arr"),
        Some(&JsValue::String("function".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_sym"),
        Some(&JsValue::String("symbol".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_num"),
        Some(&JsValue::String("number".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_str"),
        Some(&JsValue::String("string".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_obj"),
        Some(&JsValue::String("object".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_arr_obj"),
        Some(&JsValue::String("object".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_und"),
        Some(&JsValue::String("undefined".to_string()))
    );
}

#[test]
fn test_symbol_uniqueness_and_global_registry() {
    let mut engine = JsEngine::new();

    let script = r#"
        var s1 = Symbol("my_symbol");
        var s2 = Symbol("my_symbol");
        var are_unique = (s1 !== s2);
        var s1_desc = s1.description;

        var g1 = Symbol.for("app.state");
        var g2 = Symbol.for("app.state");
        var are_same_global = (g1 === g2);
        var g_key = Symbol.keyFor(g1);

        var well_known_iter = Symbol.iterator;
        var t_iter = typeof well_known_iter;

        var symbol_ctor_error = null;
        try {
            new Symbol("disallowed");
        } catch (e) {
            symbol_ctor_error = e.message;
        }
    "#;

    engine.eval(script).expect("Execution failed");

    assert_eq!(engine.globals.get("are_unique"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("s1_desc"),
        Some(&JsValue::String("my_symbol".to_string()))
    );
    assert_eq!(
        engine.globals.get("are_same_global"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(
        engine.globals.get("g_key"),
        Some(&JsValue::String("app.state".to_string()))
    );
    assert_eq!(
        engine.globals.get("t_iter"),
        Some(&JsValue::String("symbol".to_string()))
    );
    assert_eq!(
        engine.globals.get("symbol_ctor_error"),
        Some(&JsValue::String("Symbol is not a constructor".to_string()))
    );
}

#[test]
fn test_prototype_inheritance_chain_and_delegation() {
    let mut engine = JsEngine::new();

    let script = r#"
        var protoGrandparent = {
            realm: "strata",
            describe: function() { return "grandparent"; }
        };

        var protoParent = Object.create(protoGrandparent);
        protoParent.tier = "core";

        var child = Object.create(protoParent);
        child.name = "leaf";

        var child_name = child.name;
        var child_tier = child.tier;
        var child_realm = child.realm;

        var has_own_name = child.hasOwnProperty("name");
        var has_own_tier = child.hasOwnProperty("tier");
        var has_own_realm = child.hasOwnProperty("realm");

        var proto_of_child = Object.getPrototypeOf(child);
        var is_parent_proto = (proto_of_child === protoParent);
    "#;

    engine.eval(script).expect("Execution failed");

    assert_eq!(
        engine.globals.get("child_name"),
        Some(&JsValue::String("leaf".to_string()))
    );
    assert_eq!(
        engine.globals.get("child_tier"),
        Some(&JsValue::String("core".to_string()))
    );
    assert_eq!(
        engine.globals.get("child_realm"),
        Some(&JsValue::String("strata".to_string()))
    );

    assert_eq!(
        engine.globals.get("has_own_name"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(
        engine.globals.get("has_own_tier"),
        Some(&JsValue::Bool(false))
    );
    assert_eq!(
        engine.globals.get("has_own_realm"),
        Some(&JsValue::Bool(false))
    );
    assert_eq!(
        engine.globals.get("is_parent_proto"),
        Some(&JsValue::Bool(true))
    );
}

#[test]
fn test_builtin_prototype_extension_and_object_statics() {
    let mut engine = JsEngine::new().with_allow_prototype_mutation(true);

    let script = r#"
        Array.prototype.customSum = function() {
            var sum = 0;
            for (var i = 0; i < this.length; i = i + 1) {
                sum = sum + this[i];
            }
            return sum;
        };

        var list = [10, 20, 30];
        var total = list.customSum();

        var obj = { a: 1, b: 2, c: 3 };
        var keys = Object.keys(obj);
        var vals = Object.values(obj);

        var target = { x: 1 };
        var merged = Object.assign(target, { y: 2 }, { z: 3 });
        var m_x = merged.x;
        var m_y = merged.y;
        var m_z = merged.z;
    "#;

    engine.eval(script).expect("Execution failed");

    assert_eq!(engine.globals.get("total"), Some(&JsValue::Number(60.0)));
    assert_eq!(engine.globals.get("m_x"), Some(&JsValue::Number(1.0)));
    assert_eq!(engine.globals.get("m_y"), Some(&JsValue::Number(2.0)));
    assert_eq!(engine.globals.get("m_z"), Some(&JsValue::Number(3.0)));

    if let Some(JsValue::Array(keys_arr)) = engine.globals.get("keys") {
        assert_eq!(keys_arr.borrow().len(), 3);
    } else {
        panic!("Object.keys must return array");
    }
}

#[test]
fn test_standard_error_hierarchy_and_runtime_traps() {
    let mut engine = JsEngine::new();

    let script = r#"
        var customTypeErr = new TypeError("invalid widget reference");
        var is_type_err = customTypeErr instanceof TypeError;
        var is_generic_err = customTypeErr instanceof Error;
        var err_name = customTypeErr.name;
        var err_msg = customTypeErr.message;

        var nullTrapName = null;
        var nullTrapMsg = null;
        try {
            var x = null;
            var val = x.someField;
        } catch (e) {
            nullTrapName = e.name;
            nullTrapMsg = e.message;
        }

        var refTrapName = null;
        var refTrapMsg = null;
        try {
            var y = completelyUndefinedIdentifier;
        } catch (e) {
            refTrapName = e.name;
            refTrapMsg = e.message;
        }
    "#;

    engine.eval(script).expect("Execution failed");

    assert_eq!(
        engine.globals.get("is_type_err"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(
        engine.globals.get("is_generic_err"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(
        engine.globals.get("err_name"),
        Some(&JsValue::String("TypeError".to_string()))
    );
    assert_eq!(
        engine.globals.get("err_msg"),
        Some(&JsValue::String("invalid widget reference".to_string()))
    );

    assert_eq!(
        engine.globals.get("nullTrapName"),
        Some(&JsValue::String("TypeError".to_string()))
    );
    assert!(engine
        .globals
        .get("nullTrapMsg")
        .unwrap()
        .to_string()
        .contains("Cannot read properties"));

    assert_eq!(
        engine.globals.get("refTrapName"),
        Some(&JsValue::String("ReferenceError".to_string()))
    );
    assert!(engine
        .globals
        .get("refTrapMsg")
        .unwrap()
        .to_string()
        .contains("not defined"));
}

#[test]
fn test_sealed_prototype_pollution_rejection_all_builtins() {
    let mut engine = JsEngine::new();
    assert!(engine.eval("Object.prototype.polluted = 1;").is_err());
    assert!(engine.eval("Array.prototype.polluted = 1;").is_err());
    assert!(engine.eval("Function.prototype.polluted = 1;").is_err());
    assert!(engine.eval("String.prototype.polluted = 1;").is_err());
    assert!(engine.eval("Error.prototype.polluted = 1;").is_err());
}
