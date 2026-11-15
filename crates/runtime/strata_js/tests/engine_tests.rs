use strata_js::engine::{Document, JsEngine};
use strata_js::JsValue;

#[test]
fn test_js_engine_document_body_inner_html_mutation() {
    let mut engine = JsEngine::new();
    let script = r#"
        document.body.innerHTML = "<h1>Hello from JS</h1>";
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    let body_html = engine.document.body.inner_html();
    assert!(
        body_html.contains("<h1>Hello from JS</h1>"),
        "body.innerHTML must contain h1 tag: {body_html}"
    );

    let h1 = engine.document.query_selector("h1");
    assert!(h1.is_some(), "h1 element must exist in document");
    assert_eq!(h1.unwrap().text_content(), "Hello from JS");
}

#[test]
fn test_js_engine_dom_get_element_by_id_and_mutation() {
    let html = r#"
        <div id="container">
            <p id="counter">0</p>
            <button id="btn" class="primary-btn">Click me</button>
        </div>
    "#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc);

    let script = r#"
        var counter = document.getElementById("counter");
        counter.textContent = "42";
        counter.setAttribute("data-status", "active");

        var btn = document.getElementById("btn");
        btn.className = "updated-btn";
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    let counter = engine.document.get_element_by_id("counter").unwrap();
    assert_eq!(counter.text_content(), "42");
    assert_eq!(
        counter.get_attribute("data-status"),
        Some("active".to_string())
    );

    let btn = engine.document.get_element_by_id("btn").unwrap();
    assert_eq!(btn.class_name(), "updated-btn");
}

#[test]
fn test_js_engine_event_listener_and_dispatch() {
    let html = r#"<button id="my-btn">Click</button>"#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc);

    let script = r#"
        function handleClick() {
            document.title = "Button Clicked";
        }
        var btn = document.getElementById("my-btn");
        btn.addEventListener("click", handleClick);
    "#;

    engine.eval(script).expect("Script execution failed");

    let btn = engine.document.get_element_by_id("my-btn").unwrap();
    btn.dispatch_event("click", &mut engine);

    assert_eq!(*engine.document.title.borrow(), "Button Clicked");
}

#[test]
fn test_js_engine_arithmetic_control_flow_and_functions() {
    let mut engine = JsEngine::new();
    let script = r#"
        function fib(n) {
            if (n <= 1) {
                return n;
            }
            var a = 0;
            var b = 1;
            for (var i = 2; i <= n; i = i + 1) {
                var temp = a + b;
                a = b;
                b = temp;
            }
            return b;
        }

        var res = fib(10);
        console.log("fib(10) =", res);
    "#;

    let _val = engine.eval(script).expect("Script execution failed");
    assert_eq!(engine.globals.get("res"), Some(&JsValue::Number(55.0)));
    assert_eq!(engine.console_logs.len(), 1);
    assert!(engine.console_logs[0].contains("fib(10) = 55"));
}

#[test]
fn test_js_engine_html_fragment_parsing_and_serialization() {
    let html = r#"<div class="card"><span id="lbl">Title</span><input type="text" /></div>"#;
    let nodes = strata_js::engine::parse_html_fragment(html);
    assert_eq!(nodes.len(), 1);

    let div = &nodes[0];
    assert_eq!(div.tag_name(), Some("DIV".to_string()));
    assert_eq!(div.class_name(), "card");
    assert_eq!(div.children().len(), 2);

    let span = div.get_element_by_id("lbl").unwrap();
    assert_eq!(span.text_content(), "Title");
}

#[test]
fn test_js_engine_try_catch_finally_and_throw() {
    let mut engine = JsEngine::new();

    // 1. Basic try/catch handling thrown error
    let script1 = r#"
        var caught = "none";
        try {
            throw "critical failure";
        } catch (e) {
            caught = e;
        }
    "#;
    engine.eval(script1).expect("Script 1 failed");
    assert_eq!(
        engine.globals.get("caught"),
        Some(&JsValue::String("critical failure".to_string()))
    );

    // 2. Try-catch-finally ordering
    let script2 = r#"
        var order = [];
        try {
            order.push(1);
            throw 404;
        } catch (err) {
            order.push(err);
        } finally {
            order.push(3);
        }
    "#;
    engine.eval(script2).expect("Script 2 failed");
    assert_eq!(
        engine.globals.get("order"),
        Some(&JsValue::new_array(vec![
            JsValue::Number(1.0),
            JsValue::Number(404.0),
            JsValue::Number(3.0),
        ]))
    );

    // 3. Uncaught throw results in an engine Err
    let script3 = r#"
        throw "fatal runtime exception";
    "#;
    let res = engine.eval(script3);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("fatal runtime exception"));

    // 4. Nested try/catch with rethrow
    let script4 = r#"
        var outer_handled = null;
        try {
            try {
                throw "inner error";
            } catch (inner) {
                throw "re-thrown: " + inner;
            }
        } catch (outer) {
            outer_handled = outer;
        }
    "#;
    engine.eval(script4).expect("Script 4 failed");
    assert_eq!(
        engine.globals.get("outer_handled"),
        Some(&JsValue::String("re-thrown: inner error".to_string()))
    );
}

#[test]
fn test_js_engine_template_literals() {
    let mut engine = JsEngine::new();

    let script = r#"
        var user = "Alice";
        var count = 3;
        var greeting = `Hello, ${user}! You have ${count * 2} pending notifications.`;
        var simple = `Plain literal string without interpolation`;
    "#;
    engine.eval(script).expect("Template literal script failed");
    assert_eq!(
        engine.globals.get("greeting"),
        Some(&JsValue::String(
            "Hello, Alice! You have 6 pending notifications.".to_string()
        ))
    );
    assert_eq!(
        engine.globals.get("simple"),
        Some(&JsValue::String(
            "Plain literal string without interpolation".to_string()
        ))
    );
}

#[test]
fn test_js_engine_arrow_functions_and_array_methods() {
    let mut engine = JsEngine::new();

    let script = r#"
        var double = x => x * 2;
        var add = (a, b) => a + b;
        var complex = (x, y) => {
            var diff = x - y;
            return diff * diff;
        };

        var d_res = double(21);
        var a_res = add(15, 27);
        var c_res = complex(10, 4);

        var list = [1, 2, 3, 4, 5];
        var mapped = list.map(x => x * 10);
        var filtered = list.filter(x => x > 2);
        var found = list.find(x => x % 2 === 0);
        var foundIdx = list.findIndex(x => x === 4);
        var hasEven = list.some(x => x % 2 === 0);
        var allPositive = list.every(x => x > 0);
        var sum = list.reduce((acc, x) => acc + x, 0);
        var sliced = list.slice(1, 4);
        var joined = list.join("-");
        var hasThree = list.includes(3);
        var idxThree = list.indexOf(3);
    "#;

    engine
        .eval(script)
        .expect("Arrow & array methods script failed");

    assert_eq!(engine.globals.get("d_res"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("a_res"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("c_res"), Some(&JsValue::Number(36.0)));

    assert_eq!(
        engine.globals.get("mapped"),
        Some(&JsValue::new_array(vec![
            JsValue::Number(10.0),
            JsValue::Number(20.0),
            JsValue::Number(30.0),
            JsValue::Number(40.0),
            JsValue::Number(50.0),
        ]))
    );
    assert_eq!(
        engine.globals.get("filtered"),
        Some(&JsValue::new_array(vec![
            JsValue::Number(3.0),
            JsValue::Number(4.0),
            JsValue::Number(5.0),
        ]))
    );
    assert_eq!(engine.globals.get("found"), Some(&JsValue::Number(2.0)));
    assert_eq!(engine.globals.get("foundIdx"), Some(&JsValue::Number(3.0)));
    assert_eq!(engine.globals.get("hasEven"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("allPositive"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(engine.globals.get("sum"), Some(&JsValue::Number(15.0)));
    assert_eq!(
        engine.globals.get("sliced"),
        Some(&JsValue::new_array(vec![
            JsValue::Number(2.0),
            JsValue::Number(3.0),
            JsValue::Number(4.0),
        ]))
    );
    assert_eq!(
        engine.globals.get("joined"),
        Some(&JsValue::String("1-2-3-4-5".to_string()))
    );
    assert_eq!(engine.globals.get("hasThree"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("idxThree"), Some(&JsValue::Number(2.0)));
}

#[test]
fn test_w3c_dom_hierarchy_and_mutation_apis() {
    let html = r#"
        <div id="container">
            <span id="item1">First</span>
            <span id="item2">Second</span>
            <span id="item3">Third</span>
        </div>
    "#;
    let doc = Document::from_html(html);
    let mut engine = JsEngine::with_document(doc);

    // 1. Rust API hierarchy checks
    let container = engine.document.get_element_by_id("container").unwrap();
    let item1 = engine.document.get_element_by_id("item1").unwrap();
    let item2 = engine.document.get_element_by_id("item2").unwrap();
    let item3 = engine.document.get_element_by_id("item3").unwrap();

    assert_eq!(
        item2.parent_element().unwrap().id(),
        Some("container".to_string())
    );
    assert!(container.contains(&item1));
    assert!(container.contains(&item2));
    assert!(container.contains(&item3));
    assert_eq!(
        item2.previous_element_sibling().unwrap().id(),
        Some("item1".to_string())
    );
    assert_eq!(
        item2.next_element_sibling().unwrap().id(),
        Some("item3".to_string())
    );

    // 2. JS eval DOM traversal and mutation
    let script = r#"
        var container = document.getElementById("container");
        var item2 = document.getElementById("item2");

        // Verify parent and siblings in JS
        var pId = item2.parentElement.id;
        var hasItem = container.contains(item2);

        // Create new element and insert before item2
        var newItem = document.createElement("p");
        newItem.id = "inserted-item";
        newItem.textContent = "Inserted Paragraph";
        container.insertBefore(newItem, item2);

        // Replace item3 with new span
        var item3 = document.getElementById("item3");
        var replacement = document.createElement("b");
        replacement.id = "replacement-item";
        replacement.textContent = "Replaced Bold";
        container.replaceChild(replacement, item3);

        // Remove item1
        var item1 = document.getElementById("item1");
        item1.remove();

        var elemCount = container.childElementCount;
    "#;

    engine
        .eval(script)
        .expect("DOM hierarchy JS evaluation failed");

    assert_eq!(
        engine.globals.get("pId"),
        Some(&JsValue::String("container".to_string()))
    );
    assert_eq!(engine.globals.get("hasItem"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("elemCount"), Some(&JsValue::Number(3.0)));

    // Verify mutations in the document tree
    assert!(
        engine.document.get_element_by_id("item1").is_none(),
        "item1 should have been removed"
    );
    assert!(
        engine.document.get_element_by_id("inserted-item").is_some(),
        "inserted-item should exist"
    );
    assert!(
        engine
            .document
            .get_element_by_id("replacement-item")
            .is_some(),
        "replacement-item should exist"
    );
    assert!(
        engine.document.get_element_by_id("item3").is_none(),
        "item3 should have been replaced"
    );

    let inserted = engine.document.get_element_by_id("inserted-item").unwrap();
    assert_eq!(inserted.text_content(), "Inserted Paragraph");
    assert_eq!(
        inserted.next_element_sibling().unwrap().id(),
        Some("item2".to_string())
    );
}

#[test]
fn test_lexical_scoping_and_closures() {
    let mut engine = JsEngine::new();

    let script = r#"
        function makeCounter(start) {
            var count = start;
            return function() {
                count = count + 1;
                return count;
            };
        }

        var counterA = makeCounter(10);
        var counterB = makeCounter(100);

        var a1 = counterA();
        var a2 = counterA();
        var b1 = counterB();
        var a3 = counterA();
        var b2 = counterB();

        function makeAdder(x) {
            return function(y) {
                return x + y;
            };
        }

        var add5 = makeAdder(5);
        var add20 = makeAdder(20);
        var res5 = add5(7);
        var res20 = add20(7);

        function multiplier(factor) {
            return (n) => n * factor;
        }

        var times3 = multiplier(3);
        var resTimes = times3(9);
    "#;

    engine.eval(script).expect("eval lexical scoping script");

    assert_eq!(engine.globals.get("a1"), Some(&JsValue::Number(11.0)));
    assert_eq!(engine.globals.get("a2"), Some(&JsValue::Number(12.0)));
    assert_eq!(engine.globals.get("b1"), Some(&JsValue::Number(101.0)));
    assert_eq!(engine.globals.get("a3"), Some(&JsValue::Number(13.0)));
    assert_eq!(engine.globals.get("b2"), Some(&JsValue::Number(102.0)));

    assert_eq!(engine.globals.get("res5"), Some(&JsValue::Number(12.0)));
    assert_eq!(engine.globals.get("res20"), Some(&JsValue::Number(27.0)));

    assert_eq!(engine.globals.get("resTimes"), Some(&JsValue::Number(27.0)));

    // Verify 'count' and 'start' are not leaked to global scope
    assert!(!engine.globals.contains_key("count"));
    assert!(!engine.globals.contains_key("start"));
    assert!(!engine.globals.contains_key("factor"));
}

#[test]
fn test_es6_classes_constructors_and_inheritance() {
    let mut engine = JsEngine::new();

    let script = r#"
        class Animal {
            constructor(name) {
                this.name = name;
            }
            speak() {
                return this.name + " makes a sound";
            }
        }

        class Dog extends Animal {
            constructor(name, breed) {
                super(name);
                this.breed = breed;
            }
            bark() {
                return this.name + " barks";
            }
            getBreed() {
                return this.breed;
            }
        }

        class Cat extends Animal {
            speak() {
                return super.speak() + " loudly";
            }
        }

        var d = new Dog("Rex", "German Shepherd");
        var dogSpeak = d.speak();
        var dogBark = d.bark();
        var dogBreed = d.getBreed();
        var dogName = d.name;

        // Instance mutation
        d.name = "Max";
        var dogNewBark = d.bark();

        // Subclass without explicit constructor calling super.speak()
        var c = new Cat("Whiskers");
        var catSpeak = c.speak();

        // Instanceof verification
        var isDog = d instanceof Dog;
        var isAnimal = d instanceof Animal;
        var isObject = d instanceof Object;
        var isCat = d instanceof Cat;

        var typeofDog = typeof Dog;
        var typeofInstance = typeof d;

        // Built-in Error constructor via new
        var myErr = new Error("Custom fatal error");
        var errMsg = myErr.message;
        var errName = myErr.name;
    "#;

    engine.eval(script).expect("eval ES6 class script");

    assert_eq!(
        engine.globals.get("dogSpeak"),
        Some(&JsValue::String("Rex makes a sound".to_string()))
    );
    assert_eq!(
        engine.globals.get("dogBark"),
        Some(&JsValue::String("Rex barks".to_string()))
    );
    assert_eq!(
        engine.globals.get("dogBreed"),
        Some(&JsValue::String("German Shepherd".to_string()))
    );
    assert_eq!(
        engine.globals.get("dogName"),
        Some(&JsValue::String("Rex".to_string()))
    );

    assert_eq!(
        engine.globals.get("dogNewBark"),
        Some(&JsValue::String("Max barks".to_string()))
    );

    assert_eq!(
        engine.globals.get("catSpeak"),
        Some(&JsValue::String(
            "Whiskers makes a sound loudly".to_string()
        ))
    );

    assert_eq!(engine.globals.get("isDog"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isAnimal"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isObject"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isCat"), Some(&JsValue::Bool(false)));

    assert_eq!(
        engine.globals.get("typeofDog"),
        Some(&JsValue::String("function".to_string()))
    );
    assert_eq!(
        engine.globals.get("typeofInstance"),
        Some(&JsValue::String("object".to_string()))
    );

    assert_eq!(
        engine.globals.get("errMsg"),
        Some(&JsValue::String("Custom fatal error".to_string()))
    );
    assert_eq!(
        engine.globals.get("errName"),
        Some(&JsValue::String("Error".to_string()))
    );
}

#[test]
fn test_es6_async_await_and_promise_microtasks() {
    let mut engine = JsEngine::new();

    let script = r#"
        // 1. Async function declaration returning value
        async function computeSum(a, b) {
            return a + b;
        }

        // 2. Async arrow function
        var doubleVal = async (x) => x * 2;

        // 3. Sequential await unpacking
        var sumResult = await computeSum(15, 27);
        var doubleResult = await doubleVal(21);

        // 4. Awaiting raw non-promise literal immediately unwraps
        var literalAwait = await 999;

        // 5. Promise.resolve and Promise.reject static methods
        var resolvedVal = await Promise.resolve("resolved data");

        // 6. Constructor via new Promise(executor)
        var customPromise = new Promise((resolve, reject) => {
            resolve("custom resolution");
        });
        var customRes = await customPromise;

        // 7. Promise.all with await
        var p1 = Promise.resolve(10);
        var p2 = Promise.resolve(20);
        var p3 = Promise.resolve(30);
        var allResults = await Promise.all([p1, p2, p3]);

        // 8. Try / catch around rejected async function
        async function throwingFunction() {
            throw "async computation failed";
        }

        var caughtAsyncErr = "";
        try {
            await throwingFunction();
        } catch (err) {
            caughtAsyncErr = err;
        }

        // 9. Try / catch around awaited Promise.reject
        var caughtRejectErr = "";
        try {
            await Promise.reject("explicit rejection");
        } catch (err) {
            caughtRejectErr = err;
        }

        // 10. Promise chaining with .then() and .catch()
        var chainResult = 0;
        Promise.resolve(50).then(x => x + 25).then(y => {
            chainResult = y;
        });

        var handledErr = "";
        Promise.reject("fatal error").catch(e => {
            handledErr = e;
        });
    "#;

    engine.eval(script).expect("eval async / await script");

    assert_eq!(
        engine.globals.get("sumResult"),
        Some(&JsValue::Number(42.0))
    );
    assert_eq!(
        engine.globals.get("doubleResult"),
        Some(&JsValue::Number(42.0))
    );
    assert_eq!(
        engine.globals.get("literalAwait"),
        Some(&JsValue::Number(999.0))
    );
    assert_eq!(
        engine.globals.get("resolvedVal"),
        Some(&JsValue::String("resolved data".to_string()))
    );
    assert_eq!(
        engine.globals.get("customRes"),
        Some(&JsValue::String("custom resolution".to_string()))
    );

    assert_eq!(
        engine.globals.get("allResults"),
        Some(&JsValue::new_array(vec![
            JsValue::Number(10.0),
            JsValue::Number(20.0),
            JsValue::Number(30.0),
        ]))
    );

    assert_eq!(
        engine.globals.get("caughtAsyncErr"),
        Some(&JsValue::String("async computation failed".to_string()))
    );
    assert_eq!(
        engine.globals.get("caughtRejectErr"),
        Some(&JsValue::String("explicit rejection".to_string()))
    );

    assert_eq!(
        engine.globals.get("chainResult"),
        Some(&JsValue::Number(75.0))
    );
    assert_eq!(
        engine.globals.get("handledErr"),
        Some(&JsValue::String("fatal error".to_string()))
    );
}

#[test]
fn test_optional_chaining_and_nullish_coalescing() {
    let mut engine = JsEngine::new();
    let script = r#"
        // 1. Nullish coalescing (??) semantics
        var nullResult = null ?? "fallback_null";
        var undefResult = undefined ?? "fallback_undef";
        var zeroResult = 0 ?? 42;
        var falseResult = false ?? true;
        var emptyStrResult = "" ?? "fallback_str";
        var chainCoalesce = null ?? undefined ?? "final_val";

        // 2. Short-circuiting in nullish coalescing
        var sideEffectCount = 0;
        function inc() {
            sideEffectCount = sideEffectCount + 1;
            return 999;
        }
        var shortCircuitVal = "existing" ?? inc();

        // 3. Optional chaining (?.) property access
        var user = {
            profile: {
                name: "Ada Lovelace",
                details: {
                    year: 1815
                }
            }
        };

        var validName = user?.profile?.name;
        var validYear = user?.profile?.details?.year;
        var missingDeep = user?.nonexistent?.deep?.prop;
        var nullObj = null;
        var nullAccess = nullObj?.something;
        var undefObj = undefined;
        var undefAccess = undefObj?.something;

        // 4. Optional chaining (?.[expr]) computed property access
        var key = "name";
        var computedValid = user?.profile?.[key];
        var computedShortCircuitKey = "should_not_evaluate";
        var computedMissing = nullObj?.[inc()];

        // 5. Optional chaining (?.()) call
        var objWithMethod = {
            multiply: function(x, y) {
                return x * y;
            }
        };
        var methodCall = objWithMethod?.multiply(6, 7);
        var missingMethodCall = nullObj?.multiply(inc());

        // 6. Optional direct function call
        var directFn = function(val) {
            return val + 10;
        };
        var calledFn = directFn?.(32);
        var missingFn = null;
        var skippedCall = missingFn?.(inc());

        // 7. Combined optional chaining and nullish coalescing
        var fallbackName = user?.unknownField?.prop ?? "Anonymous";
        var fallbackFound = user?.profile?.name ?? "Anonymous";
    "#;

    engine
        .eval(script)
        .expect("eval optional chaining and nullish coalescing script");

    // Nullish coalescing assertions
    assert_eq!(
        engine.globals.get("nullResult"),
        Some(&JsValue::String("fallback_null".to_string()))
    );
    assert_eq!(
        engine.globals.get("undefResult"),
        Some(&JsValue::String("fallback_undef".to_string()))
    );
    assert_eq!(
        engine.globals.get("zeroResult"),
        Some(&JsValue::Number(0.0))
    );
    assert_eq!(
        engine.globals.get("falseResult"),
        Some(&JsValue::Bool(false))
    );
    assert_eq!(
        engine.globals.get("emptyStrResult"),
        Some(&JsValue::String("".to_string()))
    );
    assert_eq!(
        engine.globals.get("chainCoalesce"),
        Some(&JsValue::String("final_val".to_string()))
    );

    // Side-effects should NOT have fired for 'existing' ?? inc()
    assert_eq!(
        engine.globals.get("shortCircuitVal"),
        Some(&JsValue::String("existing".to_string()))
    );
    assert_eq!(
        engine.globals.get("sideEffectCount"),
        Some(&JsValue::Number(0.0))
    );

    // Optional chaining property access assertions
    assert_eq!(
        engine.globals.get("validName"),
        Some(&JsValue::String("Ada Lovelace".to_string()))
    );
    assert_eq!(
        engine.globals.get("validYear"),
        Some(&JsValue::Number(1815.0))
    );
    assert_eq!(engine.globals.get("missingDeep"), Some(&JsValue::Undefined));
    assert_eq!(engine.globals.get("nullAccess"), Some(&JsValue::Undefined));
    assert_eq!(engine.globals.get("undefAccess"), Some(&JsValue::Undefined));

    // Computed property assertions
    assert_eq!(
        engine.globals.get("computedValid"),
        Some(&JsValue::String("Ada Lovelace".to_string()))
    );
    assert_eq!(
        engine.globals.get("computedMissing"),
        Some(&JsValue::Undefined)
    );

    // Method and call assertions
    assert_eq!(
        engine.globals.get("methodCall"),
        Some(&JsValue::Number(42.0))
    );
    assert_eq!(
        engine.globals.get("missingMethodCall"),
        Some(&JsValue::Undefined)
    );
    assert_eq!(engine.globals.get("calledFn"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("skippedCall"), Some(&JsValue::Undefined));

    // Verify sideEffectCount remained 0 across all short-circuited optional calls
    assert_eq!(
        engine.globals.get("sideEffectCount"),
        Some(&JsValue::Number(0.0))
    );

    // Combined assertions
    assert_eq!(
        engine.globals.get("fallbackName"),
        Some(&JsValue::String("Anonymous".to_string()))
    );
    assert_eq!(
        engine.globals.get("fallbackFound"),
        Some(&JsValue::String("Ada Lovelace".to_string()))
    );
}

#[test]
fn test_switch_case_break_continue() {
    let mut engine = JsEngine::new();
    let script = r#"
        // 1. Switch with breaks
        function getStatusDesc(code) {
            var desc = "unknown";
            switch (code) {
                case 200:
                    desc = "OK";
                    break;
                case 404:
                    desc = "Not Found";
                    break;
                case 500:
                    desc = "Internal Error";
                    break;
                default:
                    desc = "Unhandled";
                    break;
            }
            return desc;
        }

        var status200 = getStatusDesc(200);
        var status404 = getStatusDesc(404);
        var statusDefault = getStatusDesc(999);

        // 2. Switch fallthrough (when break is omitted)
        var cumulativePoints = 0;
        var tier = 1;
        switch (tier) {
            case 1:
                cumulativePoints = cumulativePoints + 100;
            case 2:
                cumulativePoints = cumulativePoints + 50;
                break;
            case 3:
                cumulativePoints = cumulativePoints + 10;
                break;
        }

        // 3. While loop with break and continue
        var whileSum = 0;
        var i = 0;
        while (i < 20) {
            i = i + 1;
            if (i % 2 === 0) {
                continue;
            }
            if (i > 9) {
                break;
            }
            whileSum = whileSum + i; // accumulates odd numbers 1, 3, 5, 7, 9 = 25
        }

        // 4. For loop with break and continue
        var forSum = 0;
        for (var j = 0; j < 15; j = j + 1) {
            if (j === 3 || j === 7) {
                continue;
            }
            if (j >= 10) {
                break;
            }
            forSum = forSum + j; // sum 0..9 except 3 and 7: 45 - 3 - 7 = 35
        }

        // 5. Switch inside loop with break and continue
        var loopSwitchHits = 0;
        for (var k = 0; k < 5; k = k + 1) {
            switch (k) {
                case 0:
                case 1:
                    loopSwitchHits = loopSwitchHits + 1;
                    break;
                case 2:
                    continue; // continues the enclosing loop!
                default:
                    loopSwitchHits = loopSwitchHits + 10;
                    break;
            }
        }
    "#;

    engine
        .eval(script)
        .expect("eval switch case break continue");

    assert_eq!(
        engine.globals.get("status200"),
        Some(&JsValue::String("OK".to_string()))
    );
    assert_eq!(
        engine.globals.get("status404"),
        Some(&JsValue::String("Not Found".to_string()))
    );
    assert_eq!(
        engine.globals.get("statusDefault"),
        Some(&JsValue::String("Unhandled".to_string()))
    );

    // Fallthrough: case 1 (+100) falls through into case 2 (+50) then breaks -> 150
    assert_eq!(
        engine.globals.get("cumulativePoints"),
        Some(&JsValue::Number(150.0))
    );

    // While loop: 1 + 3 + 5 + 7 + 9 = 25 (10 was even, skipped; at 11 > 9 breaks)
    assert_eq!(engine.globals.get("whileSum"), Some(&JsValue::Number(25.0)));
    assert_eq!(engine.globals.get("i"), Some(&JsValue::Number(11.0)));

    // For loop: 0 + 1 + 2 + 4 + 5 + 6 + 8 + 9 = 35
    assert_eq!(engine.globals.get("forSum"), Some(&JsValue::Number(35.0)));
    assert_eq!(engine.globals.get("j"), Some(&JsValue::Number(10.0)));

    // Loop with switch:
    // k = 0: hits +1 -> 1
    // k = 1: hits +1 -> 2
    // k = 2: continue loop
    // k = 3: default hits +10 -> 12
    // k = 4: default hits +10 -> 22
    assert_eq!(
        engine.globals.get("loopSwitchHits"),
        Some(&JsValue::Number(22.0))
    );

    // Illegal break outside loop/switch should error
    let bad_break = engine.eval("break;");
    assert!(bad_break.is_err(), "Illegal top-level break must error");

    // Illegal continue outside loop should error
    let bad_continue = engine.eval("continue;");
    assert!(
        bad_continue.is_err(),
        "Illegal top-level continue must error"
    );
}

#[test]
fn test_destructuring_and_spread_rest() {
    let mut engine = JsEngine::new();

    let script = r#"
        // 1. Object Destructuring Declaration (renaming, defaults, rest)
        const sourceObj = { a: 10, b: 20, extra1: "foo", extra2: "bar" };
        const { a, b: customB, c = 100, d: customD = 200, ...restProps } = sourceObj;

        // 2. Array Destructuring Declaration (elision, defaults, rest)
        const sourceArr = [1, 2, 3, 4, 5];
        const [ first, , third = 99, missingWithDefault = 42, ...tailArr ] = sourceArr;

        // 3. Destructuring Assignment (Swap pattern & existing variables)
        var x = 111;
        var y = 222;
        [x, y] = [y, x];

        // 4. Object Destructuring Assignment
        var objP = 0;
        var objQ = 0;
        var restObjAssigned = null;
        ({ p: objP, q: objQ, ...restObjAssigned } = { p: 77, q: 88, z1: "zed1", z2: "zed2" });

        // 5. Array Spread Literals
        const middle = [20, 30];
        const fullArr = [10, ...middle, 40, 50];

        // 6. Object Spread Literals with Shorthand & Overrides
        const defaultSettings = { theme: "dark", fontSize: 14, logging: false };
        const theme = "solarized";
        const userSettings = { ...defaultSettings, theme, logging: true };

        // 7. Function Rest Parameters and Call Spread
        function accumulate(multiplier, ...nums) {
            let total = 0;
            for (let idx = 0; idx < nums.length; idx = idx + 1) {
                total = total + nums[idx];
            }
            return total * multiplier;
        }

        const numbersToSum = [5, 10, 15];
        const multResult = accumulate(2, ...numbersToSum);

        // 8. Arrow Function Rest Parameters
        const formatTags = (prefix, ...tags) => tags.map(t => prefix + "-" + t);
        const tagList = ["alpha", "beta"];
        const formatted = formatTags("tag", ...tagList);

        // 9. ES6 Class with Rest in Constructor and Methods
        class Collection {
            constructor(name, ...items) {
                this.name = name;
                this.items = items;
            }
            count() {
                return this.items.length;
            }
            append(...newItems) {
                return this.items.concat(newItems);
            }
        }

        const initialItems = ["apple", "banana"];
        const col = new Collection("fruits", ...initialItems);
        const colCount = col.count();
        const expandedItems = col.append("cherry", "date");
    "#;

    engine
        .eval(script)
        .expect("eval destructuring and spread/rest");

    // 1. Object Destructuring assertions
    assert_eq!(engine.globals.get("a"), Some(&JsValue::Number(10.0)));
    assert_eq!(engine.globals.get("customB"), Some(&JsValue::Number(20.0)));
    assert_eq!(engine.globals.get("c"), Some(&JsValue::Number(100.0)));
    assert_eq!(engine.globals.get("customD"), Some(&JsValue::Number(200.0)));
    if let Some(JsValue::Object(map)) = engine.globals.get("restProps") {
        let b = map.borrow();
        assert_eq!(b.get("extra1"), Some(&JsValue::String("foo".to_string())));
        assert_eq!(b.get("extra2"), Some(&JsValue::String("bar".to_string())));
        assert_eq!(b.get("a"), None);
        assert_eq!(b.get("b"), None);
    } else {
        panic!("restProps must be an object");
    }

    // 2. Array Destructuring assertions
    assert_eq!(engine.globals.get("first"), Some(&JsValue::Number(1.0)));
    assert_eq!(engine.globals.get("third"), Some(&JsValue::Number(3.0)));
    assert_eq!(
        engine.globals.get("missingWithDefault"),
        Some(&JsValue::Number(4.0))
    );
    if let Some(JsValue::Array(tail)) = engine.globals.get("tailArr") {
        assert_eq!(*tail.borrow(), vec![JsValue::Number(5.0)]);
    } else {
        panic!("tailArr must be an array");
    }

    // 3. Swap pattern
    assert_eq!(engine.globals.get("x"), Some(&JsValue::Number(222.0)));
    assert_eq!(engine.globals.get("y"), Some(&JsValue::Number(111.0)));

    // 4. Object destructuring assignment
    assert_eq!(engine.globals.get("objP"), Some(&JsValue::Number(77.0)));
    assert_eq!(engine.globals.get("objQ"), Some(&JsValue::Number(88.0)));
    if let Some(JsValue::Object(map)) = engine.globals.get("restObjAssigned") {
        let b = map.borrow();
        assert_eq!(b.get("z1"), Some(&JsValue::String("zed1".to_string())));
        assert_eq!(b.get("z2"), Some(&JsValue::String("zed2".to_string())));
    } else {
        panic!("restObjAssigned must be an object");
    }

    // 5. Array Spread Literals
    if let Some(JsValue::Array(items)) = engine.globals.get("fullArr") {
        assert_eq!(
            *items.borrow(),
            vec![
                JsValue::Number(10.0),
                JsValue::Number(20.0),
                JsValue::Number(30.0),
                JsValue::Number(40.0),
                JsValue::Number(50.0)
            ]
        );
    } else {
        panic!("fullArr must be an array");
    }

    // 6. Object Spread Literals with Shorthand & Overrides
    if let Some(JsValue::Object(settings)) = engine.globals.get("userSettings") {
        let b = settings.borrow();
        assert_eq!(
            b.get("theme"),
            Some(&JsValue::String("solarized".to_string()))
        );
        assert_eq!(b.get("fontSize"), Some(&JsValue::Number(14.0)));
        assert_eq!(b.get("logging"), Some(&JsValue::Bool(true)));
    } else {
        panic!("userSettings must be an object");
    }

    // 7. Function Rest Parameters and Call Spread
    // (5 + 10 + 15) * 2 = 60
    assert_eq!(
        engine.globals.get("multResult"),
        Some(&JsValue::Number(60.0))
    );

    // 8. Arrow Function Rest Parameters
    if let Some(JsValue::Array(tags)) = engine.globals.get("formatted") {
        assert_eq!(
            *tags.borrow(),
            vec![
                JsValue::String("tag-alpha".to_string()),
                JsValue::String("tag-beta".to_string())
            ]
        );
    } else {
        panic!("formatted must be an array");
    }

    // 9. ES6 Class with Rest in Constructor and Methods
    assert_eq!(engine.globals.get("colCount"), Some(&JsValue::Number(2.0)));
    if let Some(JsValue::Array(expanded)) = engine.globals.get("expandedItems") {
        assert_eq!(
            *expanded.borrow(),
            vec![
                JsValue::String("apple".to_string()),
                JsValue::String("banana".to_string()),
                JsValue::String("cherry".to_string()),
                JsValue::String("date".to_string())
            ]
        );
    } else {
        panic!("expandedItems must be an array");
    }
}

#[test]
fn test_regular_expression_literals_and_regex_methods() {
    let mut engine = JsEngine::new();

    let script = r#"
        // 1. Disambiguation: Division arithmetic vs Regex literal
        const quotient = 100 / 2 / 5;
        const reAlpha = /[a-z]+/i;

        // 2. Regex literal property access
        const patternSource = reAlpha.source;
        const patternFlags = reAlpha.flags;
        const isIgnoreCase = reAlpha.ignoreCase;
        const isGlobal = reAlpha.global;

        // 3. re.test()
        const testMatch = reAlpha.test("HelloWorld123");
        const testFail = /^[0-9]+$/.test("abc");

        // 4. re.exec() with capture groups
        const dateRe = /^([0-9]{4})-([0-9]{2})-([0-9]{2})$/;
        const execMatch = dateRe.exec("2026-11-08");
        const execFail = dateRe.exec("invalid-date");

        // 5. re.toString()
        const reStr = reAlpha.toString();

        // 6. new RegExp() and RegExp() constructors
        const dynamicRe = new RegExp("[0-9]+", "g");
        const funcRe = RegExp("foo.*bar", "i");
        const dynamicMatch = dynamicRe.test("Number 42 is here");
        const funcMatch = funcRe.test("FOO and BAR");

        // 7. str.match() with and without 'g'
        const sentence = "The quick brown fox jumps over 10 lazy dogs and 42 cats.";
        const matchDigitsGlobal = sentence.match(/[0-9]+/g);
        const matchFirstWord = sentence.match(/([a-z]+)/i);

        // 8. str.search()
        const searchIdx = sentence.search(/fox/);
        const searchNotFound = sentence.search(/elephant/);

        // 9. str.replace() and str.replaceAll() with Regex and String
        const replacedFirst = "foo bar foo bar".replace(/foo/, "qux");
        const replacedGlobal = "foo bar foo bar".replace(/foo/g, "qux");
        const replacedAllStr = "cat and cat".replaceAll("cat", "dog");

        // 10. str.split() with Regex
        const csvLine = "apple,  banana  , cherry,date";
        const splitTokens = csvLine.split(/\s*,\s*/);

        // 11. String helper methods
        const trimmed = "   strata engine   ".trim();
        const upper = "strata".toUpperCase();
        const lower = "STRATA".toLowerCase();
        const sliced = "Hello World".slice(0, 5);
        const hasSub = "Antigravity".includes("gravity");
        const startsWithAnti = "Antigravity".startsWith("Anti");
    "#;

    engine
        .eval(script)
        .expect("eval regular expressions and string methods");

    // 1. Division vs Regex
    assert_eq!(engine.globals.get("quotient"), Some(&JsValue::Number(10.0)));

    // 2. Regex properties
    assert_eq!(
        engine.globals.get("patternSource"),
        Some(&JsValue::String("[a-z]+".to_string()))
    );
    assert_eq!(
        engine.globals.get("patternFlags"),
        Some(&JsValue::String("i".to_string()))
    );
    assert_eq!(
        engine.globals.get("isIgnoreCase"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(engine.globals.get("isGlobal"), Some(&JsValue::Bool(false)));

    // 3. re.test()
    assert_eq!(engine.globals.get("testMatch"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("testFail"), Some(&JsValue::Bool(false)));

    // 4. re.exec()
    if let Some(JsValue::Array(ref caps)) = engine.globals.get("execMatch") {
        let b = caps.borrow();
        assert_eq!(b.len(), 4);
        assert_eq!(b[0], JsValue::String("2026-11-08".to_string()));
        assert_eq!(b[1], JsValue::String("2026".to_string()));
        assert_eq!(b[2], JsValue::String("11".to_string()));
        assert_eq!(b[3], JsValue::String("08".to_string()));
    } else {
        panic!("execMatch must be an array of captured groups");
    }
    assert_eq!(engine.globals.get("execFail"), Some(&JsValue::Null));

    // 5. re.toString()
    assert_eq!(
        engine.globals.get("reStr"),
        Some(&JsValue::String("/[a-z]+/i".to_string()))
    );

    // 6. new RegExp() & RegExp()
    assert_eq!(
        engine.globals.get("dynamicMatch"),
        Some(&JsValue::Bool(true))
    );
    assert_eq!(engine.globals.get("funcMatch"), Some(&JsValue::Bool(true)));

    // 7. str.match()
    if let Some(JsValue::Array(ref matches)) = engine.globals.get("matchDigitsGlobal") {
        assert_eq!(
            *matches.borrow(),
            vec![
                JsValue::String("10".to_string()),
                JsValue::String("42".to_string())
            ]
        );
    } else {
        panic!("matchDigitsGlobal must be an array");
    }
    if let Some(JsValue::Array(ref matches)) = engine.globals.get("matchFirstWord") {
        let b = matches.borrow();
        assert_eq!(b.first(), Some(&JsValue::String("The".to_string())));
    } else {
        panic!("matchFirstWord must be an array");
    }

    // 8. str.search()
    assert_eq!(
        engine.globals.get("searchIdx"),
        Some(&JsValue::Number(16.0))
    );
    assert_eq!(
        engine.globals.get("searchNotFound"),
        Some(&JsValue::Number(-1.0))
    );

    // 9. str.replace() and str.replaceAll()
    assert_eq!(
        engine.globals.get("replacedFirst"),
        Some(&JsValue::String("qux bar foo bar".to_string()))
    );
    assert_eq!(
        engine.globals.get("replacedGlobal"),
        Some(&JsValue::String("qux bar qux bar".to_string()))
    );
    assert_eq!(
        engine.globals.get("replacedAllStr"),
        Some(&JsValue::String("dog and dog".to_string()))
    );

    // 10. str.split()
    if let Some(JsValue::Array(ref parts)) = engine.globals.get("splitTokens") {
        assert_eq!(
            *parts.borrow(),
            vec![
                JsValue::String("apple".to_string()),
                JsValue::String("banana".to_string()),
                JsValue::String("cherry".to_string()),
                JsValue::String("date".to_string()),
            ]
        );
    } else {
        panic!("splitTokens must be an array");
    }

    // 11. String helpers
    assert_eq!(
        engine.globals.get("trimmed"),
        Some(&JsValue::String("strata engine".to_string()))
    );
    assert_eq!(
        engine.globals.get("upper"),
        Some(&JsValue::String("STRATA".to_string()))
    );
    assert_eq!(
        engine.globals.get("lower"),
        Some(&JsValue::String("strata".to_string()))
    );
    assert_eq!(
        engine.globals.get("sliced"),
        Some(&JsValue::String("Hello".to_string()))
    );
    assert_eq!(engine.globals.get("hasSub"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("startsWithAnti"),
        Some(&JsValue::Bool(true))
    );
}

#[test]
fn test_utf8_cursor_parser_zero_allocation_overhead() {
    use strata_js::engine::JsParser;

    // Multibyte UTF-8 JavaScript source containing Chinese, Cyrillic, accented characters,
    // and escaped strings, ensuring byte-cursor indexing properly navigates multibyte boundaries.
    let script = r#"
        // 这是一个中文单行注释
        /* Многострочный комментарий на кириллице
           с переносом строк и специальными символами */
        const 变量_x = 123;
        const greeting = "你好，世界！";
        const cyrillic = "Привет, мир!";
        const accented = "Café résumé naïve";

        function 计算(a, b) {
            return a + b;
        }

        const 结果 = 计算(变量_x, 456);
    "#;

    let mut parser = JsParser::new(script);
    let stmts = parser
        .parse_program()
        .expect("JsParser must parse multibyte UTF-8 program");
    assert!(
        stmts.len() >= 5,
        "Must parse all declarations and statements"
    );

    let mut engine = JsEngine::new();
    let res = engine.eval(script);
    assert!(
        res.is_ok(),
        "Evaluation of multibyte UTF-8 script must succeed: {:?}",
        res
    );

    assert_eq!(engine.globals.get("变量_x"), Some(&JsValue::Number(123.0)));
    assert_eq!(
        engine.globals.get("greeting"),
        Some(&JsValue::String("你好，世界！".to_string()))
    );
    assert_eq!(
        engine.globals.get("cyrillic"),
        Some(&JsValue::String("Привет, мир!".to_string()))
    );
    assert_eq!(
        engine.globals.get("accented"),
        Some(&JsValue::String("Café résumé naïve".to_string()))
    );
    assert_eq!(engine.globals.get("结果"), Some(&JsValue::Number(579.0)));
}

#[test]
fn test_lexical_closure_independence_no_clobbering() {
    let mut engine = JsEngine::new();
    let script = r#"
        function makeCounter(start) {
            var count = start;
            function increment() {
                count = count + 1;
                return count;
            }
            return increment;
        }

        var counterA = makeCounter(10);
        var counterB = makeCounter(100);

        var a1 = counterA();
        var a2 = counterA();
        var b1 = counterB();
        var a3 = counterA();
        var b2 = counterB();
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    assert_eq!(engine.globals.get("a1"), Some(&JsValue::Number(11.0)));
    assert_eq!(engine.globals.get("a2"), Some(&JsValue::Number(12.0)));
    assert_eq!(engine.globals.get("b1"), Some(&JsValue::Number(101.0)));
    assert_eq!(engine.globals.get("a3"), Some(&JsValue::Number(13.0)));
    assert_eq!(engine.globals.get("b2"), Some(&JsValue::Number(102.0)));
}

#[test]
fn test_function_shadowing_and_scope_isolation() {
    let mut engine = JsEngine::new();
    let script = r#"
        function helper() {
            return "global";
        }

        function runTest() {
            function helper() {
                return "local";
            }
            return helper();
        }

        var localRes = runTest();
        var globalRes = helper();
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    assert_eq!(
        engine.globals.get("localRes"),
        Some(&JsValue::String("local".to_string()))
    );
    assert_eq!(
        engine.globals.get("globalRes"),
        Some(&JsValue::String("global".to_string()))
    );
}

#[test]
fn test_recursive_named_inner_closure() {
    let mut engine = JsEngine::new();
    let script = r#"
        function createFactorial() {
            function fact(n) {
                if (n <= 1) {
                    return 1;
                }
                return n * fact(n - 1);
            }
            return fact;
        }

        var f = createFactorial();
        var res5 = f(5);
        var res6 = f(6);
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    assert_eq!(engine.globals.get("res5"), Some(&JsValue::Number(120.0)));
    assert_eq!(engine.globals.get("res6"), Some(&JsValue::Number(720.0)));
}

#[test]
fn test_arrow_function_memory_reclamation_bounded_closures() {
    let mut engine = JsEngine::new();
    let script = r#"
        var total = 0;
        for (var i = 0; i < 200; i = i + 1) {
            var arr = [1, 2, 3];
            var doubled = arr.map(x => x * 2);
            total = total + doubled[0];
        }
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    assert_eq!(engine.globals.get("total"), Some(&JsValue::Number(400.0)));
    // All temporary arrow functions must have been pruned by GC
    assert!(
        engine.closures.len() < 50,
        "Closures count must remain tightly bounded, got {}",
        engine.closures.len()
    );
}

#[test]
fn test_live_closure_preservation_after_pruning() {
    let mut engine = JsEngine::new();
    let script = r#"
        function makeMult(factor) {
            return n => n * factor;
        }
        function buildMultipliers() {
            var list = [];
            for (var i = 1; i <= 5; i = i + 1) {
                list.push(makeMult(i));
            }
            return list;
        }

        var mults = buildMultipliers();
        var val1 = mults[0](10);
        var val2 = mults[1](10);
        var val3 = mults[2](10);
        var val4 = mults[3](10);
        var val5 = mults[4](10);
    "#;

    let res = engine.eval(script);
    assert!(res.is_ok(), "Evaluation must succeed: {:?}", res);

    assert_eq!(engine.globals.get("val1"), Some(&JsValue::Number(10.0)));
    assert_eq!(engine.globals.get("val2"), Some(&JsValue::Number(20.0)));
    assert_eq!(engine.globals.get("val3"), Some(&JsValue::Number(30.0)));
    assert_eq!(engine.globals.get("val4"), Some(&JsValue::Number(40.0)));
    assert_eq!(engine.globals.get("val5"), Some(&JsValue::Number(50.0)));

    // Explicitly prune closures and verify preserved closures still execute
    engine.prune_dead_closures();
    let script2 = r#"
        var val5_repeat = mults[4](10);
    "#;
    let res2 = engine.eval(script2);
    assert!(
        res2.is_ok(),
        "Post-pruning evaluation must succeed: {:?}",
        res2
    );
    assert_eq!(
        engine.globals.get("val5_repeat"),
        Some(&JsValue::Number(50.0))
    );
}

#[test]
fn test_call_stack_recursion_limit_guard() {
    let mut engine = JsEngine::new();
    let script = r#"
        function blowStack() {
            return blowStack();
        }
        blowStack();
    "#;

    let res = engine.eval(script);
    assert!(
        res.is_err(),
        "Infinite recursion must be caught by call stack guard"
    );
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("Maximum call stack size exceeded"),
        "Error message must indicate call stack exceeded, got: {err_msg}"
    );
}

#[test]
fn test_async_await_fetch_and_json_unpacking() {
    let mut engine = JsEngine::new();
    let script = r#"
        async function runRequest() {
            var response = await fetch("mock://api.strata.internal/v1/metrics");
            var status = response.status;
            var isOk = response.ok;
            var contentType = response.headers.get("content-type");
            var body = await response.json();
            return {
                status: status,
                ok: isOk,
                contentType: contentType,
                payload: body
            };
        }

        var result = await runRequest();
        var resStatus = result.status;
        var resOk = result.ok;
        var resContentType = result.contentType;
        var resPayloadStatus = result.payload.status;
    "#;

    let res = engine.eval(script);
    assert!(
        res.is_ok(),
        "Evaluation of fetch and json await must succeed: {:?}",
        res
    );

    assert_eq!(
        engine.globals.get("resStatus"),
        Some(&JsValue::Number(200.0))
    );
    assert_eq!(engine.globals.get("resOk"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("resContentType"),
        Some(&JsValue::String("application/json".to_string()))
    );
    assert_eq!(
        engine.globals.get("resPayloadStatus"),
        Some(&JsValue::String("ok".to_string()))
    );
}

#[test]
fn test_async_await_sequential_chained_promises() {
    let mut engine = JsEngine::new();
    let script = r#"
        async function step1(n) {
            return n + 10;
        }

        async function step2(n) {
            return n * 2;
        }

        async function step3(n) {
            return n - 4;
        }

        async function pipeline(input) {
            var a = await step1(input);
            var b = await step2(a);
            var c = await step3(b);
            return c;
        }

        var finalResult = await pipeline(5);
    "#;

    let res = engine.eval(script);
    assert!(
        res.is_ok(),
        "Sequential async await pipeline must succeed: {:?}",
        res
    );
    // (5 + 10) * 2 - 4 = 30 - 4 = 26
    assert_eq!(
        engine.globals.get("finalResult"),
        Some(&JsValue::Number(26.0))
    );
}

#[test]
fn test_async_await_deferred_promise_resolution() {
    let mut engine = JsEngine::new();
    let script = r#"
        var deferredPromise = new Promise((resolve) => {
            Promise.resolve(777).then(resolve);
        });

        var unpacked = await deferredPromise;
    "#;

    let res = engine.eval(script);
    assert!(
        res.is_ok(),
        "Evaluation of deferred promise resolution must succeed: {:?}",
        res
    );
    assert_eq!(
        engine.globals.get("unpacked"),
        Some(&JsValue::Number(777.0))
    );
}

#[test]
fn test_infinite_while_loop_timeout_guard() {
    let mut engine = JsEngine::new();
    let res = engine.eval("while (true) {}");
    assert!(res.is_err(), "Infinite while loop must return RangeError");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("Maximum loop iteration limit exceeded"),
        "Expected timeout error, got: {err_msg}"
    );
}

#[test]
fn test_infinite_for_loop_timeout_guard() {
    let mut engine = JsEngine::new();
    let res = engine.eval("for (;;) {}");
    assert!(res.is_err(), "Infinite for loop must return RangeError");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("Maximum loop iteration limit exceeded"),
        "Expected timeout error, got: {err_msg}"
    );
}

#[test]
fn test_nested_object_property_assignment() {
    let mut engine = JsEngine::new();
    let script = r#"
        var user = {
            profile: {
                settings: {
                    theme: "dark"
                }
            }
        };
        user.profile.settings.theme = "light";
        user.profile.settings.fontSize = 16;
        var theme = user.profile.settings.theme;
        var fontSize = user.profile.settings.fontSize;
    "#;
    let res = engine.eval(script);
    assert!(
        res.is_ok(),
        "Nested property assignment must succeed: {:?}",
        res
    );
    assert_eq!(
        engine.globals.get("theme"),
        Some(&JsValue::String("light".to_string()))
    );
    assert_eq!(engine.globals.get("fontSize"), Some(&JsValue::Number(16.0)));
}

#[test]
fn test_prototype_pollution_guard_in_frozen_mode() {
    let mut engine = JsEngine::new();
    engine.set_freeze_prototypes(true);

    let res = engine.eval("Object.prototype.isAdmin = true;");
    assert!(
        res.is_err(),
        "Mutating Object.prototype in frozen mode must return TypeError"
    );
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("TypeError: Cannot modify frozen prototype Object.prototype"),
        "Expected TypeError for frozen prototype, got: {err_msg}"
    );

    let res_proto = engine.eval("var x = {}; x.__proto__ = null;");
    assert!(
        res_proto.is_err(),
        "Mutating __proto__ in frozen mode must fail"
    );
}

#[test]
fn test_memory_allocation_quota_enforcement() {
    // Set 64 KB memory quota
    let mut engine = JsEngine::new().with_memory_limit(64 * 1024);

    // Creating an oversized array must trigger RangeError
    let res = engine.eval("var huge = new Array(100000);");
    assert!(res.is_err(), "Allocating array exceeding quota must fail");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("RangeError: Script memory allocation quota exceeded"),
        "Expected memory quota error, got: {err_msg}"
    );

    // Pushing in a loop past the limit must also trigger RangeError
    let mut engine_push = JsEngine::new().with_memory_limit(1024);
    let push_script = r#"
        var a = [];
        for (var i = 0; i < 1000; i++) {
            a.push(i);
        }
    "#;
    let push_res = engine_push.eval(push_script);
    assert!(push_res.is_err(), "Push loop exceeding quota must fail");
    let push_err = push_res.unwrap_err();
    assert!(
        push_err.contains("RangeError: Script memory allocation quota exceeded"),
        "Expected memory quota error, got: {push_err}"
    );
}
