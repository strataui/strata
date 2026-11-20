use std::cell::Cell;
use std::rc::Rc;
use strata::prelude::*;
use strata::NodeQuery;
use strata_dom::View;

#[component]
fn FindClickApp(
    page_clicks: Rc<Cell<u32>>,
    modal_clicks: Rc<Cell<u32>>,
    icon_clicks: Rc<Cell<u32>>,
    with_modal: bool,
) -> View {
    let on_page = move |_ev: strata_dom::Event| page_clicks.set(page_clicks.get() + 1);
    let on_modal = move |_ev: strata_dom::Event| modal_clicks.set(modal_clicks.get() + 1);
    let on_icon = move |_ev: strata_dom::Event| icon_clicks.set(icon_clicks.get() + 1);

    let modal = if with_modal {
        rsx! {
            <div class="absolute left-0 top-0 w-[400px] h-[300px] flex flex-col">
                <button class="w-[400px] h-[40px]" onclick=on_modal>
                    "Confirm"
                </button>
            </div>
        }
    } else {
        rsx! { <div></div> }
    };

    rsx! {
        <div class="relative flex flex-col w-[400px] h-[300px]">
            <button class="w-[400px] h-[40px]" onclick=on_page>
                "Confirm"
            </button>
            <button class="w-[400px] h-[40px]" title="Settings" onclick=on_icon>
                <span>"Open  "</span>
                <span>"Gear"</span>
            </button>
            {modal}
        </div>
    }
}

fn runtime(with_modal: bool) -> (AppRuntime, [Rc<Cell<u32>>; 3]) {
    let counters = [
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    ];
    let rt = AppRuntime::new(
        FindClickApp(
            counters[0].clone(),
            counters[1].clone(),
            counters[2].clone(),
            with_modal,
        ),
        400.0,
        300.0,
        1.0,
    );
    (rt, counters)
}

#[test]
fn find_by_text_returns_deepest_match_and_normalizes_whitespace() {
    let (rt, _) = runtime(false);
    assert_eq!(rt.find_by_text("Gear"), Some(vec![1, 1]));
    // Exact mode compares the aggregated, whitespace-normalized content of the button.
    assert_eq!(rt.find_by_text("Open Gear"), Some(vec![1]));
    assert_eq!(rt.node_text(&[1]).as_deref(), Some("Open Gear"));
    // Substring mode reports only the deepest element.
    assert_eq!(
        rt.find(&NodeQuery::text("Ope").contains()),
        Some(vec![1, 0])
    );
    assert_eq!(rt.find_by_text("nonexistent"), None);
}

#[test]
fn find_by_title_and_click() {
    let (rt, [page, _, icon]) = runtime(false);
    assert_eq!(rt.find_by_title("Settings"), Some(vec![1]));
    assert!(rt.click_title("Settings"));
    assert_eq!((page.get(), icon.get()), (0, 1));
    assert!(!rt.click_title("Missing"));
    assert!(!rt.click_text("Missing"));
}

#[test]
fn first_and_last_match_pick_page_or_modal_button() {
    let (rt, [page, modal, _]) = runtime(true);
    let all = rt.find_all(&NodeQuery::text("Confirm"));
    assert_eq!(all.len(), 2);
    assert_eq!(all[0], vec![0]);
    assert_eq!(rt.find_last_by_text("Confirm"), Some(all[1].clone()));

    // Last match wins: the modal button.
    assert!(rt.click_last_text("Confirm"));
    assert_eq!((page.get(), modal.get()), (0, 1));

    // First match is occluded by the modal, so a real pointer click lands on the modal too.
    assert!(rt.click_text("Confirm"));
    assert_eq!((page.get(), modal.get()), (0, 2));
}

#[test]
fn single_match_without_modal() {
    let (rt, _) = runtime(false);
    assert_eq!(rt.find_all(&NodeQuery::text("Confirm")).len(), 1);
}
