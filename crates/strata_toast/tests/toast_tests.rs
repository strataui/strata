use strata_toast::prelude::*;

#[test]
fn test_toast_async_loading_and_resolution() {
    let manager = ToastManager::new();

    // Start loading state
    let id = manager.loading("Deploying smart contract to testnet...");
    let toasts = manager.toasts.get();
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0].kind, ToastKind::Loading);
    assert_eq!(toasts[0].duration_ms, 0);

    // Simulated task finishes: transition toast to success
    assert!(manager.update(id, ToastKind::Success, "Contract deployed successfully!"));
    let updated = manager.toasts.get();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].kind, ToastKind::Success);
    assert_eq!(updated[0].duration_ms, 4000);

    // Stack layout reflects 1 front toast
    let stack = manager.stacked_toasts();
    assert_eq!(stack.len(), 1);
    assert_eq!(stack[0].item.title, "Contract deployed successfully!");
}
