use crate::message::logout::Logout;
use crate::message::reject::Reject;
use crate::message::verification::verify_message;
use crate::message::verification_error::{CompIdType, MessageVerificationError};
use crate::session::ctx::{SessionCtx, TransitionResult};
use crate::session::outbound;
use crate::session::state::SessionState;
use crate::transport::writer::WriterRef;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{MSG_SEQ_NUM, SessionRejectReason};
use hotfix_store::MessageStore;
use tracing::error;
use tracing::warn;

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

pub(crate) async fn handle_incorrect_begin_string<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    received_begin_string: String,
) -> TransitionResult {
    let logout = Logout::with_reason(format!(
        "beginString={received_begin_string} is not supported"
    ));
    match ctx.prepare_message(logout).await {
        Ok(prepared) => writer.send_raw_message(prepared.raw).await,
        Err(err) => warn!("failed to send logout for incorrect begin string: {err}"),
    }
    writer.disconnect().await;
    TransitionResult::TransitionTo(SessionState::new_disconnected(
        true,
        "incorrect begin string",
    ))
}

pub(crate) async fn handle_incorrect_comp_id<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    received_comp_id: String,
    comp_id_type: CompIdType,
    msg_seq_num: u64,
) -> TransitionResult {
    error!("rejecting message with incorrect comp ID: {received_comp_id} (type: {comp_id_type:?})");
    let reject = Reject::new(msg_seq_num)
        .session_reject_reason(SessionRejectReason::ValueIsIncorrect)
        .text(&format!("invalid comp ID {received_comp_id}"));
    if let Err(err) = outbound::send_message(ctx, writer, reject).await {
        error!("failed to send reject message with invalid comp ID: {err}");
    }
    let logout = Logout::with_reason("incorrect comp ID received".to_string());
    match ctx.prepare_message(logout).await {
        Ok(prepared) => writer.send_raw_message(prepared.raw).await,
        Err(err) => warn!("failed to send logout for incorrect comp ID: {err}"),
    }
    writer.disconnect().await;
    TransitionResult::TransitionTo(SessionState::new_disconnected(true, "incorrect comp ID"))
}

pub(crate) async fn handle_sequence_number_too_low<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    expected: u64,
    actual: u64,
    possible_duplicate: bool,
) -> TransitionResult {
    if possible_duplicate {
        warn!(
            "sequence number too low (expected {expected}, actual {actual}, but counterparty indicated it's poss duplicate, ignoring"
        );
        return TransitionResult::Stay;
    }
    error!(
        "we expected {expected} sequence number, but target sent lower ({actual}), terminating..."
    );
    let reason = format!("sequence number too low (actual {actual}, expected {expected})");
    let logout = Logout::with_reason(reason.clone());
    match ctx.prepare_message(logout).await {
        Ok(prepared) => writer.send_raw_message(prepared.raw).await,
        Err(err) => warn!("failed to send logout for sequence number too low: {err}"),
    }
    writer.disconnect().await;
    TransitionResult::TransitionTo(SessionState::new_disconnected(false, &reason))
}

pub(crate) async fn handle_invalid_msg_type<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    message: &Message,
    msg_type: &str,
) {
    match message.header().get(MSG_SEQ_NUM) {
        Ok(msg_seq_num) => {
            let reject = Reject::new(msg_seq_num)
                .session_reject_reason(SessionRejectReason::InvalidMsgtype)
                .text(&format!("invalid message type {msg_type}"));
            if let Err(err) = outbound::send_message(ctx, writer, reject).await {
                error!("failed to send reject message for invalid msgtype: {err}");
            }

            #[allow(clippy::collapsible_if)]
            if let Ok(seq_num) = message.header().get::<u64>(MSG_SEQ_NUM)
                && ctx.store.next_target_seq_number() == seq_num
            {
                if let Err(err) = ctx.store.increment_target_seq_number().await {
                    error!("failed to increment target seq number: {:?}", err);
                }
            }
        }
        Err(err) => {
            error!("failed to get message seq num: {:?}", err);
        }
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
