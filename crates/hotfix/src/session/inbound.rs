use crate::message::reject::Reject;
use crate::message::verification::verify_message;
use crate::message::verification_error::MessageVerificationError;
use crate::session::ctx::SessionCtx;
use crate::session::outbound;
use crate::transport::writer::WriterRef;
use hotfix_message::message::Message;
use hotfix_message::session_fields::SessionRejectReason;
use hotfix_store::MessageStore;
use tracing::error;

pub(crate) fn verify_message_with_ctx<A, S: MessageStore>(
    ctx: &SessionCtx<A, S>,
    message: &Message,
    check_too_high: bool,
    check_too_low: bool,
) -> Result<(), MessageVerificationError> {
    let expected_seq_number = if check_too_high || check_too_low {
        Some(ctx.store.next_target_seq_number())
    } else {
        None
    };
    verify_message(
        message,
        &ctx.config,
        expected_seq_number,
        check_too_high,
        check_too_low,
    )
}

pub(crate) async fn handle_sending_time_accuracy_problem<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    msg_seq_num: u64,
    text: &str,
) {
    let reject = Reject::new(msg_seq_num)
        .session_reject_reason(SessionRejectReason::SendingtimeAccuracyProblem)
        .text(text);
    if let Err(err) = outbound::send_message(ctx, writer, reject).await {
        error!("failed to send reject for time accuracy problem: {err}");
    }
    if let Err(err) = ctx.store.increment_target_seq_number().await {
        error!("failed to increment target seq number: {:?}", err);
    }
}

pub(crate) async fn handle_original_sending_time_missing<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    msg_seq_num: u64,
) {
    let reject = Reject::new(msg_seq_num)
        .session_reject_reason(SessionRejectReason::RequiredTagMissing)
        .text("original sending time is required");
    if let Err(err) = outbound::send_message(ctx, writer, reject).await {
        error!("failed to send reject for time missing tag: {err}");
    }
    if let Err(err) = ctx.store.increment_target_seq_number().await {
        error!("failed to increment target seq number: {:?}", err);
    }
}
