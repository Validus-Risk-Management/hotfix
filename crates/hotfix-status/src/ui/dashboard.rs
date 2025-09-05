use askama::Template;
use axum::response::{Html, IntoResponse};

#[derive(Template)]
#[template(path = "dashboard.askama")]
struct DashboardTemplate<'a> {
    title: &'a str,
}

pub(crate) async fn dashboard_handler() -> impl IntoResponse {
    let template = DashboardTemplate { title: "Dashboard" };

    Html(template.render().unwrap())
}
