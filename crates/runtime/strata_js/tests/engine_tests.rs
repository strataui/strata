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
    assert_eq!(counter.get_attribute("data-status"), Some("active".to_string()));

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
    assert_eq!(engine.globals.get("caught"), Some(&JsValue::String("critical failure".to_string())));

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
        Some(&JsValue::Array(vec![
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
        Some(&JsValue::String("Hello, Alice! You have 6 pending notifications.".to_string()))
    );
    assert_eq!(
        engine.globals.get("simple"),
        Some(&JsValue::String("Plain literal string without interpolation".to_string()))
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

    engine.eval(script).expect("Arrow & array methods script failed");

    assert_eq!(engine.globals.get("d_res"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("a_res"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("c_res"), Some(&JsValue::Number(36.0)));

    assert_eq!(
        engine.globals.get("mapped"),
        Some(&JsValue::Array(vec![
            JsValue::Number(10.0),
            JsValue::Number(20.0),
            JsValue::Number(30.0),
            JsValue::Number(40.0),
            JsValue::Number(50.0),
        ]))
    );
    assert_eq!(
        engine.globals.get("filtered"),
        Some(&JsValue::Array(vec![
            JsValue::Number(3.0),
            JsValue::Number(4.0),
            JsValue::Number(5.0),
        ]))
    );
    assert_eq!(engine.globals.get("found"), Some(&JsValue::Number(2.0)));
    assert_eq!(engine.globals.get("foundIdx"), Some(&JsValue::Number(3.0)));
    assert_eq!(engine.globals.get("hasEven"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("allPositive"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("sum"), Some(&JsValue::Number(15.0)));
    assert_eq!(
        engine.globals.get("sliced"),
        Some(&JsValue::Array(vec![
            JsValue::Number(2.0),
            JsValue::Number(3.0),
            JsValue::Number(4.0),
        ]))
    );
    assert_eq!(engine.globals.get("joined"), Some(&JsValue::String("1-2-3-4-5".to_string())));
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

    assert_eq!(item2.parent_element().unwrap().id(), Some("container".to_string()));
    assert!(container.contains(&item1));
    assert!(container.contains(&item2));
    assert!(container.contains(&item3));
    assert_eq!(item2.previous_element_sibling().unwrap().id(), Some("item1".to_string()));
    assert_eq!(item2.next_element_sibling().unwrap().id(), Some("item3".to_string()));

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

    engine.eval(script).expect("DOM hierarchy JS evaluation failed");

    assert_eq!(engine.globals.get("pId"), Some(&JsValue::String("container".to_string())));
    assert_eq!(engine.globals.get("hasItem"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("elemCount"), Some(&JsValue::Number(3.0)));

    // Verify mutations in the document tree
    assert!(engine.document.get_element_by_id("item1").is_none(), "item1 should have been removed");
    assert!(engine.document.get_element_by_id("inserted-item").is_some(), "inserted-item should exist");
    assert!(engine.document.get_element_by_id("replacement-item").is_some(), "replacement-item should exist");
    assert!(engine.document.get_element_by_id("item3").is_none(), "item3 should have been replaced");

    let inserted = engine.document.get_element_by_id("inserted-item").unwrap();
    assert_eq!(inserted.text_content(), "Inserted Paragraph");
    assert_eq!(inserted.next_element_sibling().unwrap().id(), Some("item2".to_string()));
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

    assert_eq!(engine.globals.get("dogSpeak"), Some(&JsValue::String("Rex makes a sound".to_string())));
    assert_eq!(engine.globals.get("dogBark"), Some(&JsValue::String("Rex barks".to_string())));
    assert_eq!(engine.globals.get("dogBreed"), Some(&JsValue::String("German Shepherd".to_string())));
    assert_eq!(engine.globals.get("dogName"), Some(&JsValue::String("Rex".to_string())));

    assert_eq!(engine.globals.get("dogNewBark"), Some(&JsValue::String("Max barks".to_string())));

    assert_eq!(engine.globals.get("catSpeak"), Some(&JsValue::String("Whiskers makes a sound loudly".to_string())));

    assert_eq!(engine.globals.get("isDog"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isAnimal"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isObject"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("isCat"), Some(&JsValue::Bool(false)));

    assert_eq!(engine.globals.get("typeofDog"), Some(&JsValue::String("function".to_string())));
    assert_eq!(engine.globals.get("typeofInstance"), Some(&JsValue::String("object".to_string())));

    assert_eq!(engine.globals.get("errMsg"), Some(&JsValue::String("Custom fatal error".to_string())));
    assert_eq!(engine.globals.get("errName"), Some(&JsValue::String("Error".to_string())));
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

    assert_eq!(engine.globals.get("sumResult"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("doubleResult"), Some(&JsValue::Number(42.0)));
    assert_eq!(engine.globals.get("literalAwait"), Some(&JsValue::Number(999.0)));
    assert_eq!(engine.globals.get("resolvedVal"), Some(&JsValue::String("resolved data".to_string())));
    assert_eq!(engine.globals.get("customRes"), Some(&JsValue::String("custom resolution".to_string())));

    assert_eq!(
        engine.globals.get("allResults"),
        Some(&JsValue::Array(vec![
            JsValue::Number(10.0),
            JsValue::Number(20.0),
            JsValue::Number(30.0),
        ]))
    );

    assert_eq!(engine.globals.get("caughtAsyncErr"), Some(&JsValue::String("async computation failed".to_string())));
    assert_eq!(engine.globals.get("caughtRejectErr"), Some(&JsValue::String("explicit rejection".to_string())));

    assert_eq!(engine.globals.get("chainResult"), Some(&JsValue::Number(75.0)));
    assert_eq!(engine.globals.get("handledErr"), Some(&JsValue::String("fatal error".to_string())));
}


