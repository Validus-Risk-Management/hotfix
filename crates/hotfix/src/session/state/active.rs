use crate::session::state::TestRequestId;
use crate::transport::writer::WriterRef;
use tokio::time::Instant;

pub(crate) struct ActiveState {
    /// The writer's reference to send messages to the counterparty
    pub(crate) writer: WriterRef,
    /// When we should send the next heartbeat message to the counterparty
    pub(crate) heartbeat_deadline: Instant,
    /// When the next message from the counterparty is expected at the latest
    pub(crate) peer_deadline: Instant,
    /// The ID of the test request we sent on peer timer expiry
    pub(crate) sent_test_request_id: Option<TestRequestId>,
}

#[inline]
pub(crate) fn calculate_peer_interval(heartbeat_interval: u64) -> u64 {
    (heartbeat_interval as f64 * super::TEST_REQUEST_THRESHOLD).round() as u64
}
