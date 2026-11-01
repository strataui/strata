//! Micro JavaScript Reactive Runtime & DOM Binding Helpers (~1.5KB pure JS).
//!
//! Provides ultra-lightweight zero-dependency reactive primitives (`createSignal`, `createEffect`,
//! `createMemo`, `batch`) and DOM binding utilities in pure ES Modules.

pub struct JsRuntimeEmitter;

impl JsRuntimeEmitter {
    /// Returns the core JavaScript reactive runtime source code.
    pub const fn reactive_runtime() -> &'static str {
        r#"// Strata Micro Reactive Runtime (SolidJS-grade, Zero Virtual DOM)
let activeEffect = null;
let batchDepth = 0;
const pendingEffects = new Set();

export function createSignal(initialValue) {
  let value = initialValue;
  const subscribers = new Set();
  const read = () => {
    if (activeEffect) subscribers.add(activeEffect);
    return value;
  };
  const write = (newValue) => {
    value = typeof newValue === 'function' ? newValue(value) : newValue;
    for (const sub of subscribers) {
      if (batchDepth > 0) pendingEffects.add(sub);
      else sub();
    }
  };
  return [read, write];
}

export function createEffect(fn) {
  const effect = () => {
    const prevEffect = activeEffect;
    activeEffect = effect;
    try {
      fn();
    } finally {
      activeEffect = prevEffect;
    }
  };
  effect();
  return () => {
    // Teardown / dispose hook
  };
}

export function createMemo(fn) {
  const [memo, setMemo] = createSignal(fn());
  createEffect(() => setMemo(fn()));
  return memo;
}

export function batch(fn) {
  batchDepth++;
  try {
    return fn();
  } finally {
    batchDepth--;
    if (batchDepth === 0) {
      const effects = Array.from(pendingEffects);
      pendingEffects.clear();
      for (const eff of effects) eff();
    }
  }
}

export function untrack(fn) {
  const prevEffect = activeEffect;
  activeEffect = null;
  try {
    return fn();
  } finally {
    activeEffect = prevEffect;
  }
}
"#
    }

    /// Returns the DOM creation and reactive binding helper functions.
    pub const fn dom_helpers() -> &'static str {
        r#"// Strata DOM Helper & Reactive Binding Suite
export function el(tag, classes = '', attrs = {}, children = []) {
  const element = document.createElement(tag);
  if (classes) element.className = classes;
  for (const [key, val] of Object.entries(attrs)) {
    if (key.startsWith('on') && typeof val === 'function') {
      const evtName = key.slice(2).toLowerCase();
      element.addEventListener(evtName, val);
    } else if (val !== undefined && val !== null) {
      element.setAttribute(key, String(val));
    }
  }
  for (const child of children) {
    if (typeof child === 'string') {
      element.appendChild(document.createTextNode(child));
    } else if (child instanceof Node) {
      element.appendChild(child);
    } else if (typeof child === 'function') {
      // Dynamic reactive expression
      const textNode = document.createTextNode('');
      bindText(textNode, child);
      element.appendChild(textNode);
    }
  }
  return element;
}

export function txt(content) {
  return document.createTextNode(content);
}

export function frag(children = []) {
  const fragment = document.createDocumentFragment();
  for (const child of children) {
    if (typeof child === 'string') {
      fragment.appendChild(document.createTextNode(child));
    } else if (child instanceof Node) {
      fragment.appendChild(child);
    }
  }
  return fragment;
}

export function on(element, eventName, handler) {
  element.addEventListener(eventName, handler);
}

export function bindText(textNode, getter) {
  createEffect(() => {
    const val = getter();
    textNode.nodeValue = val !== undefined && val !== null ? String(val) : '';
  });
}

export function bindAttr(element, attrName, getter) {
  createEffect(() => {
    const val = getter();
    if (val === false || val === null || val === undefined) {
      element.removeAttribute(attrName);
    } else {
      element.setAttribute(attrName, val === true ? '' : String(val));
    }
  });
}

export function bindClass(element, className, getter) {
  createEffect(() => {
    element.classList.toggle(className, Boolean(getter()));
  });
}

export function mount(rootElement, componentFn) {
  rootElement.innerHTML = '';
  const node = componentFn();
  if (node) rootElement.appendChild(node);
}
"#
    }

    /// Emits the complete standalone JavaScript runtime (ESM or IIFE).
    pub fn emit_full_runtime(as_esm: bool) -> String {
        if as_esm {
            format!("{}\n\n{}", Self::reactive_runtime(), Self::dom_helpers())
        } else {
            // Emits an IIFE attached to window.Strata
            let esm_code = format!("{}\n\n{}", Self::reactive_runtime(), Self::dom_helpers());
            let converted = esm_code.replace("export function ", "function ");
            format!(
                r#"(function (global) {{
  'use strict';
  {}

  global.Strata = {{
    createSignal, createEffect, createMemo, batch, untrack,
    el, txt, frag, on, bindText, bindAttr, bindClass, mount
  }};
}})(typeof window !== 'undefined' ? window : this);
"#,
                converted
            )
        }
    }
}
