//! `Request::Once`: typing, keys or an approval answer that a resend must not repeat.
//! Its outcome goes in the same table as named edits' (`store/requests.rs`). Unlike an
//! edit it changes nothing stored, so the record follows the act: a host killed between
//! the two would type a resend again.
use super::*;
use mooring_core::protocol::RequestId;

/// One named act at a time, so a resend arriving while the first still runs waits for
/// its outcome instead of acting beside it.
static TURN: Mutex<()> = Mutex::new(());

pub(super) fn once(shared: &Shared, request: RequestId, inner: Request) -> io::Result<Reply> {
    if !matches!(
        inner,
        Request::SendText { .. } | Request::SendKeys { .. } | Request::AnswerApproval { .. }
    ) {
        return Err(io::Error::other(
            "只有打字、按键和回答审批能这样带请求 id；编辑用 Edit 自己的 request",
        ));
    }
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(first) = lock(shared)?.store.settled(&request)? {
        return first.map(|()| Reply::Ok).map_err(io::Error::other);
    }
    let result = dispatch(shared, inner);
    let mut host = lock(shared)?;
    match &result {
        Ok(_) => host.store.applied(&request)?,
        Err(e) => host.store.refused(&request, &e.to_string())?,
    }
    result
}
