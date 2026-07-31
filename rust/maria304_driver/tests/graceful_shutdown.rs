#![allow(clippy::ignored_unit_patterns, clippy::duration_suboptimal_units)]
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn cancellation_token_stops_waiting_task() {
    let token = CancellationToken::new();
    let child = token.child_token();
    let handle = tokio::spawn(async move {
        tokio::select! {
            _ = child.cancelled() => "cancelled",
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => "timeout",
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_millis(200), handle)
        .await
        .expect("timeout")
        .unwrap();
    assert_eq!(result, "cancelled");
}
