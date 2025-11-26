use crate::DashboardDataProvider;
use crate::error::DashboardResult;
use askama::Template;
use axum::extract::{FromRef, State};
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

pub(crate) async fn dashboard_handler<S, P>(
    State(data_provider): State<P>,
) -> DashboardResult<impl IntoResponse>
where
    S: Clone + Send + Sync + 'static,
    P: DashboardDataProvider + FromRef<S>,
{
    let session_info = data_provider.get_session_info().await?;
    let timestamp_string = Utc::now().to_rfc3339();

    let template = DashboardTemplate {
        title: "Dashboard",
        session_info,
        timestamp_string: &timestamp_string,
    };

    template.render().map(Html).map_err(Into::into)
}
