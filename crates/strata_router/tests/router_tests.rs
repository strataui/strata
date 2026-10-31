use strata_router::prelude::*;
use strata_dom::*;
use strata_macro::rsx;

#[test]
fn test_router_url_matching_and_params() {
    let m = match_path("/teams/:team_id/projects/:project_id", "/teams/core/projects/99").unwrap();
    assert_eq!(m.params.get("team_id").unwrap(), "core");
    assert_eq!(m.params.get("project_id").unwrap(), "99");

    let wildcard = match_path("/static/*path", "/static/css/theme/dark.css").unwrap();
    assert_eq!(wildcard.params.get("path").unwrap(), "css/theme/dark.css");

    assert!(match_path("/admin", "/user").is_none());
}

#[test]
fn test_router_query_and_hash_parsing() {
    let loc = Location::parse("/analytics?filter=active&interval=1h#cpu");
    assert_eq!(loc.pathname, "/analytics");
    assert_eq!(loc.query.get("filter").unwrap(), "active");
    assert_eq!(loc.query.get("interval").unwrap(), "1h");
    assert_eq!(loc.hash, "#cpu");
}

#[test]
fn test_router_history_and_navigator() {
    let _router = init_router("/home");
    let loc = use_location();
    let nav = use_navigate();

    assert_eq!(loc.get().pathname, "/home");

    nav.push("/reports");
    assert_eq!(loc.get().pathname, "/reports");

    nav.push("/reports/daily");
    assert_eq!(loc.get().pathname, "/reports/daily");

    assert!(nav.back());
    assert_eq!(loc.get().pathname, "/reports");

    assert!(nav.forward());
    assert_eq!(loc.get().pathname, "/reports/daily");

    nav.replace("/reports/weekly");
    assert_eq!(loc.get().pathname, "/reports/weekly");
}

#[test]
fn test_render_routes_selection() {
    let _router = init_router("/about");

    let routes = vec![
        RouteEntry::new("/home", || rsx! { <div>{"Home Page"}</div> }),
        RouteEntry::new("/about", || rsx! { <div>{"About Us"}</div> }),
        RouteEntry::new("/users/:id", || rsx! { <div>{"User Profile"}</div> }),
    ];

    let view = render_routes(&routes);
    match view.root {
        NodeBlueprint::Element { tag, .. } => assert_eq!(tag, Tag::Div),
        _ => panic!("Expected Element node"),
    }
}
