use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use strata_crdt::{
    CrdtCounterStore, CrdtSignalStore, LamportClock, LwwRegister, OrSet, PnCounter,
    ReplicatedSignal, RgaSequence, SyncMessage, VectorClock,
};
use strata_dom::reactive::{use_effect, use_memo};

#[test]
fn test_lamport_and_vector_clock() {
    let mut c1 = LamportClock::new(0, 1);
    let mut c2 = LamportClock::new(0, 2);

    assert_eq!(c1.tick(), LamportClock::new(1, 1));
    assert_eq!(c2.tick(), LamportClock::new(1, 2));

    // Causal witness
    c1.witness(c2);
    assert_eq!(c1.counter, 2);

    // Vector clock
    let mut vc1 = VectorClock::new();
    let mut vc2 = VectorClock::new();

    vc1.inc(1);
    vc1.inc(1);
    vc2.inc(2);

    assert!(!vc1.dominates(&vc2));
    vc1.merge(&vc2);
    assert!(vc1.dominates(&vc2));
}

#[test]
fn test_lww_register_convergence() {
    let clock_a = LamportClock::new(10, 1);
    let clock_b = LamportClock::new(15, 2);

    let mut reg_a = LwwRegister::new("Initial A", clock_a);
    let reg_b = LwwRegister::new("Newer B", clock_b);

    // Merging B into A must adopt B's value since clock_b > clock_a
    assert!(reg_a.merge(&reg_b));
    assert_eq!(*reg_a.get(), "Newer B");

    // Merging A back into B must be a no-op
    let mut reg_b_clone = reg_b.clone();
    assert!(!reg_b_clone.merge(&reg_a));
    assert_eq!(*reg_b_clone.get(), "Newer B");
}

#[test]
fn test_pn_counter_convergence() {
    let mut node1 = PnCounter::new();
    let mut node2 = PnCounter::new();

    node1.inc(1, 10);
    node1.dec(1, 2); // Net +8

    node2.inc(2, 5);
    node2.dec(2, 1); // Net +4

    assert_eq!(node1.value(), 8);
    assert_eq!(node2.value(), 4);

    // Merge node2 into node1 and vice versa
    node1.merge(&node2);
    node2.merge(&node1);

    // Both must converge to 8 + 4 = 12
    assert_eq!(node1.value(), 12);
    assert_eq!(node2.value(), 12);
}

#[test]
fn test_or_set_add_remove() {
    let mut set_a = OrSet::new();
    let mut set_b = OrSet::new();

    let tag1 = LamportClock::new(1, 1);
    let tag2 = LamportClock::new(2, 2);

    set_a.add("Item 1", tag1);
    set_a.add("Item 2", tag2);
    assert!(set_a.contains(&"Item 1"));
    assert!(set_a.contains(&"Item 2"));

    // Remove Item 1 on node A
    set_a.remove(&"Item 1");
    assert!(!set_a.contains(&"Item 1"));
    assert!(set_a.contains(&"Item 2"));

    // Merge into B
    set_b.merge(&set_a);
    assert!(!set_b.contains(&"Item 1"));
    assert!(set_b.contains(&"Item 2"));
}

#[test]
fn test_rga_collaborative_sequence() {
    let mut seq = RgaSequence::new();
    let id1 = LamportClock::new(1, 1);
    let id2 = LamportClock::new(2, 1);
    let id3 = LamportClock::new(3, 1);

    seq.insert(None, id1, 'H');
    seq.insert(Some(id1), id2, 'i');
    seq.insert(Some(id2), id3, '!');

    assert_eq!(seq.read(), vec!['H', 'i', '!']);

    // Delete '!'
    assert!(seq.delete(id3));
    assert_eq!(seq.read(), vec!['H', 'i']);
}

#[test]
fn test_replicated_signal_reactivity() {
    let clock = LamportClock::new(1, 1);
    let signal = ReplicatedSignal::new("Alpha".to_string(), clock);

    let notify_count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&notify_count);

    let sub_id = signal.subscribe(move || {
        count_clone.fetch_add(1, Ordering::SeqCst);
    });

    assert_eq!(signal.get(), "Alpha");

    // Setting a newer value triggers subscriber
    let clock2 = LamportClock::new(2, 1);
    assert!(signal.set("Beta".to_string(), clock2));
    assert_eq!(signal.get(), "Beta");
    assert_eq!(notify_count.load(Ordering::SeqCst), 1);

    // Stale timestamp does not update or notify
    let stale_clock = LamportClock::new(1, 1);
    assert!(!signal.set("Stale".to_string(), stale_clock));
    assert_eq!(signal.get(), "Beta");
    assert_eq!(notify_count.load(Ordering::SeqCst), 1);

    // Unsubscribe
    signal.unsubscribe(sub_id);
    let clock3 = LamportClock::new(3, 1);
    signal.set("Gamma".to_string(), clock3);
    assert_eq!(notify_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_sync_message_binary_roundtrip() {
    let msg1 = SyncMessage::RegisterDelta {
        clock: LamportClock::new(42, 7),
        key: "telemetry.altitude".to_string(),
        payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    let encoded1 = msg1.encode();
    let decoded1 = SyncMessage::decode(&encoded1).expect("Decode msg1 failed");
    assert_eq!(msg1, decoded1);

    let msg2 = SyncMessage::CounterDelta {
        peer: 99,
        positive: 1000,
        negative: 250,
    };
    let encoded2 = msg2.encode();
    let decoded2 = SyncMessage::decode(&encoded2).expect("Decode msg2 failed");
    assert_eq!(msg2, decoded2);

    let msg3 = SyncMessage::Heartbeat {
        clock: LamportClock::new(1234, 56),
    };
    let encoded3 = msg3.encode();
    let decoded3 = SyncMessage::decode(&encoded3).expect("Decode msg3 failed");
    assert_eq!(msg3, decoded3);
}

#[test]
fn test_crdt_signal_store_local_and_remote_reactivity() {
    let store_a = CrdtSignalStore::new("Initial State".to_string(), 1);
    assert_eq!(store_a.get(), "Initial State");

    // Bind an effect to store_a's reactive signal
    let effect_updates = Arc::new(AtomicUsize::new(0));
    let last_observed = Arc::new(std::sync::Mutex::new(String::new()));
    let sig = store_a.signal().clone();
    let eff_updates = Arc::clone(&effect_updates);
    let eff_last = Arc::clone(&last_observed);

    let _effect = use_effect(move || {
        let val = sig.get();
        eff_updates.fetch_add(1, Ordering::SeqCst);
        *eff_last.lock().unwrap() = val;
        None
    });

    assert_eq!(effect_updates.load(Ordering::SeqCst), 1);
    assert_eq!(*last_observed.lock().unwrap(), "Initial State");

    // Local mutation updates signal and fires effect
    assert!(store_a.set("Local Update".to_string()));
    assert_eq!(store_a.get(), "Local Update");
    assert_eq!(effect_updates.load(Ordering::SeqCst), 2);
    assert_eq!(*last_observed.lock().unwrap(), "Local Update");

    // Remote merge with newer timestamp updates signal and fires effect
    let remote_clock = LamportClock::new(100, 2);
    let remote_reg = LwwRegister::new("Remote Dominant State".to_string(), remote_clock);
    assert!(store_a.merge_remote(&remote_reg));
    assert_eq!(store_a.get(), "Remote Dominant State");
    assert_eq!(effect_updates.load(Ordering::SeqCst), 3);
    assert_eq!(*last_observed.lock().unwrap(), "Remote Dominant State");

    // Stale remote merge does not update signal or fire effect
    let stale_clock = LamportClock::new(50, 3);
    let stale_reg = LwwRegister::new("Stale State".to_string(), stale_clock);
    assert!(!store_a.merge_remote(&stale_reg));
    assert_eq!(store_a.get(), "Remote Dominant State");
    assert_eq!(effect_updates.load(Ordering::SeqCst), 3);
}

#[test]
fn test_crdt_counter_store_reactivity() {
    let counter_store = CrdtCounterStore::new(1);
    assert_eq!(counter_store.value(), 0);

    let sig = counter_store.signal().clone();
    let memo = use_memo(move || sig.get() * 10);
    assert_eq!(memo.get(), 0);

    counter_store.inc(5);
    assert_eq!(counter_store.value(), 5);
    assert_eq!(memo.get(), 50);

    counter_store.dec(2);
    assert_eq!(counter_store.value(), 3);
    assert_eq!(memo.get(), 30);

    // Merge from remote peer
    let mut remote_counter = PnCounter::new();
    remote_counter.inc(2, 20);
    counter_store.merge(&remote_counter);
    assert_eq!(counter_store.value(), 23);
    assert_eq!(memo.get(), 230);
}
