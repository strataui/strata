//! Comprehensive Integration Tests for Tier 3: Core ECMAScript Runtime (Mica / strata_js) Integrity.
//! Validates:
//! - Shared object reference semantics (mutations across aliases and function boundaries)
//! - Strict mode enforcement ("use strict" preventing undeclared global assignments)
//! - Proxy handler get and set traps
//! - Reflect API built-ins (get, set, has, deleteProperty, ownKeys)
//! - WeakMap, WeakSet, and WeakRef identity semantics
//! - BigInt literal parsing, arithmetic, exponentiation, bitwise operations, comparisons, and type guards
//! - Generator functions and generator object iteration protocols (yield, next, done)
//! - Date built-in methods (getTime, getFullYear, toISOString)
//! - Bitwise operations and exponentiation (** operator)

use strata_js::prelude::*;

#[test]
fn test_shared_object_reference_semantics() {
    let mut engine = JsEngine::new();
    let script = r#"
        var origin = { count: 0, items: [1, 2] };
        var alias = origin;

        alias.count = 42;
        alias.items.push(3);

        function mutateTarget(target) {
            target.name = "strata";
            target.items.push(4);
        }
        mutateTarget(origin);

        var isIdentical = (origin === alias);
    "#;
    engine.eval(script).expect("eval reference semantics");

    let origin = engine.globals.get("origin").expect("origin object");
    assert_eq!(origin.get_property("count"), Some(JsValue::Number(42.0)));
    assert_eq!(origin.get_property("name"), Some(JsValue::String("strata".to_string())));

    let items = origin.get_property("items").expect("items array");
    if let JsValue::Array(ref arr) = items {
        assert_eq!(
            *arr.borrow(),
            vec![
                JsValue::Number(1.0),
                JsValue::Number(2.0),
                JsValue::Number(3.0),
                JsValue::Number(4.0),
            ]
        );
    } else {
        panic!("items must be an array");
    }

    assert_eq!(engine.globals.get("isIdentical"), Some(&JsValue::Bool(true)));
}

#[test]
fn test_strict_mode_enforcement() {
    let mut engine = JsEngine::new();

    // In strict mode, assigning to an undeclared identifier must throw ReferenceError
    let strict_err_script = r#"
        "use strict";
        undeclaredVar = 100;
    "#;
    let res = engine.eval(strict_err_script);
    assert!(res.is_err(), "Strict mode should reject undeclared variable assignments");
    assert!(
        res.unwrap_err().contains("ReferenceError"),
        "Error message should mention ReferenceError"
    );

    // In non-strict mode, undeclared assignment defaults to creating a global variable
    let mut non_strict_engine = JsEngine::new();
    let non_strict_script = r#"
        declaredLater = 200;
    "#;
    let ok_res = non_strict_engine.eval(non_strict_script);
    assert!(ok_res.is_ok(), "Non-strict mode allows global property creation");
    assert_eq!(
        non_strict_engine.globals.get("declaredLater"),
        Some(&JsValue::Number(200.0))
    );
}

#[test]
fn test_proxy_traps_and_reflect_api() {
    let mut engine = JsEngine::new();
    let script = r#"
        var target = { x: 10, y: 20 };
        var proxy = new Proxy(target, {
            get: function(t, prop) {
                if (prop === "virtual") {
                    return 999;
                }
                return Reflect.get(t, prop);
            },
            set: function(t, prop, val) {
                if (prop === "blocked") {
                    return false;
                }
                return Reflect.set(t, prop, val);
            }
        });

        var readX = proxy.x;
        var readVirtual = proxy.virtual;
        proxy.z = 30;
        proxy.blocked = 500;

        var hasX = Reflect.has(target, "x");
        var hasMissing = Reflect.has(target, "missing");
        var keys = Reflect.ownKeys(target);

        Reflect.deleteProperty(target, "y");
        var hasYAfterDelete = Reflect.has(target, "y");
    "#;
    engine.eval(script).expect("eval proxy and reflect");

    assert_eq!(engine.globals.get("readX"), Some(&JsValue::Number(10.0)));
    assert_eq!(engine.globals.get("readVirtual"), Some(&JsValue::Number(999.0)));
    assert_eq!(engine.globals.get("hasX"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("hasMissing"), Some(&JsValue::Bool(false)));
    assert_eq!(engine.globals.get("hasYAfterDelete"), Some(&JsValue::Bool(false)));

    let target_val = engine.globals.get("target").expect("target");
    assert_eq!(target_val.get_property("z"), Some(JsValue::Number(30.0)));
    assert_eq!(target_val.get_property("blocked"), None);
}

#[test]
fn test_weak_collections_and_weak_ref() {
    let mut engine = JsEngine::new();
    let script = r#"
        var wm = new WeakMap();
        var keyObj1 = { id: 1 };
        var keyObj2 = { id: 2 };

        wm.set(keyObj1, "val1");
        wm.set(keyObj2, "val2");

        var getVal1 = wm.get(keyObj1);
        var hasKey1 = wm.has(keyObj1);
        wm.delete(keyObj1);
        var hasKey1AfterDel = wm.has(keyObj1);

        var ws = new WeakSet();
        ws.add(keyObj2);
        var wsHasKey2 = ws.has(keyObj2);
        ws.delete(keyObj2);
        var wsHasKey2AfterDel = ws.has(keyObj2);

        var target = { payload: "active" };
        var ref = new WeakRef(target);
        var derefVal = ref.deref().payload;
    "#;
    engine.eval(script).expect("eval weak collections");

    assert_eq!(engine.globals.get("getVal1"), Some(&JsValue::String("val1".to_string())));
    assert_eq!(engine.globals.get("hasKey1"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("hasKey1AfterDel"), Some(&JsValue::Bool(false)));

    assert_eq!(engine.globals.get("wsHasKey2"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("wsHasKey2AfterDel"), Some(&JsValue::Bool(false)));

    assert_eq!(engine.globals.get("derefVal"), Some(&JsValue::String("active".to_string())));
}

#[test]
fn test_bigint_arithmetic_bitwise_and_coercion() {
    let mut engine = JsEngine::new();
    let script = r#"
        var a = 100000000000000000000n;
        var b = 200000000000000000000n;

        var sum = a + b;
        var diff = b - a;
        var prod = 5n * 20n;
        var quot = 100n / 3n;
        var rem = 100n % 3n;
        var pow = 2n ** 10n;

        var bitAnd = 0b1100n & 0b1010n;
        var bitOr  = 0b1100n | 0b1010n;
        var bitXor = 0b1100n ^ 0b1010n;
        var bitNot = ~0n;
        var shl = 1n << 4n;
        var shr = 32n >> 2n;

        var isLess = a < b;
        var isEq = (a === 100000000000000000000n);
        var isNotStrictEqNumber = (100n === 100);
        var isLooseEqNumber = (100n == 100);

        var parsedBigInt = BigInt("9007199254740993");
    "#;
    engine.eval(script).expect("eval bigint suite");

    assert_eq!(
        engine.globals.get("sum"),
        Some(&JsValue::BigInt("300000000000000000000".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("diff"),
        Some(&JsValue::BigInt("100000000000000000000".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("prod"),
        Some(&JsValue::BigInt("100".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("quot"),
        Some(&JsValue::BigInt("33".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("rem"),
        Some(&JsValue::BigInt("1".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("pow"),
        Some(&JsValue::BigInt("1024".parse().unwrap()))
    );

    assert_eq!(
        engine.globals.get("bitAnd"),
        Some(&JsValue::BigInt("8".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("bitOr"),
        Some(&JsValue::BigInt("14".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("bitXor"),
        Some(&JsValue::BigInt("6".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("bitNot"),
        Some(&JsValue::BigInt("-1".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("shl"),
        Some(&JsValue::BigInt("16".parse().unwrap()))
    );
    assert_eq!(
        engine.globals.get("shr"),
        Some(&JsValue::BigInt("8".parse().unwrap()))
    );

    assert_eq!(engine.globals.get("isLess"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isEq"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isNotStrictEqNumber"), Some(&JsValue::Bool(false)));
    assert_eq!(engine.globals.get("isLooseEqNumber"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("parsedBigInt"),
        Some(&JsValue::BigInt("9007199254740993".parse().unwrap()))
    );

    // Mixed BigInt and Number in arithmetic must throw TypeError
    let type_err_script = "var invalid = 10n + 5;";
    let res = engine.eval(type_err_script);
    assert!(res.is_err(), "Mixed BigInt and Number arithmetic must fail");
    assert!(
        res.unwrap_err().contains("TypeError"),
        "Expected TypeError on mixed BigInt/Number"
    );
}

#[test]
fn test_generator_functions_and_yield() {
    let mut engine = JsEngine::new();
    let script = r#"
        function* countUp() {
            yield 10;
            yield 20;
            yield 30;
            return 40;
        }

        var gen = countUp();
        var step1 = gen.next();
        var step2 = gen.next();
        var step3 = gen.next();
        var step4 = gen.next();
        var step5 = gen.next();

        var v1 = step1.value;
        var d1 = step1.done;

        var v2 = step2.value;
        var d2 = step2.done;

        var v3 = step3.value;
        var d3 = step3.done;

        var v4 = step4.value;
        var d4 = step4.done;

        var v5 = step5.value;
        var d5 = step5.done;
    "#;
    engine.eval(script).expect("eval generator functions");

    assert_eq!(engine.globals.get("v1"), Some(&JsValue::Number(10.0)));
    assert_eq!(engine.globals.get("d1"), Some(&JsValue::Bool(false)));

    assert_eq!(engine.globals.get("v2"), Some(&JsValue::Number(20.0)));
    assert_eq!(engine.globals.get("d2"), Some(&JsValue::Bool(false)));

    assert_eq!(engine.globals.get("v3"), Some(&JsValue::Number(30.0)));
    assert_eq!(engine.globals.get("d3"), Some(&JsValue::Bool(false)));

    assert_eq!(engine.globals.get("v4"), Some(&JsValue::Number(40.0)));
    assert_eq!(engine.globals.get("d4"), Some(&JsValue::Bool(true)));

    assert_eq!(engine.globals.get("v5"), Some(&JsValue::Undefined));
    assert_eq!(engine.globals.get("d5"), Some(&JsValue::Bool(true)));
}

#[test]
fn test_date_api_methods() {
    let mut engine = JsEngine::new();
    let script = r#"
        var fixedDate = new Date(1700000000000);
        var timeVal = fixedDate.getTime();
        var yearVal = fixedDate.getFullYear();
        var isoStr = fixedDate.toISOString();

        var nowMs = Date.now();
    "#;
    engine.eval(script).expect("eval date methods");

    assert_eq!(engine.globals.get("timeVal"), Some(&JsValue::Number(1700000000000.0)));
    assert_eq!(engine.globals.get("yearVal"), Some(&JsValue::Number(2023.0)));

    if let Some(JsValue::String(ref iso)) = engine.globals.get("isoStr") {
        assert!(iso.starts_with("2023-11-14T"));
        assert!(iso.ends_with('Z'));
    } else {
        panic!("isoStr must be a string");
    }

    if let Some(JsValue::Number(ms)) = engine.globals.get("nowMs") {
        assert!(*ms > 0.0);
    } else {
        panic!("Date.now() must be a positive number");
    }
}

#[test]
fn test_bitwise_and_exponentiation_operators() {
    let mut engine = JsEngine::new();
    let script = r#"
        var bit_and = 12 & 10;
        var bit_or  = 12 | 10;
        var bit_xor = 12 ^ 10;
        var bit_shl = 1 << 3;
        var bit_shr = 16 >> 2;
        var exp1 = 2 ** 8;
        var exp2 = 3 ** 3;
    "#;
    engine.eval(script).expect("eval bitwise and exponentiation");

    assert_eq!(engine.globals.get("bit_and"), Some(&JsValue::Number(8.0)));
    assert_eq!(engine.globals.get("bit_or"), Some(&JsValue::Number(14.0)));
    assert_eq!(engine.globals.get("bit_xor"), Some(&JsValue::Number(6.0)));
    assert_eq!(engine.globals.get("bit_shl"), Some(&JsValue::Number(8.0)));
    assert_eq!(engine.globals.get("bit_shr"), Some(&JsValue::Number(4.0)));
    assert_eq!(engine.globals.get("exp1"), Some(&JsValue::Number(256.0)));
    assert_eq!(engine.globals.get("exp2"), Some(&JsValue::Number(27.0)));
}

#[test]
fn test_promise_first_class_reference_semantics() {
    let mut engine = JsEngine::new();
    let script = r#"
        var p1 = new Promise(function(resolve, reject) {
            resolve(100);
        });
        var typeOfP1 = typeof p1;
        var p1IsSelfIdentical = (p1 === p1);
        var p2 = Promise.resolve(200);
        var p2NotP1 = (p1 !== p2);
    "#;
    engine.eval(script).expect("eval promise reference semantics");

    let p1 = engine.globals.get("p1").expect("p1 promise");
    assert!(p1.is_promise(), "p1 must be a first-class JsValue::Promise");
    assert_eq!(engine.globals.get("typeOfP1"), Some(&JsValue::String("object".to_string())));
    assert_eq!(engine.globals.get("p1IsSelfIdentical"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("p2NotP1"), Some(&JsValue::Bool(true)));
    assert!(p1.is_truthy());
    assert_eq!(p1.to_json_string(), "{}");
}

#[test]
fn test_async_await_cooperative_settlement() {
    let mut engine = JsEngine::new();
    let script = r#"
        async function runCompute() {
            var a = await Promise.resolve(21);
            var b = await Promise.resolve(a * 2);
            return b;
        }

        var result = null;
        async function main() {
            result = await runCompute();
        }
        main();
    "#;
    engine.eval(script).expect("eval async await cooperative settlement");
    strata_js::promise::drain_microtasks();

    assert_eq!(engine.globals.get("result"), Some(&JsValue::Number(42.0)));
}
