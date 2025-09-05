use crate::AppState;
use crate::data_provider::DataProvider;
use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use hotfix::session::SessionInfo;

#[derive(Template)]
#[template(path = "dashboard.askama")]
struct DashboardTemplate<'a> {
    title: &'a str,
    session_info: SessionInfo,
}

pub(crate) async fn dashboard_handler<P: DataProvider>(
    State(state): State<AppState<P>>,
) -> impl IntoResponse {
    let session_info = state.data_provider.get_session_info().await;

    let template = DashboardTemplate {
        title: "Dashboard",
        session_info,
    };
    Html(template.render().unwrap())
}
