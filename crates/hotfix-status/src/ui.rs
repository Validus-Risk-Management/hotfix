use askama::Template;
use axum::Router;
use axum::response::{Html, IntoResponse};
use axum::routing::get;

#[derive(Template)]
#[template(path = "dashboard.askama")]
struct DashboardTemplate<'a> {
    title: &'a str,
}

async fn dashboard_handler() -> impl IntoResponse {
    let template = DashboardTemplate { title: "Dashboard" };

    Html(template.render().unwrap())
}

pub fn builder_ui_router() -> Router {
    Router::new().route("/dashboard", get(dashboard_handler))
}
