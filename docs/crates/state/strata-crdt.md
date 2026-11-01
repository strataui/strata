# strata-crdt

Conflict-Free Replicated Data Types (CRDTs) and causal ordering primitives for local-first synchronization and distributed cloud telemetry across Project Strata nodes.

---

## Architectural Topology

The `strata-crdt` crate provides mathematically verified, state-based and operation-based synchronization primitives designed for multi-region edge clouds, distributed telemetry meshes, and peer-to-peer UI collaboration. By eliminating central coordinator locks and consensus latency penalties, `strata-crdt` enables edge nodes to mutate state locally with immediate UI reactivity, guaranteeing eventual convergence when synchronizing over lossy, high-latency, or partitioned networks.

```
       +-------------------------------------------------------+
       |               Strata Reactive UI Layer                |
       |             (Fine-Grained Signals DAG)                |
       +---------------------------+---------------------------+
                                   |
                       Direct Signal Subscription
                                   v
       +-------------------------------------------------------+
       |                 ReplicatedSignal<T>                   |
       |           (LwwRegister + UI Event Dispatch)           |
       +---------------------------+---------------------------+
                                   |
                +------------------+------------------+
                |                                     |
                v                                     v
   +------------------------+             +------------------------+
   |   Conflict-Free Core   |             |   Logical Clocks       |
   | - LwwRegister<T>       |             | - LamportClock (Total) |
   | - OrSet<T>             |             | - VectorClock (Causal) |
   | - RgaSequence<T>       |             +------------------------+
   | - PnCounter            |                         |
   +------------+-----------+                         |
                |                                     |
                +------------------+------------------+
                                   |
                                   v
       +-------------------------------------------------------+
       |                  SyncMessage Layer                    |
       |         (Compact Binary Delta Serialization)          |
       +---------------------------+---------------------------+
                                   |
                  QUIC / WebSockets / Edge P2P Mesh
                                   v
                    Remote Edge Nodes & Datacenters
```

### Core Design Principles

1. **Deterministic Eventual Convergence**: State merge operations satisfy the mathematical properties of a join-semilattice: commutativity ($A \sqcup B = B \sqcup A$), associativity ($(A \sqcup B) \sqcup C = A \sqcup (B \sqcup C)$), and idempotence ($A \sqcup A = A$).
2. **Zero Coordination Mutexes**: Local operations execute in constant time $\mathcal{O}(1)$ without blocking on distributed locks or network roundtrips.
3. **Reactive Signal Integration**: The `ReplicatedSignal<T>` bridge connects CRDT state directly to the Strata fine-grained reactive graph, dispatching DOM micro-updates only when remote deltas advance causal state.
4. **Binary Wire Efficiency**: Zero-copy little-endian framing minimizes replication bandwidth across edge regions.

---

## Causality and Logical Clocks

Distributed systems lack a synchronized global wall clock. `strata-crdt` implements logical clock primitives in `clock.rs` to establish rigorous causal and total orderings.

### Lamport Clock

The `LamportClock` enforces a strict monotonic total ordering across events in the distributed cluster. Each clock instance consists of a 64-bit scalar counter and a 64-bit peer identifier:

```rust
use strata_crdt::{LamportClock, PeerId};

let mut local_clock = LamportClock::new(0, 101);

// Increment clock upon local state modification
let t1 = local_clock.tick();
assert_eq!(t1.counter, 1);
assert_eq!(t1.peer, 101);

// Witness remote incoming clock from peer 202
let remote_clock = LamportClock::new(5, 202);
local_clock.witness(remote_clock);
assert_eq!(local_clock.counter, 6);
```

#### Deterministic Ordering Resolution

The `Ord` implementation resolves concurrent collisions deterministically using the node's unique `PeerId`:

$$\text{Clock}_A < \text{Clock}_B \iff (\text{counter}_A < \text{counter}_B) \lor (\text{counter}_A = \text{counter}_B \land \text{peer}_A < \text{peer}_B)$$

### Vector Clock

The `VectorClock` tracks causal dependencies across multi-peer topologies, detecting whether two events are causally related or concurrent:

```rust
use strata_crdt::VectorClock;

let mut node_a = VectorClock::new();
let mut node_b = VectorClock::new();

node_a.inc(1);
node_a.inc(1);
node_b.inc(2);

// Neither dominates: events occurred concurrently
assert!(!node_a.dominates(&node_b));
assert!(!node_b.dominates(&node_a));

// Join states
node_a.merge(&node_b);
assert!(node_a.dominates(&node_b));
```

The dominance predicate $\text{dominates}(V_1, V_2)$ evaluates to `true` if and only if $\forall p \in \text{Peers}, V_1[p] \ge V_2[p]$.

---

## Conflict-Free Replicated Primitives

### Last-Write-Wins Register (`LwwRegister<T>`)

The `LwwRegister<T>` wraps any cloneable value and pairs it with a `LamportClock`. When updates or remote states are received, the value with the higher timestamp supersedes earlier values.

```rust
use strata_crdt::{LamportClock, LwwRegister};

// Edge node in us-east-1
let clock_east = LamportClock::new(10, 1);
let mut reg_east = LwwRegister::new("cluster-healthy", clock_east);

// Edge node in eu-west-1 with newer clock
let clock_west = LamportClock::new(14, 2);
let reg_west = LwwRegister::new("cluster-degraded", clock_west);

// Merge converges deterministically to the newer state
assert!(reg_east.merge(&reg_west));
assert_eq!(*reg_east.get(), "cluster-degraded");

// Stale updates are rejected
let stale_clock = LamportClock::new(8, 3);
assert!(!reg_east.set("cluster-offline", stale_clock));
assert_eq!(*reg_east.get(), "cluster-degraded");
```

### Positive-Negative Counter (`PnCounter`)

The `PnCounter` supports distributed increments and decrements without coordination. It maintains two separate component maps per peer: positive additions $P$ and negative subtractions $N$.

```rust
use strata_crdt::PnCounter;

let mut us_east = PnCounter::new();
let mut eu_central = PnCounter::new();

// us-east scales active worker instances up by 10 and down by 2 (net +8)
us_east.inc(1, 10);
us_east.dec(1, 2);

// eu-central scales active worker instances up by 5 and down by 1 (net +4)
eu_central.inc(2, 5);
eu_central.dec(2, 1);

// Converge across regions
us_east.merge(&eu_central);
eu_central.merge(&us_east);

assert_eq!(us_east.value(), 12);
assert_eq!(eu_central.value(), 12);
```

#### Merge Semantics

The merge operation computes the component-wise supremum:

$$P_{\text{merged}}[p] = \max(P_1[p], P_2[p]), \quad N_{\text{merged}}[p] = \max(N_1[p], N_2[p])$$

$$\text{Value} = \sum_{p} P_{\text{merged}}[p] - \sum_{p} N_{\text{merged}}[p]$$

### Observed-Remove Set (`OrSet<T>`)

The `OrSet<T>` solves the classic distributed set problem where an element can be repeatedly added and removed concurrently. It tags every add operation with a unique `LamportClock`. When an element is removed, all observed add-tags are added to a tombstone set.

```rust
use strata_crdt::{LamportClock, OrSet};

let mut node_mesh_a = OrSet::new();
let mut node_mesh_b = OrSet::new();

let t1 = LamportClock::new(1, 1);
let t2 = LamportClock::new(2, 1);

// Node A registers active microservice instances
node_mesh_a.add("api-gateway-01".to_string(), t1);
node_mesh_a.add("telemetry-collector-04".to_string(), t2);

// Replicate to Node B
node_mesh_b.merge(&node_mesh_a);
assert!(node_mesh_b.contains(&"api-gateway-01".to_string()));

// Node A decommissions api-gateway-01
node_mesh_a.remove(&"api-gateway-01".to_string());
assert!(!node_mesh_a.contains(&"api-gateway-01".to_string()));

// Node B merges deletion state
node_mesh_b.merge(&node_mesh_a);
assert!(!node_mesh_b.contains(&"api-gateway-01".to_string()));
assert!(node_mesh_b.contains(&"telemetry-collector-04".to_string()));
```

### Replicated Growable Array (`RgaSequence<T>`)

The `RgaSequence<T>` provides collaborative sequence and text editing. Elements are stored as `RgaNode<T>` items linked through explicit predecessor references and ordered by unique timestamp tags.

```rust
use strata_crdt::{LamportClock, RgaSequence};

let mut log_stream = RgaSequence::new();
let id1 = LamportClock::new(1, 1);
let id2 = LamportClock::new(2, 1);
let id3 = LamportClock::new(3, 1);

// Append structured audit log entries
log_stream.insert(None, id1, "POD_INIT");
log_stream.insert(Some(id1), id2, "CONFIG_APPLIED");
log_stream.insert(Some(id2), id3, "PORT_BIND_8080");

assert_eq!(log_stream.read(), vec!["POD_INIT", "CONFIG_APPLIED", "PORT_BIND_8080"]);

// Soft-tombstone an invalidated log entry
log_stream.delete(id2);
assert_eq!(log_stream.read(), vec!["POD_INIT", "PORT_BIND_8080"]);
```

---

## Reactive UI Integration: `ReplicatedSignal<T>`

The `ReplicatedSignal<T>` bridges distributed CRDT synchronization with Strata's reactive rendering architecture. It wraps an internal `Arc<Mutex<LwwRegister<T>>>` and notifies UI subscribers only when a local mutation or remote merge strictly advances the state's logical clock.

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use strata_crdt::{LamportClock, ReplicatedSignal};

let clock = LamportClock::new(1, 100);
let signal = ReplicatedSignal::new("INIT".to_string(), clock);

let render_counter = Arc::new(AtomicUsize::new(0));
let counter_clone = Arc::clone(&render_counter);

// UI subscriber registered by component
let sub_id = signal.subscribe(move || {
    counter_clone.fetch_add(1, Ordering::SeqCst);
});

// Setting a newer timestamp updates value and fires notification
let clock2 = LamportClock::new(2, 100);
signal.set("PROCESSING".to_string(), clock2);
assert_eq!(signal.get(), "PROCESSING");
assert_eq!(render_counter.load(Ordering::SeqCst), 1);

// Out-of-order or duplicate timestamp is dropped without UI render churn
let clock_stale = LamportClock::new(1, 50);
assert!(!signal.set("STALE".to_string(), clock_stale));
assert_eq!(signal.get(), "PROCESSING");
assert_eq!(render_counter.load(Ordering::SeqCst), 1);

// Unsubscribe on component unmount
signal.unsubscribe(sub_id);
```

---

## Wire Protocol & Replication Framing

The `SyncMessage` enum provides a compact binary wire framing protocol for streaming state deltas over WebSocket, TCP, or QUIC connections.

### Binary Layout Specification

#### Type 1: RegisterDelta
- `byte[0]`: Message Type Tag (`0x01`)
- `byte[1..9]`: Lamport Clock Counter (`u64`, Little Endian)
- `byte[9..17]`: Lamport Clock Peer ID (`u64`, Little Endian)
- `byte[17..19]`: Key Length $K$ (`u16`, Little Endian)
- `byte[19..19+K]`: UTF-8 Key String
- `byte[19+K..23+K]`: Payload Length $L$ (`u32`, Little Endian)
- `byte[23+K..23+K+L]`: Binary Payload Bytes

#### Type 2: CounterDelta
- `byte[0]`: Message Type Tag (`0x02`)
- `byte[1..9]`: Peer ID (`u64`, Little Endian)
- `byte[9..17]`: Positive Count (`u64`, Little Endian)
- `byte[17..25]`: Negative Count (`u64`, Little Endian)

#### Type 3: Heartbeat
- `byte[0]`: Message Type Tag (`0x03`)
- `byte[1..9]`: Lamport Clock Counter (`u64`, Little Endian)
- `byte[9..17]`: Lamport Clock Peer ID (`u64`, Little Endian)

### Binary Encoding & Decoding Example

```rust
use strata_crdt::{LamportClock, SyncMessage};

let clock = LamportClock::new(128, 42);
let msg = SyncMessage::RegisterDelta {
    clock,
    key: "cluster.us-east.load_factor".to_string(),
    payload: vec![0x3F, 0x80, 0x00, 0x00], // f32 1.0f
};

// Serialize into compact binary buffer
let bytes = msg.encode();

// Transmit over wire and decode on remote peer
let decoded = SyncMessage::decode(&bytes).expect("Valid framing format");
assert_eq!(decoded, msg);
```

---

## Cloud Telemetry Case Study: Multi-Region Cluster State

Below is an end-to-end implementation of an edge observability coordinator synchronizing regional cluster metrics across distinct data centers:

```rust
use std::sync::{Arc, Mutex};
use strata_crdt::{LamportClock, LwwRegister, OrSet, PnCounter, SyncMessage};

pub struct RegionalClusterNode {
    pub region_id: u64,
    pub clock: Mutex<LamportClock>,
    pub status: Mutex<LwwRegister<String>>,
    pub active_nodes: Mutex<OrSet<String>>,
    pub request_counter: Mutex<PnCounter>,
}

impl RegionalClusterNode {
    pub fn new(region_id: u64, initial_status: &str) -> Self {
        let clock = LamportClock::new(0, region_id);
        Self {
            region_id,
            clock: Mutex::new(clock),
            status: Mutex::new(LwwRegister::new(initial_status.to_string(), clock)),
            active_nodes: Mutex::new(OrSet::new()),
            request_counter: Mutex::new(PnCounter::new()),
        }
    }

    pub fn record_ingress_requests(&self, count: u64) {
        self.request_counter.lock().unwrap().inc(self.region_id, count);
    }

    pub fn register_worker_node(&self, node_id: &str) {
        let mut clock = self.clock.lock().unwrap();
        let tag = clock.tick();
        self.active_nodes.lock().unwrap().add(node_id.to_string(), tag);
    }

    pub fn update_status(&self, new_status: &str) -> SyncMessage {
        let mut clock = self.clock.lock().unwrap();
        let tag = clock.tick();
        self.status.lock().unwrap().set(new_status.to_string(), tag);

        SyncMessage::RegisterDelta {
            clock: tag,
            key: format!("region.{}.status", self.region_id),
            payload: new_status.as_bytes().to_vec(),
        }
    }

    pub fn apply_remote_sync(&self, msg: &SyncMessage) {
        match msg {
            SyncMessage::RegisterDelta { clock, payload, .. } => {
                let mut local_clock = self.clock.lock().unwrap();
                local_clock.witness(*clock);
                if let Ok(status_str) = String::from_utf8(payload.clone()) {
                    let remote_reg = LwwRegister::new(status_str, *clock);
                    self.status.lock().unwrap().merge(&remote_reg);
                }
            }
            SyncMessage::CounterDelta { peer, positive, negative } => {
                let mut remote_counter = PnCounter::new();
                remote_counter.inc(*peer, *positive);
                remote_counter.dec(*peer, *negative);
                self.request_counter.lock().unwrap().merge(&remote_counter);
            }
            SyncMessage::Heartbeat { clock } => {
                self.clock.lock().unwrap().witness(*clock);
            }
        }
    }
}
```

---

## Verification & Semilattice Invariants

`strata-crdt` primitives are verified against standard semilattice properties:

| Property | Mathematical Invariant | Verification in `strata-crdt` |
| :--- | :--- | :--- |
| **Commutativity** | $A \sqcup B = B \sqcup A$ | Message delivery order over UDP/WebSocket does not alter converged state. |
| **Associativity** | $(A \sqcup B) \sqcup C = A \sqcup (B \sqcup C)$ | Regional intermediate batch merges produce identical outputs regardless of topology. |
| **Idempotence** | $A \sqcup A = A$ | Duplicate packet delivery or re-transmissions are safe no-ops. |
| **Monotonicity** | $t_1 \le t_2 \implies \text{State}(t_1) \le \text{State}(t_2)$ | Causally stale updates never overwrite newer local or remote state. |
