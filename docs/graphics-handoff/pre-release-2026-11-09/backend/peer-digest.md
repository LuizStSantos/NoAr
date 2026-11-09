# Peer challenge and resolution digest

Round A reports are in the parent directory. Review every finding and challenge both original scope and proposed fixes. Runtime checks are pending; no approval from source alone. B-010 public reachability was not established and zero-bindings state remains ambiguous. Reviewers must flag any unresolved high defect; parent will restore blocker=true if warranted.

- **B-001 HIGH: Manual field expansion allocation.** `backend/app/src/graphics/templates/graphics.rs`. Proposed resolution: Fields64/value1000/records16 and expanded256KiB bounds before string construction.
- **B-002 HIGH: Unbounded stale pending takes survive out/edit/delete/restart.** `backend/app/src/graphics.rs`. Proposed resolution: Bounded16 pending queue; RAII, lifecycle/edit generations and serialized publication.
- **B-003 HIGH: Optional bad permanent artwork blocks whole output startup.** `backend/app/src/player/output/playout.rs`. Proposed resolution: Isolate per-model asset failures, retainhealthy; DB errors propagate.
- **B-004 MEDIUM: Filtered image target uses unfiltered layer index.** `backend/app/src/graphics/templates/graphics.rs`. Proposed resolution: Mapper returns emitted spec and emitted primary index; renderer preserves index on omitted rasters.
- **B-005 MEDIUM: Temporary takes rasterize using unapplied output dimensions.** `backend/app/src/graphics.rs`. Proposed resolution: Uses actual running output size.
- **B-006 HIGH: Moving bound model logo rerasterizes on output thread.** `backend/engine/src/compositor/logo.rs`. Proposed resolution: Cached model translation with shared prepared layers; no native raster/I/O in move frame path.
- **B-007 HIGH: Native artwork preparation holds frame-critical control locks.** `backend/engine/src/compositor/lshape.rs`. Proposed resolution: Snapshot prepare outside lock then identity/generation validation; logo same pattern.
- **B-008 MEDIUM: Nested Lshape refresh resets motion epoch.** `backend/engine/src/compositor/lshape.rs`. Proposed resolution: Retains nested and role motion epochs.
- **B-009 MEDIUM: Bound logo refresh resets scroll epoch.** `backend/engine/src/compositor/logo.rs`. Proposed resolution: Retains own animated model epochs across replacement/refresh.
- **B-010 MEDIUM: Partial schema29/30 bindings without marker fail startup conversion.** `backend/app/src/db/handles/graphics/compatibility/conversion/graphics.rs`. Proposed resolution: Adopts surviving bindings/scenes, avoids frozen import. Zero bindings+zero scenes ambiguous. Historical public deletion unreachable basedoncheckedcallers; downgradefrominitialseverityconditional.
- **B-011 MEDIUM: Unchanged compatibility saves round authored fractional geometry.** `backend/app/src/db/handles/graphics/compatibility/graphics.rs`. Proposed resolution: Independent projected position/scale change checks; previous dimensions passed on output resize.
