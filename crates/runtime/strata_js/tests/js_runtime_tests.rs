//! Comprehensive Integration Tests for `strata_js` Runtime, dynamic values, promises, and Web APIs.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::Duration;

use strata_js::prelude::*;

// Helper to create a dummy Waker for polling futures synchronously without external dependencies
fn dummy_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

#[test]
fn test_json_parsing_and_serialization() {
    let json_text = r#"
    {
        "title": "Strata WebEngine",
        "version": 1.0,
        "is_fast": true,
        "tags": ["zero-dep", "rust", "wasm"],
        "metadata": {
            "author": "Larvance",
            "active": null
        },
    }
    "#;

    let parsed = parse_json(json_text).expect("Failed to parse JSON");
    assert_eq!(parsed["title"].as_str(), Some("Strata WebEngine"));
    assert_eq!(parsed["version"].as_f64(), Some(1.0));
    assert_eq!(parsed["is_fast"].as_bool(), Some(true));
    assert_eq!(parsed["tags"].len(), 3);
    assert_eq!(parsed["tags"][0].as_str(), Some("zero-dep"));
    assert_eq!(parsed["metadata"]["author"].as_str(), Some("Larvance"));
    assert!(parsed["metadata"]["active"].is_null());

    // Roundtrip test
    let serialized = stringify_json(&parsed);
    let roundtripped = parse_json(&serialized).expect("Failed to re-parse serialized JSON");
    assert_eq!(parsed, roundtripped);

    // Escape character testing
    let escaped_json = r#"{"msg": "Hello\nWorld\t\"Escaped\"\u0041"}"#;
    let escaped_val = parse_json(escaped_json).expect("Escape parse failed");
    assert_eq!(escaped_val["msg"].as_str(), Some("Hello\nWorld\t\"Escaped\"A"));
}

#[test]
fn test_js_typeof_and_truthiness() {
    assert_eq!(JsValue::Undefined.js_typeof(), "undefined");
    assert_eq!(JsValue::Null.js_typeof(), "object"); // Spec quirk
    assert_eq!(JsValue::Bool(true).js_typeof(), "boolean");
    assert_eq!(JsValue::Number(42.0).js_typeof(), "number");
    assert_eq!(JsValue::String("str".into()).js_typeof(), "string");
    assert_eq!(JsValue::Array(vec![]).js_typeof(), "object");
    assert_eq!(JsValue::Object(BTreeMap::new()).js_typeof(), "object");

    // Truthiness
    assert!(!JsValue::Undefined.is_truthy());
    assert!(!JsValue::Null.is_truthy());
    assert!(!JsValue::Bool(false).is_truthy());
    assert!(JsValue::Bool(true).is_truthy());
    assert!(!JsValue::Number(0.0).is_truthy());
    assert!(!JsValue::Number(f64::NAN).is_truthy());
    assert!(JsValue::Number(1.0).is_truthy());
    assert!(!JsValue::String("".into()).is_truthy());
    assert!(JsValue::String("not empty".into()).is_truthy());
    assert!(JsValue::Array(vec![]).is_truthy());
    assert!(JsValue::Object(BTreeMap::new()).is_truthy());
}

#[test]
fn test_strict_and_loose_equality() {
    let num_42 = JsValue::Number(42.0);
    let str_42 = JsValue::String("42".into());
    let nan_val = JsValue::Number(f64::NAN);

    // Strict equality (===)
    assert!(num_42.strict_eq(&JsValue::Number(42.0)));
    assert!(!num_42.strict_eq(&str_42));
    assert!(!nan_val.strict_eq(&nan_val)); // NaN !== NaN

    // Loose equality (==)
    assert!(num_42.loose_eq(&str_42)); // 42 == "42"
    assert!(JsValue::Null.loose_eq(&JsValue::Undefined)); // null == undefined
    assert!(JsValue::Undefined.loose_eq(&JsValue::Null));
    assert!(JsValue::Bool(true).loose_eq(&JsValue::Number(1.0))); // true == 1
    assert!(JsValue::Bool(false).loose_eq(&JsValue::Number(0.0))); // false == 0
    assert!(JsValue::Bool(false).loose_eq(&JsValue::String("0".into()))); // false == "0"

    // Array to string coercion in loose eq
    let arr = JsValue::Array(vec![JsValue::Number(1.0), JsValue::Number(2.0)]);
    assert!(arr.loose_eq(&JsValue::String("1,2".into())));
}

#[test]
fn test_array_prototype_methods() {
    let arr = JsValue::Array(vec![
        JsValue::Number(1.0),
        JsValue::Number(2.0),
        JsValue::Number(3.0),
        JsValue::Number(4.0),
        JsValue::Number(5.0),
    ]);

    // map: x * 2
    let doubled = arr.map(|x| JsValue::Number(x.as_f64().unwrap() * 2.0));
    assert_eq!(doubled[0], JsValue::Number(2.0));
    assert_eq!(doubled[4], JsValue::Number(10.0));

    // filter: x % 2 == 0
    let evens = arr.filter(|x| x.as_f64().unwrap() as i64 % 2 == 0);
    assert_eq!(evens.len(), 2);
    assert_eq!(evens[0], JsValue::Number(2.0));
    assert_eq!(evens[1], JsValue::Number(4.0));

    // reduce: sum
    let sum = arr.reduce(
        |acc, x| JsValue::Number(acc.as_f64().unwrap_or(0.0) + x.as_f64().unwrap_or(0.0)),
        JsValue::Number(0.0),
    );
    assert_eq!(sum.as_f64(), Some(15.0));

    // find & find_index
    assert_eq!(arr.find(|x| x.as_f64() == Some(3.0)), Some(&JsValue::Number(3.0)));
    assert_eq!(arr.find_index(|x| x.as_f64() == Some(4.0)), Some(3));

    // some & every
    assert!(arr.some(|x| x.as_f64() == Some(5.0)));
    assert!(!arr.some(|x| x.as_f64() == Some(99.0)));
    assert!(arr.every(|x| x.as_f64().unwrap() > 0.0));
    assert!(!arr.every(|x| x.as_f64().unwrap() > 3.0));

    // includes
    assert!(arr.includes(&JsValue::Number(2.0)));
    assert!(!arr.includes(&JsValue::Number(100.0)));

    // slice
    let sliced = arr.slice(1, Some(3));
    assert_eq!(sliced.len(), 2);
    assert_eq!(sliced[0], JsValue::Number(2.0));
    assert_eq!(sliced[1], JsValue::Number(3.0));

    let sliced_negative = arr.slice(-2, None);
    assert_eq!(sliced_negative.len(), 2);
    assert_eq!(sliced_negative[0], JsValue::Number(4.0));
    assert_eq!(sliced_negative[1], JsValue::Number(5.0));

    // join
    assert_eq!(arr.join("-"), "1-2-3-4-5");

    // push, pop, reverse, sort
    let mut mut_arr = arr.clone();
    mut_arr.push(6.0);
    assert_eq!(mut_arr.len(), 6);
    assert_eq!(mut_arr.pop(), Some(JsValue::Number(6.0)));
    assert_eq!(mut_arr.len(), 5);

    mut_arr.reverse();
    assert_eq!(mut_arr[0], JsValue::Number(5.0));

    mut_arr.sort_by(|a, b| a.as_f64().unwrap().partial_cmp(&b.as_f64().unwrap()).unwrap());
    assert_eq!(mut_arr[0], JsValue::Number(1.0));

    // flat
    let nested = JsValue::Array(vec![
        JsValue::Number(1.0),
        JsValue::Array(vec![JsValue::Number(2.0), JsValue::Array(vec![JsValue::Number(3.0)])]),
    ]);
    let flat_1 = nested.flat(1);
    assert_eq!(flat_1.len(), 3);
    assert_eq!(flat_1[0], JsValue::Number(1.0));
    assert_eq!(flat_1[1], JsValue::Number(2.0));
    assert!(matches!(flat_1[2], JsValue::Array(_)));

    let flat_2 = nested.flat(2);
    assert_eq!(flat_2.len(), 3);
    assert_eq!(flat_2[2], JsValue::Number(3.0));
}

#[test]
fn test_js_object_utilities() {
    let mut obj = JsValue::Object(BTreeMap::new());
    obj.set_property("a", 10.0);
    obj.set_property("b", "str");

    let keys = JsObject::keys(&obj);
    assert_eq!(keys, vec!["a", "b"]);

    let values = JsObject::values(&obj);
    assert_eq!(values.len(), 2);

    let entries = JsObject::entries(&obj);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "a");

    let mut target = JsValue::Object(BTreeMap::new());
    target.set_property("c", 30.0);
    JsObject::assign(&mut target, &obj);
    assert_eq!(target["a"], JsValue::Number(10.0));
    assert_eq!(target["c"], JsValue::Number(30.0));
}

#[test]
fn test_microtask_queue_and_promises() {
    let log = Rc::new(RefCell::new(Vec::new()));

    // Microtask scheduling
    let log_1 = Rc::clone(&log);
    queue_microtask(move || {
        log_1.borrow_mut().push("microtask_1".to_string());
    });

    let log_2 = Rc::clone(&log);
    queue_microtask(move || {
        log_2.borrow_mut().push("microtask_2".to_string());
    });

    assert_eq!(pending_microtasks_count(), 2);
    let drained = drain_microtasks();
    assert_eq!(drained, 2);
    assert_eq!(*log.borrow(), vec!["microtask_1", "microtask_2"]);

    // Promise .then chaining
    let promise_log = Rc::new(RefCell::new(Vec::new()));
    let p_log = Rc::clone(&promise_log);

    let promise = JsPromise::resolve(10)
        .then(|val| val * 2)
        .then(move |doubled| {
            p_log.borrow_mut().push(format!("result: {}", doubled));
            doubled
        });

    drain_microtasks();
    assert_eq!(*promise_log.borrow(), vec!["result: 20"]);
    assert_eq!(promise.state(), PromiseState::Fulfilled(20));

    // Promise .catch error recovery
    let recovered = JsPromise::<i32>::reject("network error")
        .catch(|_err| -1)
        .then(|val| val + 10);

    drain_microtasks();
    assert_eq!(recovered.state(), PromiseState::Fulfilled(9));

    // Promise.all
    let p1 = JsPromise::resolve(1);
    let p2 = JsPromise::resolve(2);
    let p3 = JsPromise::resolve(3);
    let all = JsPromise::all(vec![p1, p2, p3]);

    drain_microtasks();
    assert_eq!(all.state(), PromiseState::Fulfilled(vec![1, 2, 3]));

    // Promise.race
    let (slow_p, _slow_res, _slow_rej) = JsPromise::<&str>::new_pending();
    let fast_p = JsPromise::resolve("fast");
    let race = JsPromise::race(vec![slow_p, fast_p]);

    drain_microtasks();
    assert_eq!(race.state(), PromiseState::Fulfilled("fast"));

    // Future implementation: Poll test
    let mut fut = JsPromise::resolve("resolved_future");
    let waker = dummy_waker();
    let mut cx = Context::from_waker(&waker);
    let pinned = std::pin::Pin::new(&mut fut);
    let poll_res = std::future::Future::poll(pinned, &mut cx);
    assert_eq!(poll_res, Poll::Ready(Ok("resolved_future")));
}

#[test]
fn test_headers_and_response() {
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/json");
    headers.append("Accept", "text/html");
    headers.append("Accept", "application/xhtml+xml");

    // Case-insensitivity
    assert!(headers.has("content-type"));
    assert_eq!(headers.get("CONTENT-TYPE"), Some("application/json".to_string()));
    assert_eq!(
        headers.get("accept"),
        Some("text/html, application/xhtml+xml".to_string())
    );
    assert_eq!(headers.get_all("accept"), vec!["text/html", "application/xhtml+xml"]);

    headers.delete("content-type");
    assert!(!headers.has("content-type"));

    // Response creation
    let body = b"{\"success\": true}".to_vec();
    let res = Response::new(body, 200, headers);
    assert!(res.ok);
    assert_eq!(res.status, 200);
    assert_eq!(res.status_text, "OK");
}

#[test]
fn test_url_search_params() {
    let query = "q=strata+framework&category=frontend&tag=rust&tag=zero-dep";
    let mut params = URLSearchParams::parse(query);

    assert_eq!(params.get("q"), Some("strata framework"));
    assert_eq!(params.get("category"), Some("frontend"));
    assert_eq!(params.get_all("tag"), vec!["rust", "zero-dep"]);

    params.set("category", "compiler");
    assert_eq!(params.get("category"), Some("compiler"));

    params.append("tag", "fast");
    assert_eq!(params.get_all("tag"), vec!["rust", "zero-dep", "fast"]);

    params.delete("q");
    assert!(!params.has("q"));

    params.sort();
    let encoded = params.to_query_string();
    assert!(encoded.starts_with("category=compiler&tag="));
}

#[test]
fn test_timer_scheduler() {
    let triggered = Rc::new(RefCell::new(0usize));
    let t_clone = Rc::clone(&triggered);

    let _timer_id = set_timeout(
        move || {
            *t_clone.borrow_mut() += 1;
        },
        50,
    );

    let cancelled_marker = Rc::new(RefCell::new(false));
    let c_clone = Rc::clone(&cancelled_marker);
    let cancel_id = set_timeout(
        move || {
            *c_clone.borrow_mut() = true;
        },
        50,
    );
    clear_timeout(cancel_id);

    // Initial tick before delay expires
    tick_timers();
    assert_eq!(*triggered.borrow(), 0);

    // Advance simulated time past delay
    let fired = advance_timers_by(Duration::from_millis(60));
    assert_eq!(fired, 1);
    assert_eq!(*triggered.borrow(), 1);
    assert!(!*cancelled_marker.borrow()); // Cancelled timer did not run
}

#[test]
fn test_storage_api() {
    let storage = Storage::new();
    assert_eq!(storage.length(), 0);

    storage.set_item("theme", "dark");
    storage.set_item("token", "xyz_auth_token");
    assert_eq!(storage.length(), 2);
    assert_eq!(storage.get_item("theme"), Some("dark".to_string()));

    storage.remove_item("token");
    assert_eq!(storage.length(), 1);
    assert_eq!(storage.get_item("token"), None);

    storage.clear();
    assert_eq!(storage.length(), 0);
}
