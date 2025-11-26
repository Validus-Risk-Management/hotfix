use crate::AppState;
use crate::data_provider::DataProvider;
use crate::error::AppResult;
use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use chrono::Utc;
use hotfix::session::SessionInfo;
use hotfix::session::Status::{Active, Disconnected};

#[derive(Template)]
#[template(path = "dashboard.askama")]
struct DashboardTemplate<'a> {
    title: &'a str,
    session_info: SessionInfo,
    timestamp_string: &'a str,
}

pub(crate) async fn dashboard_handler<P: DataProvider>(
    State(state): State<AppState<P>>,
) -> AppResult<impl IntoResponse> {
    let session_info = state.data_provider.get_session_info().await?;
    let timestamp_string = Utc::now().to_rfc3339();

    let template = DashboardTemplate {
        title: "Dashboard",
        session_info,
        timestamp_string: &timestamp_string,
    };

    template.render().map(Html).map_err(Into::into)
}
