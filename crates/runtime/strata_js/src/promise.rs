//! Pure-Rust, zero-dependency cooperative microtask queue and Promise runtime.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use crate::JsValue;

thread_local! {
    static MICROTASK_QUEUE: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
}

/// Queues a closure to run as a microtask on the thread-local microtask queue.
pub fn queue_microtask<F: FnOnce() + 'static>(task: F) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push_back(Box::new(task));
    });
}

/// Drains and executes all pending microtasks in the queue until empty.
/// Returns the number of executed microtasks.
pub fn drain_microtasks() -> usize {
    let mut count = 0;
    loop {
        let task = MICROTASK_QUEUE.with(|q| q.borrow_mut().pop_front());
        match task {
            Some(t) => {
                t();
                count += 1;
            }
            None => break,
        }
    }
    count
}

/// Returns the number of microtasks currently waiting in the queue.
pub fn pending_microtasks_count() -> usize {
    MICROTASK_QUEUE.with(|q| q.borrow().len())
}

/// Represents the settlement state of a `JsPromise<T>`.
#[derive(Debug, Clone, PartialEq)]
pub enum PromiseState<T> {
    Pending,
    Fulfilled(T),
    Rejected(JsValue),
}

pub type ResolveFn<T> = Rc<dyn Fn(T)>;
pub type RejectFn = Rc<dyn Fn(JsValue)>;

struct PromiseInner<T> {
    state: PromiseState<T>,
    fulfill_handlers: Vec<Box<dyn FnOnce(T)>>,
    reject_handlers: Vec<Box<dyn FnOnce(JsValue)>>,
    wakers: Vec<Waker>,
}

/// Dynamic, cooperative JavaScript-style Promise with Rust `Future` implementation.
pub struct JsPromise<T> {
    inner: Rc<RefCell<PromiseInner<T>>>,
}

impl<T> Clone for JsPromise<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> JsPromise<T> {
    /// Creates a new pending promise along with resolve and reject controller callbacks.
    pub fn new_pending() -> (Self, ResolveFn<T>, RejectFn) {
        let inner = Rc::new(RefCell::new(PromiseInner {
            state: PromiseState::Pending,
            fulfill_handlers: Vec::new(),
            reject_handlers: Vec::new(),
            wakers: Vec::new(),
        }));

        let inner_resolve = Rc::clone(&inner);
        let resolve: ResolveFn<T> = Rc::new(move |val: T| {
            let mut inner = inner_resolve.borrow_mut();
            if let PromiseState::Pending = inner.state {
                inner.state = PromiseState::Fulfilled(val.clone());
                let handlers = std::mem::take(&mut inner.fulfill_handlers);
                let wakers = std::mem::take(&mut inner.wakers);
                drop(inner);

                for handler in handlers {
                    let v = val.clone();
                    queue_microtask(move || handler(v));
                }
                for waker in wakers {
                    waker.wake();
                }
            }
        });

        let inner_reject = Rc::clone(&inner);
        let reject: RejectFn = Rc::new(move |err: JsValue| {
            let mut inner = inner_reject.borrow_mut();
            if let PromiseState::Pending = inner.state {
                inner.state = PromiseState::Rejected(err.clone());
                let handlers = std::mem::take(&mut inner.reject_handlers);
                let wakers = std::mem::take(&mut inner.wakers);
                drop(inner);

                for handler in handlers {
                    let e = err.clone();
                    queue_microtask(move || handler(e));
                }
                for waker in wakers {
                    waker.wake();
                }
            }
        });

        (Self { inner }, resolve, reject)
    }

    /// Constructs a promise by executing an executor closure receiving resolve and reject callbacks.
    pub fn new<F>(executor: F) -> Self
    where
        F: FnOnce(ResolveFn<T>, RejectFn),
    {
        let (promise, resolve, reject) = Self::new_pending();
        executor(resolve, reject);
        promise
    }

    /// Creates an immediately resolved promise.
    pub fn resolve(val: T) -> Self {
        let (promise, resolve, _reject) = Self::new_pending();
        resolve(val);
        promise
    }

    /// Creates an immediately rejected promise.
    pub fn reject(err: impl Into<JsValue>) -> Self {
        let (promise, _resolve, reject) = Self::new_pending();
        reject(err.into());
        promise
    }

    /// Returns the current state of the promise.
    pub fn state(&self) -> PromiseState<T> {
        self.inner.borrow().state.clone()
    }

    /// Attaches a fulfillment callback transforming value of type `T` into `U`.
    pub fn then<U: Clone + 'static, F: FnOnce(T) -> U + 'static>(self, on_fulfilled: F) -> JsPromise<U> {
        let (next_promise, resolve, reject) = JsPromise::<U>::new_pending();
        let mut inner = self.inner.borrow_mut();

        match &inner.state {
            PromiseState::Fulfilled(val) => {
                let v = val.clone();
                queue_microtask(move || {
                    let res = on_fulfilled(v);
                    resolve(res);
                });
            }
            PromiseState::Rejected(err) => {
                let e = err.clone();
                queue_microtask(move || {
                    reject(e);
                });
            }
            PromiseState::Pending => {
                inner.fulfill_handlers.push(Box::new(move |val| {
                    let res = on_fulfilled(val);
                    resolve(res);
                }));
                inner.reject_handlers.push(Box::new(move |err| {
                    reject(err);
                }));
            }
        }

        next_promise
    }

    /// Attaches an async fulfillment callback returning another `JsPromise<U>`.
    pub fn then_async<U: Clone + 'static, F: FnOnce(T) -> JsPromise<U> + 'static>(
        self,
        on_fulfilled: F,
    ) -> JsPromise<U> {
        let (next_promise, resolve, reject) = JsPromise::<U>::new_pending();
        let mut inner = self.inner.borrow_mut();

        let chain_step = {
            let resolve = Rc::clone(&resolve);
            let reject = Rc::clone(&reject);
            move |val: T| {
                let returned_promise = on_fulfilled(val);
                let resolve_clone = Rc::clone(&resolve);
                let reject_clone = Rc::clone(&reject);

                returned_promise
                    .then(move |u| {
                        resolve_clone(u);
                    })
                    .catch(move |err| {
                        reject_clone(err);
                    });
            }
        };

        match &inner.state {
            PromiseState::Fulfilled(val) => {
                let v = val.clone();
                queue_microtask(move || chain_step(v));
            }
            PromiseState::Rejected(err) => {
                let e = err.clone();
                let reject_clone = Rc::clone(&reject);
                queue_microtask(move || reject_clone(e));
            }
            PromiseState::Pending => {
                inner.fulfill_handlers.push(Box::new(chain_step));
                let reject_clone = Rc::clone(&reject);
                inner.reject_handlers.push(Box::new(move |err| reject_clone(err)));
            }
        }

        next_promise
    }

    /// Attaches a rejection handler to the promise.
    pub fn catch<F: FnOnce(JsValue) -> T + 'static>(self, on_rejected: F) -> JsPromise<T> {
        let (next_promise, resolve, _reject) = JsPromise::<T>::new_pending();
        let mut inner = self.inner.borrow_mut();

        match &inner.state {
            PromiseState::Fulfilled(val) => {
                let v = val.clone();
                let resolve_clone = Rc::clone(&resolve);
                queue_microtask(move || resolve_clone(v));
            }
            PromiseState::Rejected(err) => {
                let e = err.clone();
                let resolve_clone = Rc::clone(&resolve);
                queue_microtask(move || {
                    let recovered = on_rejected(e);
                    resolve_clone(recovered);
                });
            }
            PromiseState::Pending => {
                let resolve_fulfill = Rc::clone(&resolve);
                let resolve_reject = Rc::clone(&resolve);
                inner.fulfill_handlers.push(Box::new(move |val| resolve_fulfill(val)));
                inner.reject_handlers.push(Box::new(move |err| {
                    let recovered = on_rejected(err);
                    resolve_reject(recovered);
                }));
            }
        }

        next_promise
    }

    /// Attaches a callback executed whenever the promise settles (fulfilled or rejected).
    pub fn finally<F: FnOnce() + 'static>(self, on_finally: F) -> JsPromise<T> {
        let (next_promise, resolve, reject) = JsPromise::<T>::new_pending();
        let on_finally = Rc::new(RefCell::new(Some(on_finally)));
        let on_finally_rej = Rc::clone(&on_finally);

        let mut inner = self.inner.borrow_mut();

        match &inner.state {
            PromiseState::Fulfilled(val) => {
                let v = val.clone();
                queue_microtask(move || {
                    if let Some(cb) = on_finally.borrow_mut().take() {
                        cb();
                    }
                    resolve(v);
                });
            }
            PromiseState::Rejected(err) => {
                let e = err.clone();
                queue_microtask(move || {
                    if let Some(cb) = on_finally_rej.borrow_mut().take() {
                        cb();
                    }
                    reject(e);
                });
            }
            PromiseState::Pending => {
                inner.fulfill_handlers.push(Box::new(move |val| {
                    if let Some(cb) = on_finally.borrow_mut().take() {
                        cb();
                    }
                    resolve(val);
                }));
                inner.reject_handlers.push(Box::new(move |err| {
                    if let Some(cb) = on_finally_rej.borrow_mut().take() {
                        cb();
                    }
                    reject(err);
                }));
            }
        }

        next_promise
    }

    /// Promise.all: Resolves when all promises resolve, or rejects when any rejects.
    pub fn all(promises: Vec<JsPromise<T>>) -> JsPromise<Vec<T>> {
        if promises.is_empty() {
            return JsPromise::resolve(Vec::new());
        }

        let total = promises.len();
        let (all_promise, resolve, reject) = JsPromise::<Vec<T>>::new_pending();
        let results = Rc::new(RefCell::new(vec![None; total]));
        let completed = Rc::new(RefCell::new(0usize));
        let rejected = Rc::new(RefCell::new(false));

        for (idx, p) in promises.into_iter().enumerate() {
            let results_clone = Rc::clone(&results);
            let completed_clone = Rc::clone(&completed);
            let rejected_clone_then = Rc::clone(&rejected);
            let rejected_clone_catch = Rc::clone(&rejected);
            let resolve_clone = Rc::clone(&resolve);
            let reject_clone = Rc::clone(&reject);

            p.then(move |val| {
                if *rejected_clone_then.borrow() {
                    return;
                }
                results_clone.borrow_mut()[idx] = Some(val);
                let mut c = completed_clone.borrow_mut();
                *c += 1;
                if *c == total {
                    let final_vals: Vec<T> = results_clone
                        .borrow_mut()
                        .drain(..)
                        .map(|opt| opt.unwrap())
                        .collect();
                    resolve_clone(final_vals);
                }
            })
            .catch(move |err| {
                let mut rej = rejected_clone_catch.borrow_mut();
                if !*rej {
                    *rej = true;
                    reject_clone(err);
                }
            });
        }

        all_promise
    }

    /// Promise.race: Settles as soon as the first promise settles.
    pub fn race(promises: Vec<JsPromise<T>>) -> JsPromise<T> {
        let (race_promise, resolve, reject) = JsPromise::<T>::new_pending();
        let settled = Rc::new(RefCell::new(false));

        for p in promises {
            let settled_res = Rc::clone(&settled);
            let settled_rej = Rc::clone(&settled);
            let resolve_clone = Rc::clone(&resolve);
            let reject_clone = Rc::clone(&reject);

            p.then(move |val| {
                let mut s = settled_res.borrow_mut();
                if !*s {
                    *s = true;
                    resolve_clone(val);
                }
            })
            .catch(move |err| {
                let mut s = settled_rej.borrow_mut();
                if !*s {
                    *s = true;
                    reject_clone(err);
                }
            });
        }

        race_promise
    }
}

impl<T: Clone + 'static> Future for JsPromise<T> {
    type Output = Result<T, JsValue>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = self.inner.borrow_mut();
        match &inner.state {
            PromiseState::Fulfilled(val) => Poll::Ready(Ok(val.clone())),
            PromiseState::Rejected(err) => Poll::Ready(Err(err.clone())),
            PromiseState::Pending => {
                inner.wakers.push(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
