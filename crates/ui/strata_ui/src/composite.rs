//! High-Level Composite Application Organisms for Strata UI.

#![allow(non_snake_case)]

use std::rc::Rc;
use strata_dom::{NodeBlueprint, View};
use strata_icons::prelude::{Icon, IconName};
use strata_macro::{component, rsx};

/// Form field container bundling labels, required indicators, and error validation states.
#[component]
pub fn FormField(
    label: Option<&'static str>,
    required: Option<bool>,
    helper_text: Option<&'static str>,
    error_message: Option<&'static str>,
    children: Option<Vec<NodeBlueprint>>,
    class: Option<&'static str>,
) -> View {
    let req = required.unwrap_or(false);
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("flex flex-col gap-1.5 w-full {}", custom)}>
            {if let Some(lbl) = label {
                rsx! {
                    <label class="text-xs font-mono uppercase tracking-wider text-neutral-400 select-none flex items-center gap-1">
                        <span>{lbl}</span>
                        {if req {
                            rsx! { <span class="text-rose-400 font-bold">{"*"}</span> }
                        } else {
                            rsx! { <></> }
                        }}
                    </label>
                }
            } else {
                rsx! { <></> }
            }}
            {children.unwrap_or_default()}
            {if let Some(err) = error_message {
                rsx! { <span class="text-xs text-rose-400 font-medium">{err}</span> }
            } else if let Some(help) = helper_text {
                rsx! { <span class="text-xs text-neutral-500">{help}</span> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Drag-and-drop file upload target zone.
#[component]
pub fn MediaDropzone(
    title: Option<&'static str>,
    description: Option<&'static str>,
    accept: Option<&'static str>,
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");
    let t = title.unwrap_or("Upload files");
    let d = description.unwrap_or("Drag and drop files here, or click to browse");

    rsx! {
        <div class={format!("flex flex-col items-center justify-center p-8 rounded-2xl border-2 border-dashed border-neutral-800 hover:border-indigo-500/60 bg-neutral-900/30 hover:bg-neutral-900/60 transition-all duration-200 cursor-pointer text-center group {}", custom)}>
            <div class="p-3.5 rounded-2xl bg-neutral-800/80 group-hover:bg-indigo-600/20 text-neutral-400 group-hover:text-indigo-400 transition-colors mb-3">
                {Icon(IconName::Upload, Some("w-6 h-6 text-current")).root}
            </div>
            <h4 class="text-sm font-semibold text-white group-hover:text-indigo-300 transition-colors">{t}</h4>
            <p class="text-xs text-neutral-400 mt-1 max-w-xs">{d}</p>
            {if let Some(acc) = accept {
                rsx! {
                    <span class="mt-3 px-2 py-0.5 rounded-full border border-neutral-800 bg-neutral-900 text-[10px] font-mono text-neutral-500">
                        {acc}
                    </span>
                }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}

/// Column definition for DataTable composite.
#[derive(Clone)]
pub struct TableColumnDef<T> {
    pub key: &'static str,
    pub title: &'static str,
    pub accessor: Rc<dyn Fn(&T) -> String>,
}

impl<T> TableColumnDef<T> {
    pub fn new(key: &'static str, title: &'static str, accessor: impl Fn(&T) -> String + 'static) -> Self {
        Self {
            key,
            title,
            accessor: Rc::new(accessor),
        }
    }
}

/// Reusable generic DataTable component.
pub fn DataTable<T: 'static>(
    columns: Vec<TableColumnDef<T>>,
    data: &[T],
    class: Option<&'static str>,
) -> View {
    let custom = class.unwrap_or("");

    rsx! {
        <div class="w-full overflow-x-auto rounded-xl border border-neutral-800">
            <table class={format!("w-full text-left text-sm text-neutral-300 {}", custom)}>
                <thead class="bg-neutral-900/80 border-b border-neutral-800 text-xs font-mono uppercase tracking-wider text-neutral-400">
                    <tr>
                        {columns.iter().map(|col| rsx! {
                            <th class="px-4 py-3 font-semibold text-neutral-300">
                                {col.title}
                            </th>
                        })}
                    </tr>
                </thead>
                <tbody class="divide-y divide-neutral-800/60 bg-neutral-950/40">
                    {data.iter().map(|row_item| rsx! {
                        <tr class="hover:bg-neutral-800/30 transition-colors">
                            {columns.iter().map(|col| {
                                let cell_val = (col.accessor)(row_item);
                                rsx! {
                                    <td class="px-4 py-3 text-neutral-300">
                                        {cell_val}
                                    </td>
                                }
                            })}
                        </tr>
                    })}
                </tbody>
            </table>
        </div>
    }
}
